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

use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

use core::sync::atomic::{AtomicU32, Ordering};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};

use protocol::{Frame, MAX_FRAME_LEN, Message};

mod drivers;
use drivers::{i3g4250d::I3g4250d, lsm303agr::Lsm303agr};
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
type GyroSpi = ExclusiveDevice<Spi<'static, Async, spi::mode::Master>, Output<'static>, NoDelay>;

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

static BLINK_DELAY: Signal<CriticalSectionRawMutex, u64> = Signal::new();
/// Samples waiting to be framed and sent over UART.
static TELEMETRY: Channel<CriticalSectionRawMutex, Message, 16> = Channel::new();
/// Samples dropped because `TELEMETRY` was full.
static DROPPED: AtomicU32 = AtomicU32::new(0);

/// Queue a sample without waiting. Sampling must keep its period, so a full
/// queue drops the sample and counts it instead of blocking.
fn publish(msg: Message) {
    if TELEMETRY.try_send(msg).is_err() {
        DROPPED.fetch_add(1, Ordering::Relaxed);
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

#[embassy_executor::task]
async fn uart_task(mut tx: UartTx<'static, Async>) {
    let mut seq: u16 = 0;
    let mut buf = [0u8; MAX_FRAME_LEN];

    loop {
        let msg = TELEMETRY.receive().await;
        let frame = Frame { seq, msg };
        seq = seq.wrapping_add(1);

        let n = match protocol::encode(&frame, &mut buf) {
            Ok(n) => n,
            Err(e) => {
                error!("encode failed: {}", Debug2Format(&e));
                continue;
            }
        };

        if let Err(e) = tx.write(&buf[..n]).await {
            error!("uart write failed: {}", e);
        }

        if seq.is_multiple_of(100) {
            info!(
                "uart: seq={} dropped={}",
                seq,
                DROPPED.load(Ordering::Relaxed)
            );
        }
    }
}

#[embassy_executor::task]
async fn sensor_task(i2c: I2c<'static, Async, Master>) {
    let mut sensor = match Lsm303agr::new(i2c).await {
        Ok(sensor) => sensor,
        Err(e) => {
            error!("lsm303agr init failed: {}", e);
            return;
        }
    };

    loop {
        match sensor.read_accel().await {
            Ok(a) => publish(Message::Accel {
                x: a.x,
                y: a.y,
                z: a.z,
            }),
            Err(e) => error!("accel read failed: {}", e),
        }

        match sensor.read_mag().await {
            Ok(m) => publish(Message::Mag {
                x: m.x,
                y: m.y,
                z: m.z,
            }),
            Err(e) => error!("mag read failed: {}", e),
        }
        Timer::after_millis(100).await;
    }
}

#[embassy_executor::task]
async fn gyro_task(spi: GyroSpi) {
    let mut gyro = match I3g4250d::new(spi).await {
        Ok(gyro) => gyro,
        Err(e) => {
            error!("i3g4250d init failed: {}", e);
            return;
        }
    };

    loop {
        match gyro.read().await {
            Ok(g) => publish(Message::Gyro {
                x: g.x,
                y: g.y,
                z: g.z,
            }),
            Err(e) => error!("gyro read failed: {}", e),
        }

        Timer::after_millis(100).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("start, protocol v{}", protocol::VERSION);

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
    let gyro_spi = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
    spawner.spawn(gyro_task(gyro_spi).unwrap());
}
