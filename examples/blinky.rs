#![deny(warnings)]
#![deny(unsafe_code)]
#![no_main]
#![no_std]

use cortex_m as _;
use cortex_m_rt as rt;
use panic_halt as _;

use stm32g4xx_hal as hal;

use crate::hal::prelude::*;
use crate::hal::stm32;
use rt::entry;
use stm32g4xx_hal::gpio::GpioExt;
use stm32g4xx_hal::rcc::RccExt;
use rtt_target::{rprintln, rtt_init_print};

#[entry]
fn main() -> ! {
    rtt_init_print!();
    let dp = stm32::Peripherals::take().expect("cannot take peripherals");
    let mut rcc = dp.RCC.freeze(hal::rcc::Config::pll());
    let gpioa = dp.GPIOA.split(&mut rcc);
    let mut led = gpioa.pa5.into_push_pull_output();

    loop {
        for _ in 0..1_00_000 {
            led.set_low().unwrap();
        }
        rprintln!("Hello fro new code!!!");
        for _ in 0..1_00_000 {
            led.set_high().unwrap();
        }
    }
}
