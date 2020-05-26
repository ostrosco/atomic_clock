#![no_std]
#![no_main]

// pick a panicking behavior
extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// extern crate panic_abort; // requires nightly
// extern crate panic_itm; // logs messages over ITM; requires ITM support
// extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f3::stm32f303;

#[entry]
fn main() -> ! {
    let perip = stm32f303::Peripherals::take().unwrap(); 
    let gpioc = &perip.GPIOC;
    gpioc.moder.write(|m| {
        m.moder5().input()
    });
    gpioc.pupdr.write(|p| {
        unsafe {
            p.pupdr5().bits(0b00)
        }
    });

    hprintln!("{:?}", gpioc.moder.read().moder5()).unwrap();

    loop {
        hprintln!("PC5: {}", gpioc.idr.read().idr5().bit()).unwrap();
    }
}
