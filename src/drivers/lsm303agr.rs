use embedded_hal_async::i2c::I2c;

const ACCEL_ADDR: u8 = 0x19;
const MAG_ADDR: u8 = 0x1E;

const WHO_AM_I_A: u8 = 0x0F;
const WHO_AM_I_M: u8 = 0x4F;

const CFG_REG_A_M: u8 = 0x60;
const CFG_REG_C_M: u8 = 0x62;

const CTRL_REG1_A: u8 = 0x20;
const CTRL_REG4_A: u8 = 0x23;
const OUT_X_L_A: u8 = 0x28;
const OUTX_L_REG_M: u8 = 0x68;

const ACCEL_ID: u8 = 0x33;
const MAG_ID: u8 = 0x40;
// Set on the register address to auto-increment during a multi-byte read.
const AUTO_INCREMENT: u8 = 0x80;

#[derive(Debug, defmt::Format)]
pub enum Error<E> {
    Bus(E),
    WrongId(u8),
}

// Acceleration
#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct Accel {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}
#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct Mag {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

pub struct Lsm303agr<I2C> {
    i2c: I2C,
}

impl<I2C: I2c> Lsm303agr<I2C> {
    pub async fn new(i2c: I2C) -> Result<Self, Error<I2C::Error>> {
        let mut this = Self { i2c };

        let id = this.read_reg(ACCEL_ADDR, WHO_AM_I_A).await?;
        if id != ACCEL_ID {
            return Err(Error::WrongId(id));
        }

        // 100 Hz, normal power, X/Y/Z enabled
        this.write_reg(ACCEL_ADDR, CTRL_REG1_A, 0x57).await?;
        // Block data update
        this.write_reg(ACCEL_ADDR, CTRL_REG4_A, 0x88).await?;

        let id = this.read_reg(MAG_ADDR, WHO_AM_I_M).await?;
        if id != MAG_ID {
            return Err(Error::WrongId(id));
        }

        this.write_reg(MAG_ADDR, CFG_REG_A_M, 0x80).await?;
        this.write_reg(MAG_ADDR, CFG_REG_C_M, 0x10).await?;

        Ok(this)
    }

    pub async fn read_accel(&mut self) -> Result<Accel, Error<I2C::Error>> {
        let mut buf = [0u8; 6];
        self.i2c
            .write_read(ACCEL_ADDR, &[OUT_X_L_A | AUTO_INCREMENT], &mut buf)
            .await
            .map_err(Error::Bus)?;

        Ok(Accel {
            x: i16::from_le_bytes([buf[0], buf[1]]) >> 4,
            y: i16::from_le_bytes([buf[2], buf[3]]) >> 4,
            z: i16::from_le_bytes([buf[4], buf[5]]) >> 4,
        })
    }

    pub async fn read_mag(&mut self) -> Result<Mag, Error<I2C::Error>> {
        let mut buf = [0u8; 6];
        self.i2c
            .write_read(MAG_ADDR, &[OUTX_L_REG_M | AUTO_INCREMENT], &mut buf)
            .await
            .map_err(Error::Bus)?;

        let to_mgauss = |lo, hi| i32::from(i16::from_le_bytes([lo, hi])) * 3 / 2;
        Ok(Mag {
            x: to_mgauss(buf[0], buf[1]),
            y: to_mgauss(buf[2], buf[3]),
            z: to_mgauss(buf[4], buf[5]),
        })
    }

    async fn read_reg(&mut self, addr: u8, reg: u8) -> Result<u8, Error<I2C::Error>> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(addr, &[reg], &mut buf)
            .await
            .map_err(Error::Bus)?;

        Ok(buf[0])
    }

    async fn write_reg(&mut self, addr: u8, reg: u8, value: u8) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(addr, &[reg, value])
            .await
            .map_err(Error::Bus)
    }
}
