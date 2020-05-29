#![no_std]
#![no_main]

extern crate panic_halt;
use lazy_static::lazy_static;

use core::cell::RefCell;
use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f3::stm32f303::{self, interrupt, Interrupt, GPIOC, TIM2};

static mut TIME_FRAME: [u8; 60] = [0; 60];
static mut TIME_FRAME_IX: usize = 0;
lazy_static! {
    static ref MUTEX_GPIOC: Mutex<RefCell<Option<GPIOC>>> =
        Mutex::new(RefCell::new(None));
    static ref MUTEX_TIM2: Mutex<RefCell<Option<TIM2>>> =
        Mutex::new(RefCell::new(None));
}

#[entry]
fn main() -> ! {
    let stm_perip = stm32f303::Peripherals::take().unwrap();
    let mut cortex_perip = cortex_m::Peripherals::take().unwrap();
    let rcc = &stm_perip.RCC;
    let gpioc = &stm_perip.GPIOC;

    // Enable the clock on GPIOC.
    rcc.ahbenr.write(|w| w.iopcen().enabled());

    // Configure PC5 on the board to be an floating input.
    gpioc.moder.write(|w| w.moder5().input());
    gpioc.pupdr.write(|w| unsafe { w.pupdr5().bits(0b00) });

    // Configure TIM2 to use as an interrupt for sampling the WWVB signal.
    configure_clock(&stm_perip);

    cortex_m::interrupt::free(|cs| {
        MUTEX_GPIOC.borrow(cs).replace(Some(stm_perip.GPIOC));
        MUTEX_TIM2.borrow(cs).replace(Some(stm_perip.TIM2));
    });

    // Enable the NVIC interrupt for TIM2.
    let nvic = &mut cortex_perip.NVIC;
    unsafe {
        NVIC::unmask(Interrupt::TIM2);
        nvic.set_priority(Interrupt::TIM2, 0x01);
    }
    NVIC::unpend(Interrupt::TIM2);

    unsafe {
        cortex_m::interrupt::enable();
    }

    loop {}
}

fn configure_clock(stm_perip: &stm32f303::Peripherals) {
    let tim2 = &stm_perip.TIM2;
    let rcc = &stm_perip.RCC;

    // Disable TIM2 before we start configuring.
    tim2.cr1.write(|w| w.cen().disabled());

    // Enable APB1_CLOCK going to TIM2.
    rcc.apb1enr.write(|w| w.tim2en().enabled());

    // Reset TIM2.
    rcc.apb1rstr
        .write(|w| w.tim2rst().set_bit().tim2rst().clear_bit());

    // The APB1_CLOCK that drives TIM2 runs at 8MHz by default, so prescale
    // down to 1kHz and then sample every 100ms.
    tim2.psc.write(|w| w.psc().bits(7_999));
    tim2.arr.write(|w| w.arr().bits(100));

    // Apply the settings and reset the timer.
    tim2.egr.write(|w| w.ug().update());

    // Enable an update interrupt for TIM2.
    tim2.dier.write(|w| w.uie().enabled());

    // Re-enable TIM2.
    tim2.cr1.write(|w| w.cen().enabled());
}

#[interrupt]
fn TIM2() {
    static mut BYTES: [u8; 10] = [0; 10];
    static mut INDEX: usize = 0;

    let value = cortex_m::interrupt::free(|cs| {
        let tim2 = MUTEX_TIM2.borrow(cs).borrow();
        let tim2 = tim2.as_ref()?;
        tim2.sr.write(|w| w.uif().clear());
        let refcell = MUTEX_GPIOC.borrow(cs).borrow();
        let gpioc = refcell.as_ref()?;
        Some(gpioc.idr.read().idr5().bit())
    });

    BYTES[*INDEX] = if value.unwrap() { 1 } else { 0 };
    if *INDEX >= 9 {
        *INDEX = 0;
        let byte = match BYTES {
            [0, 0, 0, 0, 0, 1, 1, 1, 1, 1] => {
                hprintln!("Got a 1 bit").unwrap();
                1
            }
            [0, 0, 1, 1, 1, 1, 1, 1, 1, 1] => {
                hprintln!("Got a 0 bit").unwrap();
                0
            }
            _ => {
                hprintln!("Unknown byte sequence").unwrap();
                0
            }
        };
    } else {
        *INDEX += 1;
    }
}
