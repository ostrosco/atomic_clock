#![no_std]
#![no_main]

extern crate panic_halt; 

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f3::stm32f303::{self, interrupt, Interrupt};

static mut TIME_FRAME: [u8; 60] = [0; 60];

#[entry]
fn main() -> ! {
    let stm_perip = stm32f303::Peripherals::take().unwrap();
    let mut cortex_perip = cortex_m::Peripherals::take().unwrap();
    configure_clock(&mut cortex_perip, &stm_perip);
    let rcc = &stm_perip.RCC;
    let gpioc = &stm_perip.GPIOC;
    let _tim2 = &stm_perip.TIM2;

    // Enable the clock on GPIOC.
    rcc.ahbenr.write(|w| w.iopcen().enabled());

    // Configure PC5 on the board to be an floating input.
    gpioc.moder.write(|w| w.moder5().input());
    gpioc.pupdr.write(|w| unsafe { w.pupdr5().bits(0b00) });

    unsafe {
        cortex_m::interrupt::enable();
    }

    loop {}
}

fn configure_clock(
    cortex_perip: &mut cortex_m::Peripherals,
    stm_perip: &stm32f303::Peripherals,
) {
    let tim2 = &stm_perip.TIM2;
    let rcc = &stm_perip.RCC;
    let nvic = &mut cortex_perip.NVIC;

    // Disable TIM2 before we start configuring.
    tim2.cr1.write(|w| w.cen().disabled());

    // Enable APB1_CLOCK going to TIM2.
    rcc.apb1enr.write(|w| w.tim2en().enabled());

    // Enable the NVIC interrupt for TIM2.
    unsafe {
        NVIC::unmask(Interrupt::TIM2);
        nvic.set_priority(Interrupt::TIM2, 0x01);
    }
    NVIC::unpend(Interrupt::TIM2);

    // Reset TIM2.
    rcc.apb1rstr
        .write(|w| w.tim2rst().set_bit().tim2rst().clear_bit());

    // The APB1_CLOCK that drives TIM2 runs at 8MHz by default, so prescale
    
    // down to 1kHz and then sample every 100ms.
    tim2.psc.write(|w| unsafe { w.psc().bits(7_999) });
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

    hprintln!("Time fired?").unwrap();
}
