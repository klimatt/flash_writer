#![no_std]
#![no_main]

use cortex_m as _;
use cortex_m_rt as rt;

use stm32g4xx_hal as hal;

use hal::prelude::*;
use hal::stm32::*;
use hal::timer::*;
use rtic::app;
use rt::entry;
use stm32g4xx_hal::gpio::GpioExt;
use stm32g4xx_hal::rcc::RccExt;
use rtt_target::{rprintln, rtt_init_print};
use hal::stm32::interrupt::*;



#[app(device = stm32g4xx_hal::stm32, peripherals = true)]
const APP: () = {
    struct Resources {
       usr_led: hal::gpio::gpioa::PA5<Output<PushPull>>,
       tim: Timer<TIM16>
    }
    #[init]
    fn init(ctx: init::Context) -> init::LateResources {
        rtt_init_print!();
        let dp : hal::stm32::Peripherals= ctx.device;
        let mut rcc = dp.RCC.freeze(hal::rcc::Config::pll());
        let gpioa = dp.GPIOA.split(&mut rcc);
        let mut usr_led = gpioa.pa5.into_push_pull_output();
        let mut tim = dp.TIM16.timer(&mut rcc);
        tim.start(1000.ms());
        tim.listen();
        init::LateResources {
           usr_led,
            tim
        }

    }
    #[idle(resources = [])]
    fn idle(ctx: idle::Context) -> ! {
        loop {
            cortex_m::asm::delay(10_000);
            rprintln!("idle!");
            cortex_m::asm::delay(10_000);
        }

    }

    #[task(binds = TIM1_UP_TIM16, priority = 2 , resources = [usr_led, tim])]
    fn tim_irq(ctx: tim_irq::Context){
        rprintln!("Timer_Work!");
        let led = ctx.resources.usr_led;
        let tim = ctx.resources.tim;
        tim.clear_irq();
        led.toggle().unwrap();
    }
};


use core::panic::PanicInfo;
use core::sync::atomic::{self, Ordering};
use core::borrow::{BorrowMut, Borrow};
use stm32g4xx_hal::gpio::{Output, PushPull};
use stm32g4xx_hal::time::U32Ext;

#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rprintln!("Panic: {:?}", info);
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}