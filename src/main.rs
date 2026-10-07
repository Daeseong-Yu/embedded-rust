#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts, dma,
    exti::{self, ExtiInput},
    gpio::{Level, Output, Pull, Speed},
    i2c::{self, I2c, Master},
    interrupt,
    mode::Async,
    peripherals,
    spi::{self, Spi},
    time::Hertz,
    usart::{Config, UartTx},
};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(
    struct Irqs {
        EXTI0 => exti::InterruptHandler<interrupt::typelevel::EXTI0>;
        DMA1_CHANNEL4 => dma::InterruptHandler<peripherals::DMA1_CH4>;
        I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
        I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
        DMA1_CHANNEL6 => dma::InterruptHandler<peripherals::DMA1_CH6>;
        DMA1_CHANNEL7 => dma::InterruptHandler<peripherals::DMA1_CH7>;
        DMA1_CHANNEL2 => dma::InterruptHandler<peripherals::DMA1_CH2>;
        DMA1_CHANNEL3 => dma::InterruptHandler<peripherals::DMA1_CH3>;
});

const DELAY_MS: [u64; 3] = [500, 200, 50];
const ACCEL_ADDR: u8 = 0x19;
const MAG_ADDR: u8 = 0x1E;
const WHO_AM_I_A: u8 = 0x0F;
const WHO_AM_I_M: u8 = 0x4F;
const GYRO_WHO_AM_I: u8 = 0x0F;
const SPI_READ: u8 = 0x80;

static BLINK_DELAY: Signal<CriticalSectionRawMutex, u64> = Signal::new();

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
async fn sensor_task(mut i2c: I2c<'static, Async, Master>) {
    let mut id = [0u8; 1];

    match i2c.write_read(ACCEL_ADDR, &[WHO_AM_I_A], &mut id).await {
        Ok(()) => info!("accel WHO_AM_I = {=u8:#x}", id[0]),
        Err(e) => error!("accel read failed: {}", e),
    }

    match i2c.write_read(MAG_ADDR, &[WHO_AM_I_M], &mut id).await {
        Ok(()) => info!("mag WHO_AM_I = {=u8:#x}", id[0]),
        Err(e) => error!("mag read failed: {}", e),
    }
}

#[embassy_executor::task]
async fn gyro_task(mut spi: Spi<'static, Async, spi::mode::Master>, mut cs: Output<'static>) {
    let mut buf = [GYRO_WHO_AM_I | SPI_READ, 0x00];

    cs.set_low();
    let result = spi.transfer_in_place(&mut buf).await;
    cs.set_high();

    match result {
        Ok(()) => info!("gyro WHO_AM_I = {=u8:#x}", buf[1]),
        Err(e) => error!("gyro read failed: {}", e),
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
    // Uart
    let tx = UartTx::new(p.USART1, p.PC4, p.DMA1_CH4, Irqs, config).unwrap();

    // I2C
    let i2c = I2c::new(
        p.I2C1,
        p.PB6,
        p.PB7,
        p.DMA1_CH6,
        p.DMA1_CH7,
        Irqs,
        i2c::Config::default(),
    );

    // SPI
    let mut spi_config = spi::Config::default();
    spi_config.mode = spi::MODE_3;
    spi_config.frequency = Hertz(1_000_000);
    let spi = Spi::new(
        p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA1_CH3, p.DMA1_CH2, Irqs, spi_config,
    );

    let cs = Output::new(p.PE3, Level::High, Speed::Low);

    spawner.spawn(uart_task(tx).unwrap());
    spawner.spawn(button_task(button).unwrap());
    spawner.spawn(led_task(led).unwrap());
    spawner.spawn(sensor_task(i2c).unwrap());
    spawner.spawn(gyro_task(spi, cs).unwrap());
}
