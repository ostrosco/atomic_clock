#![no_std]
#![no_main]

extern crate panic_halt;

pub mod consts;
pub mod rtc;
pub mod wwvb;

use crate::wwvb::WWVBError;
use core::cell::RefCell;
use core::ops::DerefMut;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f1xx_hal::{
    device::NVIC,
    pac::{self, interrupt, Interrupt},
    prelude::*,
    pwm_input::*,
    rtc::Rtc,
    time::KiloHertz,
    timer::Timer,
};

#[derive(PartialEq)]
enum SyncState {
    NotSynced,
    FirstSync,
    Synced,
}

static RTC: Mutex<RefCell<Option<Rtc>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let perip = pac::Peripherals::take().unwrap();
    let cortex_perip = cortex_m::Peripherals::take().unwrap();
    let mut flash = perip.FLASH.constrain();
    let mut rcc = perip.RCC.constrain();

    // Set the vector table offset register to point to the start of FLASh for this board.
    unsafe {
        cortex_perip.SCB.vtor.write(0x08000000);
    }

    let clocks = rcc
        .cfgr
        .use_hse(8.mhz())
        .sysclk(48.mhz())
        .pclk1(24.mhz())
        .freeze(&mut flash.acr);

    let mut afio = perip.AFIO.constrain(&mut rcc.apb2);
    let mut dbg = perip.DBGMCU;
    let mut pwr = perip.PWR;
    let mut backup_domain =
        rcc.bkp.constrain(perip.BKP, &mut rcc.apb1, &mut pwr);

    let mut rtc = Rtc::rtc(perip.RTC, &mut backup_domain);
    rtc.listen_alarm();

    let mut nvic = cortex_perip.NVIC;
    unsafe {
        nvic.set_priority(Interrupt::RTC, 3);
        NVIC::unmask(Interrupt::RTC);
    }
    NVIC::unpend(Interrupt::RTC);

    cortex_m::interrupt::free(|cs| {
        rtc.set_alarm(rtc.current_time() + 1);
        *RTC.borrow(cs).borrow_mut() = Some(rtc);
    });

    let gpioa = perip.GPIOA.split(&mut rcc.apb2);
    let gpiob = perip.GPIOB.split(&mut rcc.apb2);

    // We need to disable JTAG support here for PB4 so we can use it for PWM input capture.
    let (_pa15, _pb3, pb4) =
        afio.mapr.disable_jtag(gpioa.pa15, gpiob.pb3, gpiob.pb4);
    let pb5 = gpiob.pb5;

    let config: Configuration<KiloHertz> = Configuration::RawValues {
        arr: consts::ARR,
        presc: consts::PRESC,
    };
    let pwm_input = Timer::tim3(perip.TIM3, &clocks, &mut rcc.apb1).pwm_input(
        (pb4, pb5),
        &mut afio.mapr,
        &mut dbg,
        config,
    );

    let mut frame = [0u16; 60];
    let mut slice_ix = 0;
    let mut sync_state = SyncState::NotSynced;

    loop {
        if let Ok((duty_cycle, period)) =
            pwm_input.read_duty(ReadMode::WaitForNextCapture)
        {
            // Correct for counter rollover when calculating the duty.
            let duty = if period < duty_cycle {
                period + consts::ARR - duty_cycle
            } else {
                period - duty_cycle
            };

            if sync_state == SyncState::Synced {
                match wwvb::handle_bit(duty, slice_ix) {
                    Ok(val) => {
                        frame[slice_ix] = val;
                        slice_ix += 1;
                    }
                    Err(WWVBError::InvalidSync) => {
                        hprintln!("Invalid sync received, resyncing").unwrap();
                        sync_state = SyncState::NotSynced;
                        frame = [0; 60];
                        slice_ix = 0;
                    }
                    Err(WWVBError::UnknownSignal) => {
                        hprintln!("Unknown signal received, resyncing")
                            .unwrap();
                        sync_state = SyncState::NotSynced;
                        frame = [0; 60];
                        slice_ix = 0;
                    }
                }
            } else {
                // WWVB will output two sync messages to indicate the start of a frame. Once we get
                // the second sync, save it into the frame as it's technically the first second of
                // data in the frame.
                if duty > consts::SYNC_MIN && duty < consts::SYNC_MAX {
                    if sync_state == SyncState::NotSynced {
                        sync_state = SyncState::FirstSync;
                    } else {
                        hprintln!("Synced to WWVB").unwrap();
                        sync_state = SyncState::Synced;
                        frame[0] = 2;
                        slice_ix = 1;
                    }
                } else {
                    sync_state = SyncState::NotSynced;
                }
            }
        }

        if slice_ix == 60 {
            let minute = wwvb::calc_minute(&frame);
            let hour = wwvb::calc_hour(&frame);
            let doy = wwvb::calc_doy(&frame);
            let year = wwvb::calc_year(&frame);
            let leap_year = wwvb::is_leap_year(&frame);
            let (date_year, _, _) = wwvb::to_date(year, doy, leap_year);

            if minute >= 60 || hour >= 24 || doy >= 367 || year >= 99 {
                hprintln!("Invalid minute received, resyncing.").unwrap();
                sync_state = SyncState::NotSynced;
                continue;
            } else {
                // Calculate the Unix timestamp. Keep in mind that since the WWVB frame we receive
                // is from the previous minute, we need to add 60 seconds to represent the _actual_
                // minute we're currently starting.
                let unix_ts = rtc::to_timestamp(date_year, doy, hour, minute);
                cortex_m::interrupt::free(|cs| {
                    if let Some(ref mut rtc) =
                        *RTC.borrow(cs).borrow_mut().deref_mut()
                    {
                        rtc.set_time(unix_ts);
                    }
                });
                hprintln!("Unix time: {}", unix_ts).unwrap();
            }

            frame = [0; 60];
            slice_ix = 0;
        }
    }
}

#[interrupt]
fn RTC() {
    cortex_m::interrupt::free(|cs| {
        if let Some(ref mut rtc) = *RTC.borrow(cs).borrow_mut().deref_mut() {
            rtc.set_alarm(rtc.current_time() + 1);
            rtc.clear_alarm_flag();
        }
    });
}
