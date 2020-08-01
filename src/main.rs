#![no_std]
#![no_main]

extern crate panic_halt;

pub mod consts;
pub mod rtc;
pub mod wwvb;

use crate::wwvb::WWVBError;
use core::cell::RefCell;
use core::fmt::Write;
use core::ops::DerefMut;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use nb::block;
use stm32f1xx_hal::{
    delay::Delay,
    device::NVIC,
    pac::{self, interrupt, Interrupt},
    prelude::*,
    pwm_input::*,
    rtc::Rtc,
    serial::{Config, Serial, Tx1},
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
static G_USART: Mutex<RefCell<Option<Tx1>>> = Mutex::new(RefCell::new(None));
static G_DELAY: Mutex<RefCell<Option<Delay>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let perip = pac::Peripherals::take().unwrap();
    let cortex_perip = cortex_m::Peripherals::take().unwrap();
    let mut flash = perip.FLASH.constrain();
    let mut rcc = perip.RCC.constrain();

    // Set the vector table offset register to point to the start of FLASH for this board.
    /*
    unsafe {
        cortex_perip.SCB.vtor.write(0x08000000);
    }
    */

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

    let mut gpioa = perip.GPIOA.split(&mut rcc.apb2);
    let gpiob = perip.GPIOB.split(&mut rcc.apb2);
    let tx = gpioa.pa9.into_alternate_push_pull(&mut gpioa.crh);
    let rx = gpioa.pa10;

    // We need to disable JTAG support here for PB4 so we can use it for PWM input capture.
    let (_pa15, _pb3, pb4) =
        afio.mapr.disable_jtag(gpioa.pa15, gpiob.pb3, gpiob.pb4);
    let pb5 = gpiob.pb5;

    // Configure the USART for talking to the display.
    let serial = Serial::usart1(
        perip.USART1,
        (tx, rx),
        &mut afio.mapr,
        Config::default().baudrate(9600.bps()),
        clocks,
        &mut rcc.apb2,
    );

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
    let mut delay = Delay::new(cortex_perip.SYST, clocks);

    let mut tx = serial.split().0;

    // Set the backlight brightness to 50%.
    send_cmd_to_display(b"\xfe\x53\x04", &mut tx, &mut delay, 100_u16);

    // Clear the display.
    send_cmd_to_display(b"\xfe\x51", &mut tx, &mut delay, 100_u16);


    // Configure the real time clock. Here we are using an alarm interrupt versus the seconds
    // interrupt. For some reason, enabling the seconds interrupt would cause the board to hand
    // indefinitely so we just set alarms to trigger every second.
    let mut rtc = Rtc::rtc(perip.RTC, &mut backup_domain);
    rtc.listen_alarm();
    let mut nvic = cortex_perip.NVIC;
    unsafe {
        nvic.set_priority(Interrupt::RTC, 3);
        NVIC::unmask(Interrupt::RTC);
    }
    NVIC::unpend(Interrupt::RTC);

    // Move the RTC and the USART into their appropriate mutexes and set up our first alarm.
    cortex_m::interrupt::free(|cs| {
        rtc.set_alarm(rtc.current_time() + 1);
        *RTC.borrow(cs).borrow_mut() = Some(rtc);
        *G_USART.borrow(cs).borrow_mut() = Some(tx);
        *G_DELAY.borrow(cs).borrow_mut() = Some(delay);
    });

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
                        hprintln!("Invalid sync received").unwrap();
                        sync_state = SyncState::NotSynced;
                        frame = [0; 60];
                        slice_ix = 0;
                    }
                    Err(WWVBError::UnknownSignal) => {
                        hprintln!("Unknown signal received").unwrap();
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
                sync_state = SyncState::NotSynced;
                continue;
            } else {
                // Calculate the Unix timestamp. Keep in mind that since the WWVB frame we receive
                // is from the previous minute, we need to add 60 seconds to represent the _actual_
                // minute we're currently starting.
                //
                // TODO: right now we're about a second off.
                let unix_ts =
                    rtc::to_timestamp(date_year, doy, hour, minute) + 60;
                cortex_m::interrupt::free(|cs| {
                    if let Some(ref mut rtc) =
                        *RTC.borrow(cs).borrow_mut().deref_mut()
                    {
                        rtc.set_time(unix_ts);
                        rtc.set_alarm(unix_ts + 1);
                    }
                });
            }

            frame = [0; 60];
            slice_ix = 0;
        }
    }
}

// Send a command to the New Haven Display.
fn send_cmd_to_display(
    command: &[u8],
    tx: &mut Tx1,
    delay: &mut Delay,
    delay_time: u16,
) {
    for byte in command {
        block!(tx.write(*byte)).ok();
    }
    delay.delay_us(delay_time);
}

#[interrupt]
fn RTC() {
    static mut USART: Option<Tx1> = None;
    static mut DELAY: Option<Delay> = None;
    let mut tx = USART.get_or_insert_with(|| {
        cortex_m::interrupt::free(|cs| {
            G_USART.borrow(cs).replace(None).unwrap()
        })
    });

    let mut delay = DELAY.get_or_insert_with(|| {
        cortex_m::interrupt::free(|cs| {
            G_DELAY.borrow(cs).replace(None).unwrap()
        })
    });

    cortex_m::interrupt::free(|cs| {
        if let Some(ref mut rtc) = *RTC.borrow(cs).borrow_mut().deref_mut() {
            // Write the current time to the display and set up the next second's alarm.
            let time = rtc.current_time();
            // Reset the cursor to the first position before drawing instead of clearing each time.
            send_cmd_to_display(b"\xfe\x45\x00", &mut tx, &mut delay, 100_u16);
            let (year, doy, hour, minute, second) = rtc::from_timestamp(time);
            write!(tx, "{:02}:{:02}:{:02}", hour, minute, second).unwrap();
            send_cmd_to_display(b"\xfe\x45\x40", &mut tx, &mut delay, 100_u16);
            write!(tx, "{}-{:03}", year, doy).unwrap();
            rtc.clear_alarm_flag();
            rtc.set_alarm(rtc.current_time() + 1);
        }
    });
}
