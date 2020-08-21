#![no_std]
#![no_main]

extern crate panic_halt;

pub mod consts;
pub mod time;
pub mod wwvb;

use crate::time::Timestamp;
use core::cell::RefCell;
use core::fmt::Write;
use core::ops::DerefMut;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::OutputPin;
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

// Mutex for access to the real-time clock. This is shared between the RTC interrupt and the main
// loop of the program.
static G_RTC: Mutex<RefCell<Option<Rtc>>> = Mutex::new(RefCell::new(None));

// Mutex for USART1. This is consumed when the first RTC interrupt fires.
static G_USART: Mutex<RefCell<Option<Tx1>>> = Mutex::new(RefCell::new(None));

// Mutex for the delay. This is consumed when the first RTC interrupt fires.
static G_DELAY: Mutex<RefCell<Option<Delay>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let perip = pac::Peripherals::take().unwrap();
    let cortex_perip = cortex_m::Peripherals::take().unwrap();
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
    let mut pwr = perip.PWR;
    let mut backup_domain =
        rcc.bkp.constrain(perip.BKP, &mut rcc.apb1, &mut pwr);

    let mut gpioa = perip.GPIOA.split(&mut rcc.apb2);
    let mut gpiob = perip.GPIOB.split(&mut rcc.apb2);
    let tx = gpioa.pa9.into_alternate_push_pull(&mut gpioa.crh);
    let rx = gpioa.pa10;

    // We need to disable JTAG support here for PB4 so we can use it for PWM input capture.
    let (_pa15, pb3, pb4) =
        afio.mapr.disable_jtag(gpioa.pa15, gpiob.pb3, gpiob.pb4);
    let pb5 = gpiob.pb5;

    // Control the PDN pin to trigger the fast startup on the WWVB receiver.
    let mut power_on = pb3.into_open_drain_output(&mut gpiob.crl);
    power_on.set_low().unwrap();

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
    // interrupt. For some reason, enabling the seconds interrupt would cause the board to hang
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
        *G_RTC.borrow(cs).borrow_mut() = Some(rtc);
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
                    // WWVB does not offer any error correction. So if we get an invalid pulse or
                    // we've missed a bit somehow, we can only throw away this frame, resync, and
                    // try again.
                    Err(_) => {
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
            if let Err(_) = set_rtc(&frame) {
                sync_state = SyncState::NotSynced;
                continue;
            }
            frame = [0; 60];
            slice_ix = 0;
        }
    }
}

fn set_rtc(frame: &[u16; 60]) -> Result<(), ()> {
    let minute = wwvb::calc_minute(&frame);
    let hour = wwvb::calc_hour(&frame);
    let doy = wwvb::calc_doy(&frame);
    let year = wwvb::calc_year(&frame);
    let leap_year = wwvb::is_leap_year(&frame);
    let (date_year, _, _) = wwvb::to_date(year, doy, leap_year);

    if minute >= 60 || hour >= 24 || doy >= 367 || year >= 99 {
        Err(())
    } else {
        let timestamp = Timestamp::new(date_year, doy, hour, minute, 0);

        cortex_m::interrupt::free(|cs| {
            if let Some(ref mut rtc) =
                *G_RTC.borrow(cs).borrow_mut().deref_mut()
            {
                // Calculate the Unix timestamp. Keep in mind that since the WWVB frame we
                // receive is from the previous minute, we need to add 60 seconds to represent
                // the actual minute we're currently starting. We're also behind one second
                // when doing this calculation so we compensate for that here.
                let unix_ts = timestamp.to_unix() + 61;
                rtc.set_time(unix_ts);
                rtc.set_alarm(unix_ts + 1);
            }
        });
        Ok(())
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
        if let Some(ref mut rtc) = *G_RTC.borrow(cs).borrow_mut().deref_mut() {
            // Write the current time to the display and set up the next second's alarm.
            let time = rtc.current_time();
            // Reset the cursor to the first position before drawing instead of clearing each time.
            let time = Timestamp::from_unix(time);
            let date = time.to_date();

            // Write the current time to the first line of the display and the current date to the
            // second line of the display.
            send_cmd_to_display(b"\xfe\x45\x00", &mut tx, &mut delay, 100_u16);
            write!(
                tx,
                "{:02}:{:02}:{:02}",
                time.hour, time.minute, time.seconds
            )
            .unwrap();
            send_cmd_to_display(b"\xfe\x45\x40", &mut tx, &mut delay, 100_u16);
            write!(tx, "{}-{:02}-{:02}", date.year, date.month, date.day)
                .unwrap();

            // Clear the interrupt flag and set another alarm to take place one second later.
            rtc.clear_alarm_flag();
            rtc.set_alarm(rtc.current_time() + 1);
        }
    });
}
