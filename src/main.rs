#![no_std]
#![no_main]

extern crate panic_halt;

pub mod consts;
pub mod wwvb;

use crate::wwvb::WWVBError;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f1xx_hal::{
    pac, prelude::*, pwm_input::*, time::KiloHertz, timer::Timer,
};

#[derive(PartialEq)]
enum SyncState {
    NotSynced,
    FirstSync,
    Synced,
}

#[entry]
fn main() -> ! {
    let perip = pac::Peripherals::take().unwrap();
    let mut flash = perip.FLASH.constrain();
    let mut rcc = perip.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(8.mhz())
        .sysclk(48.mhz())
        .pclk1(24.mhz())
        .freeze(&mut flash.acr);
    let mut afio = perip.AFIO.constrain(&mut rcc.apb2);
    let mut dbg = perip.DBGMCU;

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
            let (date_year, date_month, date_day) =
                wwvb::to_date(year, doy, leap_year);

            if minute >= 60 || hour >= 24 || doy >= 367 || year >= 99 {
                hprintln!("Invalid minute received, resyncing.").unwrap();
                sync_state = SyncState::NotSynced;
                continue;
            } else {
                // For now, we're just going to print out the date and time as we don't have a
                // display hooked up to the interface.
                hprintln!("Time: {:02}:{:02}", hour, minute,).unwrap();
                hprintln!("Date: {}-{}-{}", date_year, date_month, date_day)
                    .unwrap();
            }

            frame = [0; 60];
            slice_ix = 0;
        }
    }
}
