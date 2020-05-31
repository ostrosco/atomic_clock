#![no_std]
#![no_main]

extern crate panic_halt;
use lazy_static::lazy_static;

use core::cell::RefCell;
use cortex_m::{interrupt::Mutex, peripheral::NVIC};
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f3::stm32f303::{self, interrupt, Interrupt, RCC, TIM3};

lazy_static! {
    static ref MUTEX_TIM3: Mutex<RefCell<Option<TIM3>>> = Mutex::new(RefCell::new(None));
    static ref TIMECODE: Mutex<RefCell<[u8; 60]>> = Mutex::new(RefCell::new([0; 60]));
    static ref TIMECODE_IX: Mutex<RefCell<usize>> = Mutex::new(RefCell::new(0));
    static ref TIMECODE_READY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
}

#[entry]
fn main() -> ! {
    let stm_perip = stm32f303::Peripherals::take().unwrap();
    let mut cortex_perip = cortex_m::Peripherals::take().unwrap();
    let rcc = &stm_perip.RCC;
    let gpioa = &stm_perip.GPIOA;

    // configure_clock(&stm_perip.RCC);

    // Enable the clock on GPIOA.
    rcc.ahbenr.modify(|_, w| w.iopaen().enabled());

    // Configure PA5 on the board to be TIM3 channel 1 input.
    gpioa.afrl.modify(|_, w| w.afrl6().af2());
    gpioa.pupdr.modify(|_, w| unsafe { w.pupdr6().bits(0b10) });
    gpioa.moder.modify(|_, w| w.moder6().alternate());

    // Configure TIM3 to use as an interrupt for sampling the WWVB signal.
    configure_timer(&stm_perip.TIM3, &stm_perip.RCC);

    /*
    hprintln!("AFRL:  {:032b}", gpioa.afrl.read().bits()).unwrap();
    hprintln!("MODER: {:032b}", gpioa.moder.read().bits()).unwrap();
    hprintln!("PUPDR: {:032b}", gpioa.pupdr.read().bits()).unwrap();
    */

    cortex_m::interrupt::free(|cs| {
        MUTEX_TIM3.borrow(cs).replace(Some(stm_perip.TIM3));
    });

    // Enable the NVIC interrupt for TIM3.
    let nvic = &mut cortex_perip.NVIC;
    unsafe {
        NVIC::unmask(Interrupt::TIM3);
        nvic.set_priority(Interrupt::TIM3, 0x01);
    }
    NVIC::unpend(Interrupt::TIM3);

    unsafe {
        cortex_m::interrupt::enable();
    }

    loop {
        cortex_m::interrupt::free(|cs| {
            let timecode = TIMECODE.borrow(cs).borrow();
            let mut timecode_ready = TIMECODE_READY.borrow(cs).borrow_mut();
            if *timecode_ready {
                hprintln!("{:?}", &timecode[0..10]).unwrap();
                hprintln!("{:?}", &timecode[10..20]).unwrap();
                hprintln!("{:?}", &timecode[20..30]).unwrap();
                hprintln!("{:?}", &timecode[30..40]).unwrap();
                hprintln!("{:?}", &timecode[40..50]).unwrap();
                hprintln!("{:?}", &timecode[50..60]).unwrap();
            }
            *timecode_ready = false;
        });
    }
}

fn configure_clock(rcc: &RCC) {
    // Turn on the HSE and wait for it to be ready.
    rcc.cr.modify(|_, w| w.hseon().on());

    while rcc.cr.read().hserdy().is_not_ready() {}

    rcc.cfgr.modify(|_, w| {
        w.pllsrc()
            .hse_div_prediv()
            .pllmul()
            .mul6()
            .pllxtpre()
            .div2()
    });

    rcc.cr.modify(|_, w| w.pllon().on());

    while rcc.cr.read().pllrdy().is_not_ready() {}

    rcc.cfgr.modify(|_, w| w.sw().hse());

    rcc.cfgr.modify(|_, w| w.ppre1().div2());
}

fn configure_timer(tim3: &TIM3, rcc: &RCC) {
    // Disable TIM3 before we start configuring.
    tim3.cr1.modify(|_, w| w.cen().disabled().ckd().div4());

    // Enable APB1_CLOCK going to TIM3.
    rcc.apb1enr.modify(|_, w| w.tim3en().enabled());

    // Reset TIM3.
    rcc.apb1rstr
        .modify(|_, w| w.tim3rst().set_bit().tim3rst().clear_bit());

    tim3.psc.modify(|_, w| w.psc().bits(7999));
    tim3.arr.modify(|_, w| w.arr().bits(1000));
    tim3.egr.write(|w| w.ug().update());

    // Disable capture from the counter into the capture register.
    tim3.ccer
        .modify(|_, w| w.cc1e().clear_bit().cc2e().clear_bit());

    // Set up TIM3 to:
    // - perform input capture on
    // - wait until 8 consecutive samples are at the same level before triggering the interrupt
    tim3.ccmr1_input().modify(|_, w| {
        w.cc1s()
            .ti1()
            .ic1f()
            .bits(0b1111)
            .cc2s()
            .ti1()
            .ic2f()
            .bits(0b1111)
    });

    // Set up TIM3 to capture on rising edge only for testing.
    tim3.ccer.modify(|_, w| {
        w.cc1p()
            .clear_bit()
            .cc1np()
            .clear_bit()
            .cc2p()
            .set_bit()
            .cc2np()
            .clear_bit()
    });

    // Configure the slave controller to:
    // - select the valid trigger input to TI1FP1
    // - configure the slave mode controller to reset mode
    tim3.smcr.modify(|_, w| w.ts().ti1fp1().sms().reset_mode());

    // Enable capture from the counter into the capture register.
    tim3.ccer.modify(|_, w| w.cc1e().set_bit().cc2e().set_bit());

    // Enable an capture/compare interrupt for TIM3.
    tim3.dier
        .modify(|_, w| w.cc1ie().enabled().cc2ie().enabled());

    // Re-enable TIM3.
    tim3.cr1.modify(|_, w| w.cen().enabled());

    /*
    hprintln!("CCER: {:016b}", tim3.ccer.read().bits()).unwrap();
    hprintln!("CCMR: {:016b}", tim3.ccmr1_input().read().bits()).unwrap();
    hprintln!("DIER: {:016b}", tim3.dier.read().bits()).unwrap();
    hprintln!("CR1:  {:016b}", tim3.cr1.read().bits()).unwrap();
    */
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    Rising,
    Falling,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SyncState {
    NoSync,
    FirstSync,
    Sync,
}

#[interrupt]
fn TIM3() {
    static mut RISING_COUNT: u16 = 0;
    static mut FALLING_COUNT: u16 = 0;
    static mut PREV_EDGE: Option<Edge> = None;
    static mut SYNC_STATE: SyncState = SyncState::NoSync;

    let (edge, counter) = cortex_m::interrupt::free(|cs| {
        let tim3 = MUTEX_TIM3.borrow(cs).borrow();
        let tim3 = tim3.as_ref().unwrap();

        if tim3.sr.read().cc1if().bit_is_set() {
            return (Edge::Rising, tim3.ccr1.read().ccr().bits());
        } else if tim3.sr.read().cc2if().bit_is_set() {
            return (Edge::Falling, tim3.ccr2.read().ccr().bits());
        } else {
            panic!("Unknown interrupt trigger.");
        };
    });
    match *PREV_EDGE {
        Some(ref mut prev_edge) => {
            if edge == *prev_edge {
                hprintln!("Missed an edge!").unwrap();
                return;
            } else {
                *prev_edge = edge;
            }
        }
        None => *PREV_EDGE = Some(edge),
    }

    match edge {
        Edge::Rising => {
            *RISING_COUNT = counter;

            let pulse_width = if *FALLING_COUNT > *RISING_COUNT {
                *FALLING_COUNT - *RISING_COUNT
            } else {
                *FALLING_COUNT + 1000 - *RISING_COUNT
            };

            cortex_m::interrupt::free(|cs| {
                let mut timecode = TIMECODE.borrow(cs).borrow_mut();
                let mut timecode_ix = TIMECODE_IX.borrow(cs).borrow_mut();
                let mut timecode_ready = TIMECODE_READY.borrow(cs).borrow_mut();
                if *SYNC_STATE == SyncState::Sync {
                    if pulse_width >= 100 && pulse_width <= 300 {
                        timecode[*timecode_ix] = 2;
                        *timecode_ix += 1;
                    }
                    if pulse_width >= 400 && pulse_width <= 600 {
                        timecode[*timecode_ix] = 1;
                        *timecode_ix += 1;
                    } else if pulse_width >= 700 && pulse_width <= 900 {
                        timecode[*timecode_ix] = 0;
                        *timecode_ix += 1;
                    }

                    if *timecode_ix >= 60 {
                        *timecode_ready = true;
                        *timecode_ix = 0;
                    }
                } else {
                    if pulse_width >= 100 && pulse_width <= 300 {
                        if *SYNC_STATE == SyncState::NoSync {
                            *SYNC_STATE = SyncState::FirstSync;
                        } else {
                            hprintln!("Synced to WWVB").unwrap();
                            *SYNC_STATE = SyncState::Sync;
                            *timecode_ix = 0;
                        }
                    } else {
                        *SYNC_STATE = SyncState::NoSync;
                    }
                }
            });
        }
        Edge::Falling => {
            *FALLING_COUNT = counter;
        }
    }
}
