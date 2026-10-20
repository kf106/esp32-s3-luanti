//! The board's 1.47" 172x320 ST7789 LCD, driven through ESP-IDF's esp_lcd
//! component, and the thread that redraws the status screen.
//!
//! Pins (Waveshare ESP32-S3-LCD-1.47): MOSI 45, SCLK 40, CS 42, DC 41,
//! RST 39, backlight 48 (active high). The panel's 172 columns sit in the
//! middle of the ST7789's 240, hence the 34-pixel gap.

use std::ptr::null_mut;
use std::time::{Duration, Instant};

use esp_idf_svc::hal::gpio::{AnyIOPin, OutputPin};
use esp_idf_svc::hal::spi::{Dma, SpiAnyPins, SpiDriver, SpiDriverConfig};
use esp_idf_svc::sys::*;
use log::{info, warn};

use crate::screen::{self, Frame, System, HEIGHT, WIDTH};
use crate::status::{self, SharedStatus};

const PIN_CS: i32 = 42;
const PIN_DC: i32 = 41;
const PIN_RST: i32 = 39;
const PIN_BACKLIGHT: i32 = 48;
const PIXEL_CLOCK_HZ: u32 = 40_000_000;
/// Gap between the controller's 240 columns and the panel's 172.
const GAP: i32 = 34;
/// Landscape has two orientations, 180 degrees apart. If the text is upside
/// down, flip this.
const UPSIDE_DOWN: bool = false;

/// Rows sent per transfer, from a buffer in DMA-capable internal RAM.
const STRIP_ROWS: usize = 20;
const STRIP_BYTES: usize = WIDTH * STRIP_ROWS * 2;
const ST7789_NOP: i32 = 0x00;

const REFRESH: Duration = Duration::from_secs(1);
/// Measured peak use is about 3.5 KB; this leaves a wide margin.
const DISPLAY_STACK: usize = 12 * 1024;

pub struct Lcd {
    _spi: SpiDriver<'static>,
    io: esp_lcd_panel_io_handle_t,
    panel: esp_lcd_panel_handle_t,
    strip: *mut u8,
}

// The handles are only used from the display thread.
unsafe impl Send for Lcd {}

impl Lcd {
    pub fn new(
        spi: impl SpiAnyPins + 'static,
        sclk: impl OutputPin + 'static,
        mosi: impl OutputPin + 'static,
    ) -> Result<Self, EspError> {
        let spi = SpiDriver::new(
            spi,
            sclk,
            mosi,
            None::<AnyIOPin>,
            &SpiDriverConfig::new().dma(Dma::Auto(STRIP_BYTES)),
        )?;

        let io_config = esp_lcd_panel_io_spi_config_t {
            cs_gpio_num: PIN_CS,
            dc_gpio_num: PIN_DC,
            spi_mode: 0,
            pclk_hz: PIXEL_CLOCK_HZ,
            trans_queue_depth: 10,
            lcd_cmd_bits: 8,
            lcd_param_bits: 8,
            ..Default::default()
        };
        let mut io: esp_lcd_panel_io_handle_t = null_mut();
        esp!(unsafe { esp_lcd_new_panel_io_spi(spi.host() as _, &io_config, &mut io) })?;

        // RGB element order and big-endian data are the zero defaults.
        let panel_config = esp_lcd_panel_dev_config_t {
            reset_gpio_num: PIN_RST,
            bits_per_pixel: 16,
            ..Default::default()
        };
        let mut panel: esp_lcd_panel_handle_t = null_mut();
        esp!(unsafe { esp_lcd_new_panel_st7789(io, &panel_config, &mut panel) })?;
        unsafe {
            esp!(esp_lcd_panel_reset(panel))?;
            esp!(esp_lcd_panel_init(panel))?;
            // An IPS panel: colours are inverted unless told otherwise.
            esp!(esp_lcd_panel_invert_color(panel, true))?;
            // Landscape: swap axes plus one mirror is a 90 degree turn.
            esp!(esp_lcd_panel_swap_xy(panel, true))?;
            esp!(esp_lcd_panel_mirror(panel, !UPSIDE_DOWN, UPSIDE_DOWN))?;
            // After the swap, the 172-pixel axis is y.
            esp!(esp_lcd_panel_set_gap(panel, 0, GAP))?;
        }

        let strip = unsafe { heap_caps_malloc(STRIP_BYTES, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL) } as *mut u8;
        if strip.is_null() {
            return Err(EspError::from(ESP_ERR_NO_MEM as esp_err_t).unwrap());
        }
        let lcd = Self { _spi: spi, io, panel, strip };

        // Clear the screen before switching the display and backlight on,
        // so the panel's random power-up contents never show.
        lcd.show(&Frame::new())?;
        unsafe {
            esp!(esp_lcd_panel_disp_on_off(panel, true))?;
            esp!(gpio_reset_pin(PIN_BACKLIGHT as _))?;
            esp!(gpio_set_direction(PIN_BACKLIGHT as _, gpio_mode_t_GPIO_MODE_OUTPUT))?;
            esp!(gpio_set_level(PIN_BACKLIGHT as _, 1))?;
        }
        Ok(lcd)
    }

    /// Send a whole frame, a strip at a time.
    pub fn show(&self, frame: &Frame) -> Result<(), EspError> {
        let buf = unsafe { std::slice::from_raw_parts_mut(self.strip, STRIP_BYTES) };
        let mut y = 0;
        while y < HEIGHT {
            let rows = STRIP_ROWS.min(HEIGHT - y);
            frame.write_be(y..y + rows, &mut buf[..rows * WIDTH * 2]);
            unsafe {
                esp!(esp_lcd_panel_draw_bitmap(self.panel, 0, y as i32, WIDTH as i32, (y + rows) as i32, self.strip as *const _))?;
                // draw_bitmap only queues the transfer. Any command waits
                // for queued pixel data to finish first, so a NOP makes it
                // safe to refill the buffer.
                esp!(esp_lcd_panel_io_tx_param(self.io, ST7789_NOP, std::ptr::null(), 0))?;
            }
            y += rows;
        }
        Ok(())
    }
}

/// Start the LCD and a thread that redraws the status screen every second.
/// If the LCD can't be started, the server carries on without it.
pub fn start(
    status: SharedStatus,
    spi: impl SpiAnyPins + 'static,
    sclk: impl OutputPin + 'static,
    mosi: impl OutputPin + 'static,
) {
    let lcd = match Lcd::new(spi, sclk, mosi) {
        Ok(lcd) => lcd,
        Err(e) => {
            warn!("LCD not available: {e}");
            return;
        }
    };
    info!("LCD started");
    let spawned = std::thread::Builder::new()
        .name("display".into())
        .stack_size(DISPLAY_STACK)
        .spawn(move || run(lcd, status));
    if let Err(e) = spawned {
        warn!("Starting the display thread: {e}");
    }
}

fn run(lcd: Lcd, status: SharedStatus) {
    let mut frame = Frame::new();
    let mut last = (Instant::now(), 0u64, 0u64);
    loop {
        let snapshot = status::lock(&status).clone();
        let now = Instant::now();
        let dt = now.duration_since(last.0).as_secs_f32().max(0.001);
        let sys = System {
            internal_free_kb: unsafe { heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT) } as u32 / 1024,
            psram_free_kb: unsafe { heap_caps_get_free_size(MALLOC_CAP_SPIRAM) } as u32 / 1024,
            rssi: rssi(),
            packets_in_per_s: (snapshot.packets_in - last.1) as f32 / dt,
            packets_out_per_s: (snapshot.packets_out - last.2) as f32 / dt,
        };
        last = (now, snapshot.packets_in, snapshot.packets_out);

        screen::render(&mut frame, &snapshot, &sys);
        if let Err(e) = lcd.show(&frame) {
            warn!("LCD update failed: {e}");
        }
        std::thread::sleep(REFRESH.saturating_sub(now.elapsed()));
    }
}

fn rssi() -> Option<i8> {
    let mut ap: wifi_ap_record_t = Default::default();
    (unsafe { esp_wifi_sta_get_ap_info(&mut ap) } == 0).then_some(ap.rssi)
}
