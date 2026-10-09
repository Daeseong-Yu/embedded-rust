use embedded_hal_async::spi::SpiDevice;

const WHO_AM_I: u8 = 0x0F;
const CTRL_REG1: u8 = 0x20;
const OUT_X_L: u8 = 0x28;

const GYRO_ID: u8 = 0xD3;
const READ: u8 = 0x80;
const AUTO_INCREMENT: u8 = 0x40;

#[derive(Debug, defmt::Format)]
pub enum Error<E> {
    Bus(E),
    WrongId(u8),
}

#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct Gyro {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

pub struct I3g4250d<SPI> {
    spi: SPI,
}

impl<SPI: SpiDevice> I3g4250d<SPI> {
    pub async fn new(spi: SPI) -> Result<Self, Error<SPI::Error>> {
        let mut this = Self { spi };

        let id = this.read_reg(WHO_AM_I).await?;
        if id != GYRO_ID {
            return Err(Error::WrongId(id));
        }

        this.write_reg(CTRL_REG1, 0x0F).await?;

        Ok(this)
    }

    pub async fn read(&mut self) -> Result<Gyro, Error<SPI::Error>> {
        let mut buf = [0u8; 7];
        buf[0] = OUT_X_L | READ | AUTO_INCREMENT;
        self.spi
            .transfer_in_place(&mut buf)
            .await
            .map_err(Error::Bus)?;

        let to_mdps = |lo, hi| i32::from(i16::from_le_bytes([lo, hi])) * 875 / 100;

        Ok(Gyro {
            x: to_mdps(buf[1], buf[2]),
            y: to_mdps(buf[3], buf[4]),
            z: to_mdps(buf[5], buf[6]),
        })
    }

    async fn read_reg(&mut self, reg: u8) -> Result<u8, Error<SPI::Error>> {
        let mut buf = [reg | READ, 0x00];
        self.spi
            .transfer_in_place(&mut buf)
            .await
            .map_err(Error::Bus)?;
        Ok(buf[1])
    }

    async fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), Error<SPI::Error>> {
        self.spi.write(&[reg, value]).await.map_err(Error::Bus)
    }
}
