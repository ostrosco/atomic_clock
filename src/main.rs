#![no_std]
#![no_main]

extern crate panic_halt;

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f1xx_hal::{
    pac, prelude::*, pwm_input::*, time::KiloHertz, timer::Timer,
};

// The sysclock is set to run at 48MHz, so we're prescaling down to a 10kHz clock and setting the
// counter to 10000.
const ARR: u16 = 10000;
const PRESC: u16 = 4799;

// Bounds for accepting a signal as one of the three values that WWVB can generate.
const SYNC_MIN: u16 = 1000;
const SYNC_MAX: u16 = 3000;
const ONE_MIN: u16 = 4000;
const ONE_MAX: u16 = 6000;
const ZERO_MIN: u16 = 7000;
const ZERO_MAX: u16 = 9000;

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

    // We're going to freeze the clock immediately. The target platform here is the STM32 Black
    // Pill which by default runs at 8MHz which is what we're expecting to see here.
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
        arr: ARR,
        presc: PRESC,
    };
    let pwm_input = Timer::tim3(perip.TIM3, &clocks, &mut rcc.apb1).pwm_input(
        (pb4, pb5),
        &mut afio.mapr,
        &mut dbg,
        config,
    );

    let mut time_slice = [0u16; 60];
    let mut slice_ix = 0;
    let mut sync_state = SyncState::NotSynced;

    loop {
        if let Ok((duty_cycle, period)) =
            pwm_input.read_duty(ReadMode::WaitForNextCapture)
        {
            // Correct for counter rollover when calculating the duty.
            let duty = if period < duty_cycle {
                period + ARR - duty_cycle
            } else {
                period - duty_cycle
            };

            if sync_state == SyncState::Synced {
                if duty > SYNC_MIN && duty < SYNC_MAX {
                    // We only expect sync signals at bits 0, 9, 19, 29, 39, 49, and 59. If we get
                    // a sync outside these bounds, we must have missed something and we should
                    // assume that our data is borked and need to resync.
                    if slice_ix == 0
                        || slice_ix == 9
                        || slice_ix == 19
                        || slice_ix == 29
                        || slice_ix == 39
                        || slice_ix == 49
                        || slice_ix == 59
                    {
                        time_slice[slice_ix] = 2;
                        slice_ix += 1;
                    } else {
                        hprintln!(
                            "Invalid sync received at {}, resyncing",
                            slice_ix
                        )
                        .unwrap();
                        sync_state = SyncState::NotSynced;
                        time_slice = [0; 60];
                        slice_ix = 0;
                    }
                } else if duty > ONE_MIN && duty < ONE_MAX {
                    time_slice[slice_ix] = 1;
                    slice_ix += 1;
                } else if duty > ZERO_MIN && duty < ZERO_MAX {
                    time_slice[slice_ix] = 0;
                    slice_ix += 1;
                } else {
                    // If we get something that we don't recognize, there's no error recovery we
                    // can do on the data. Consider this minute a wash and wait for the next sync.
                    hprintln!("Unknown pulse, waiting for resync").unwrap();
                    sync_state = SyncState::NotSynced;
                    slice_ix = 0;
                }
            } else {
                // WWVB will output two sync messages to indicate the start of a frame. Once we get
                // the second sync, save it into the frame as it's technically the first second of
                // data in the frame.
                if duty > SYNC_MIN && duty < SYNC_MAX {
                    if sync_state == SyncState::NotSynced {
                        sync_state = SyncState::FirstSync;
                    } else {
                        hprintln!("Synced to WWVB").unwrap();
                        sync_state = SyncState::Synced;
                        time_slice[0] = 2;
                        slice_ix = 1;
                    }
                } else {
                    sync_state = SyncState::NotSynced;
                }
            }
        }

        if slice_ix == 60 {
            let minute = 40 * time_slice[1]
                + 20 * time_slice[2]
                + 10 * time_slice[3]
                + 8 * time_slice[5]
                + 4 * time_slice[6]
                + 2 * time_slice[7]
                + time_slice[8];

            let hour = 20 * time_slice[12]
                + 10 * time_slice[13]
                + 8 * time_slice[15]
                + 4 * time_slice[16]
                + 2 * time_slice[17]
                + time_slice[18];

            let doy = 200 * time_slice[22]
                + 100 * time_slice[23]
                + 80 * time_slice[25]
                + 40 * time_slice[26]
                + 20 * time_slice[27]
                + 10 * time_slice[28]
                + 8 * time_slice[30]
                + 4 * time_slice[31]
                + 2 * time_slice[32]
                + time_slice[33];

            let year = 80 * time_slice[45]
                + 40 * time_slice[46]
                + 20 * time_slice[47]
                + 10 * time_slice[48]
                + 8 * time_slice[50]
                + 4 * time_slice[51]
                + 2 * time_slice[52]
                + time_slice[53];

            if minute >= 60 || hour >= 24 || doy >= 367 || year >= 99 {
                hprintln!("Invalid minute received, resyncing.").unwrap();
                sync_state = SyncState::NotSynced;
                continue;
            } else {
                hprintln!(
                    "Time: {:02}:{:02}, 20{}-{}",
                    hour,
                    minute,
                    year,
                    doy
                )
                .unwrap();
            }

            time_slice = [0; 60];
            slice_ix = 0;
        }
    }
}
