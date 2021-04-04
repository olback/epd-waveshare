//! A simple Driver for the Waveshare 7.5" E-Ink Display (V2) via SPI
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/wiki/7.5inch_e-Paper_HAT)
//! - [Waveshare C driver](https://github.com/waveshare/e-Paper/blob/702def0/RaspberryPi%26JetsonNano/c/lib/e-Paper/EPD_7in5_V2.c)
//! - [Waveshare Python driver](https://github.com/waveshare/e-Paper/blob/702def0/RaspberryPi%26JetsonNano/python/lib/waveshare_epd/epd7in5_V2.py)
//!
//! Important note for V2:
//! Revision V2 has been released on 2019.11, the resolution is upgraded to 800×480, from 640×384 of V1.
//! The hardware and interface of V2 are compatible with V1, however, the related software should be updated.

use embedded_hal::{
    blocking::{delay::*, spi::Write},
    digital::v2::{InputPin, OutputPin},
};

use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLUT, WaveshareDisplay};

pub(crate) mod command;
use self::command::Command;

#[cfg(feature = "graphics")]
mod graphics;
#[cfg(feature = "graphics")]
pub use self::graphics::Display7in5;

/// Width of the display
pub const WIDTH: u32 = 800;
/// Height of the display
pub const HEIGHT: u32 = 480;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = true;

/// EPD7in5 (V2) driver
///
pub struct EPD7in5<SPI, CS, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, CS, BUSY, DC, RST, DELAY>,
    /// Background Color
    color: Color,
}

impl<SPI, CS, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, CS, BUSY, DC, RST, DELAY>
    for EPD7in5<SPI, CS, BUSY, DC, RST, DELAY>
where
    SPI: Write<u8>,
    CS: OutputPin,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayMs<u8>,
{
    fn init(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        // Reset the device
        self.interface.reset(2);

        // V2 procedure as described here:
        // https://github.com/waveshare/e-Paper/blob/master/RaspberryPi%26JetsonNano/python/lib/waveshare_epd/epd7in5bc_V2.py
        // and as per specs:
        // https://www.waveshare.com/w/upload/6/60/7.5inch_e-Paper_V2_Specification.pdf

        self.cmd_with_data(spi, Command::BOOSTER_SOFT_START, &[0x17, 0x17, 0x27, 0x17])?;
        self.cmd_with_data(spi, Command::POWER_SETTING, &[0x07, 0x17, 0x3F, 0x3F])?;
        self.command(spi, Command::POWER_ON)?;
        self.wait_until_idle(spi);
        self.cmd_with_data(spi, Command::PANEL_SETTING, &[0x1F])?;
        self.cmd_with_data(spi, Command::PLL_CONTROL, &[0x06])?;
        self.cmd_with_data(spi, Command::TCON_RESOLUTION, &[0x03, 0x20, 0x01, 0xE0])?;
        self.cmd_with_data(spi, Command::DUAL_SPI, &[0x00])?;
        self.cmd_with_data(spi, Command::TCON_SETTING, &[0x22])?;
        self.cmd_with_data(spi, Command::VCOM_AND_DATA_INTERVAL_SETTING, &[0x10, 0x07])?;
        self.wait_until_idle(spi);
        Ok(())
    }
}

impl<SPI, CS, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, CS, BUSY, DC, RST, DELAY>
    for EPD7in5<SPI, CS, BUSY, DC, RST, DELAY>
where
    SPI: Write<u8>,
    CS: OutputPin,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayMs<u8>,
{
    type DisplayColor = Color;
    fn new(
        spi: &mut SPI,
        cs: CS,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: DELAY,
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(cs, busy, dc, rst, delay);
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = EPD7in5 { interface, color };

        epd.init(spi)?;

        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.init(spi)
    }

    fn sleep(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi);
        self.command(spi, Command::POWER_OFF)?;
        self.wait_until_idle(spi);
        self.cmd_with_data(spi, Command::DEEP_SLEEP, &[0xA5])?;
        Ok(())
    }

    fn update_frame(&mut self, spi: &mut SPI, buffer: &[u8]) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi);
        self.cmd_with_data(spi, Command::DATA_START_TRANSMISSION_2, buffer)?;
        Ok(())
    }

    fn update_partial_frame(
        &mut self,
        _spi: &mut SPI,
        _buffer: &[u8],
        _x: u32,
        _y: u32,
        _width: u32,
        _height: u32,
    ) -> Result<(), SPI::Error> {
        unimplemented!();
    }

    fn display_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi);
        self.command(spi, Command::DISPLAY_REFRESH)?;
        Ok(())
    }

    fn update_and_display_frame(&mut self, spi: &mut SPI, buffer: &[u8]) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer)?;
        self.command(spi, Command::DISPLAY_REFRESH)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi);
        self.send_resolution(spi)?;

        self.command(spi, Command::DATA_START_TRANSMISSION_1)?;
        self.interface.data_x_times(spi, 0x00, WIDTH * HEIGHT / 8)?;

        self.command(spi, Command::DATA_START_TRANSMISSION_2)?;
        self.interface.data_x_times(spi, 0x00, WIDTH * HEIGHT / 8)?;

        self.command(spi, Command::DISPLAY_REFRESH)?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Color) {
        self.color = color;
    }

    fn background_color(&self) -> &Color {
        &self.color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _refresh_rate: Option<RefreshLUT>,
    ) -> Result<(), SPI::Error> {
        unimplemented!();
    }

    fn is_busy(&self) -> bool {
        self.interface.is_busy(IS_BUSY_LOW)
    }
}

impl<SPI, CS, BUSY, DC, RST, DELAY> EPD7in5<SPI, CS, BUSY, DC, RST, DELAY>
where
    SPI: Write<u8>,
    CS: OutputPin,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayMs<u8>,
{
    fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command)
    }

    fn send_data(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        self.interface.data(spi, data)
    }

    fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data)
    }

    fn wait_until_idle(&mut self, spi: &mut SPI) {
        self.interface
            .wait_until_idle(spi, Command::GET_STATUS, IS_BUSY_LOW)
    }

    fn send_resolution(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        let w = self.width();
        let h = self.height();

        self.command(spi, Command::TCON_RESOLUTION)?;
        self.send_data(spi, &[(w >> 8) as u8])?;
        self.send_data(spi, &[w as u8])?;
        self.send_data(spi, &[(h >> 8) as u8])?;
        self.send_data(spi, &[h as u8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 800);
        assert_eq!(HEIGHT, 480);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
