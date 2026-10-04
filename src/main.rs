#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts, dma,
    exti::{self, ExtiInput},
    gpio::{Level, Output, Pull, Speed},
    interrupt,
    mode::Async,
    peripherals,
    usart::{Config, UartTx},
};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(
    struct Irqs {
        EXTI0 => exti::InterruptHandler<interrupt::typelevel::EXTI0>;
        DMA1_CHANNEL4 => dma::InterruptHandler<peripherals::DMA1_CH4>;
});

const DELAY_MS: [u64; 3] = [500, 200, 50];
static BLINK_DELAY: Signal<CriticalSectionRawMutex, u64> = Signal::new();

#[embassy_executor::task]
async fn uart_task(mut tx: UartTx<'static, Async>) {
    loop {
        match tx.write(b"hello\r\n").await {
            Ok(()) => info!("uart: sent"),
            Err(e) => error!("uart write failed: {}", e),
        }
        Timer::after_millis(1000).await;
    }
}

#[embassy_executor::task]
async fn led_task(mut led: Output<'static>) {
    let mut delay_ms = DELAY_MS[0];

    loop {
        if let Some(new_delay) = BLINK_DELAY.try_take() {
            delay_ms = new_delay;
            info!("blink delay changed: {} ms", delay_ms);
        }

        led.toggle();
        Timer::after_millis(delay_ms).await;
    }
}

#[embassy_executor::task]
async fn button_task(mut button: ExtiInput<'static, Async>) {
    let mut index = 0;

    loop {
        button.wait_for_rising_edge().await;
        index = (index + 1) % DELAY_MS.len();
        info!("button pressed");
        BLINK_DELAY.signal(DELAY_MS[index]);
        Timer::after_millis(30).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("start");

    let led = Output::new(p.PE9, Level::Low, Speed::Low);
    let button = ExtiInput::new(p.PA0, p.EXTI0, Pull::Down, Irqs);
    let mut config = Config::default();
    config.baudrate = 115_200;
    let tx = UartTx::new(p.USART1, p.PC4, p.DMA1_CH4, Irqs, config).unwrap();

    spawner.spawn(uart_task(tx).unwrap());
    spawner.spawn(button_task(button).unwrap());
    spawner.spawn(led_task(led).unwrap());
}
