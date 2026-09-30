#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;
use stm32f3::stm32f303;

#[entry]
fn main() -> ! {
    let peripherals = stm32f303::Peripherals::take().unwrap();

    // 1. Enable GPIOA and GPIOE clocks
    peripherals
        .RCC
        .ahbenr
        .modify(|_, w| w.iopaen().set_bit().iopeen().set_bit());
    // 2. Enable SYSCFG clock
    peripherals
        .RCC
        .apb2enr
        .modify(|_, w| w.syscfgen().set_bit());
    peripherals.SYSCFG.exticr1.modify(|_, w| w.exti0().pa0());

    // 3. Configure PAx
    // PA0 - input
    peripherals.GPIOA.moder.modify(|_, w| w.moder0().input());
    // PA0 - no pull-up, no pull-down
    peripherals.GPIOA.pupdr.modify(|_, w| w.pupdr0().floating());
    // PE9 - output
    peripherals.GPIOE.moder.modify(|_, w| w.moder9().output());

    loop {
        let pressed = peripherals.GPIOA.idr.read().idr0().bit_is_set();

        if pressed {
            peripherals.GPIOE.bsrr.write(|w| w.bs9().set_bit());
        } else {
            peripherals.GPIOE.bsrr.write(|w| w.br9().set_bit());
        }
    }
}
