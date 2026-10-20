//! The board's 1.47" 172x320 ST7789 LCD, driven through ESP-IDF's esp_lcd
//! component, and the thread that redraws the status screens. The BOOT
//! button pages through them.
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

use crate::indicator::{self, Event, Indicator, State};
use crate::screen::{self, Frame, Page, System, View, HEIGHT, WIDTH};
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
/// The screen switches off after this long without a button press (it
/// warms the board, and nobody watches it all the time). A press wakes it.
const SCREEN_TIMEOUT: Duration = Duration::from_secs(60);
/// The board's RGB LED: one addressable (WS2812-style) LED on GPIO 38.
const PIN_LED: i32 = 38;
/// The BOOT button: GPIO 0, pulled up, low while pressed.
const PIN_BUTTON: i32 = 0;
/// How often the button is checked; also its debounce time.
const BUTTON_POLL: Duration = Duration::from_millis(30);
/// The SD card's free space is looked up this often (it can be slow).
const SD_INTERVAL: Duration = Duration::from_secs(30);
const SD_MOUNT: &str = "/sd";
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

    /// Switch the backlight and the panel on or off.
    pub fn set_on(&self, on: bool) -> Result<(), EspError> {
        unsafe {
            esp!(gpio_set_level(PIN_BACKLIGHT as _, on as u32))?;
            esp!(esp_lcd_panel_disp_on_off(self.panel, on))?;
        }
        Ok(())
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
    let mut page = Page::Overview;
    let button = Button::new();
    let mut cpu = CpuLoad::new();
    let thermometer = Thermometer::new();
    let mut last = (Instant::now(), 0u64, 0u64);
    let mut last_server = (0u64, 0u64, 0u64); // busy_us, block_us, blocks
    let mut block_ms = None;
    // When the SD card was last checked, and what it said.
    let mut sd: (Option<Instant>, Option<(u64, u64)>) = (None, None);
    let mut drawn: Option<Instant> = None;
    let mut last_press = Instant::now();
    let mut screen_on = true;
    let led = RgbLed::new();
    let mut indicator = Indicator::new();
    let mut shown: Option<[u8; 3]> = None;
    let mut ticked = Instant::now();
    // Counts already flashed for (joins, leaves, chats, problems).
    let mut seen = {
        let st = status::lock(&status);
        (st.joins, st.leaves, st.chats, status::problems())
    };
    loop {
        // The LED: dark while the screen is on (the screen says it all),
        // otherwise breathing and flashing (see indicator.rs).
        let (state, counts) = {
            let st = status::lock(&status);
            let state = if st.address.starts_with("Wi-Fi") {
                State::NoWifi
            } else {
                State::Running { players: st.players.len() }
            };
            (state, (st.joins, st.leaves, st.chats, status::problems()))
        };
        let news = [
            (counts.0 > seen.0, Event::Join),
            (counts.1 > seen.1, Event::Leave),
            (counts.2 > seen.2, Event::Chat),
            (counts.3 > seen.3, Event::Problem),
        ];
        seen = counts;
        let dt = ticked.elapsed();
        ticked = Instant::now();
        if let Some(led) = &led {
            let bytes = if screen_on {
                [0; 3]
            } else {
                for (new, e) in news {
                    if new {
                        indicator.event(e);
                    }
                }
                let c = indicator.colour(dt, state);
                [indicator::to_byte(c.0), indicator::to_byte(c.1), indicator::to_byte(c.2)]
            };
            if shown != Some(bytes) {
                led.set(bytes);
                shown = Some(bytes);
            }
        }

        let pressed = button.pressed();
        if pressed {
            last_press = Instant::now();
            if screen_on {
                page = page.next();
            } else {
                // The first press only wakes the screen, on the page it showed.
                screen_on = true;
                drawn = None;
                if let Err(e) = lcd.set_on(true) {
                    warn!("LCD on: {e}");
                }
                continue;
            }
        }
        if screen_on && last_press.elapsed() >= SCREEN_TIMEOUT {
            screen_on = false;
            if let Err(e) = lcd.set_on(false) {
                warn!("LCD off: {e}");
            }
        }
        if !screen_on {
            std::thread::sleep(BUTTON_POLL);
            continue;
        }
        if !pressed && drawn.is_some_and(|t| t.elapsed() < REFRESH) {
            std::thread::sleep(BUTTON_POLL);
            continue;
        }
        drawn = Some(Instant::now());

        let snapshot = status::lock(&status).clone();
        let now = Instant::now();
        let dt = now.duration_since(last.0).as_secs_f32().max(0.001);
        // Server load and block time since the last redraw.
        let busy = (snapshot.busy_us - last_server.0) as f32 / (dt * 1e6) * 100.0;
        let blocks = snapshot.blocks_built - last_server.2;
        if blocks > 0 {
            block_ms = Some((snapshot.block_us - last_server.1) as f32 / blocks as f32 / 1000.0);
        } else if block_ms.is_none() && snapshot.blocks_built > 0 {
            block_ms = Some(snapshot.block_us as f32 / snapshot.blocks_built as f32 / 1000.0);
        }
        last_server = (snapshot.busy_us, snapshot.block_us, snapshot.blocks_built);
        // Only while the Device page is up: the first look can mean reading
        // the card's whole allocation table, and the server waits for the
        // card meanwhile.
        if page == Page::Device && sd.0.map_or(true, |t| t.elapsed() >= SD_INTERVAL) {
            sd = (Some(Instant::now()), sd_space());
        }
        let (rssi, channel) = wifi();
        let sys = unsafe {
            System {
                internal_free_kb: heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT) as u32 / 1024,
                internal_min_kb: heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT) as u32 / 1024,
                internal_largest_kb: heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT) as u32 / 1024,
                psram_free_kb: heap_caps_get_free_size(MALLOC_CAP_SPIRAM) as u32 / 1024,
                psram_min_kb: heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM) as u32 / 1024,
                rssi,
                channel,
                packets_in_per_s: (snapshot.packets_in - last.1) as f32 / dt,
                packets_out_per_s: (snapshot.packets_out - last.2) as f32 / dt,
                cpu: cpu.sample(),
                server_busy: busy.clamp(0.0, 100.0),
                block_ms,
                sd: sd.1,
                chip_temp: thermometer.as_ref().and_then(Thermometer::read),
            }
        };
        last = (now, snapshot.packets_in, snapshot.packets_out);

        screen::render(&mut frame, &View { page, status: &snapshot, system: &sys });
        if let Err(e) = lcd.show(&frame) {
            warn!("LCD update failed: {e}");
        }
    }
}

/// The RGB LED, driven by the RMT peripheral: each bit is a high pulse,
/// short for 0 and long for 1, 24 bits per colour in green, red, blue order.
struct RgbLed {
    channel: rmt_channel_handle_t,
    encoder: rmt_encoder_handle_t,
}

impl RgbLed {
    /// RMT ticks per second: 0.1 us each.
    const RESOLUTION_HZ: u32 = 10_000_000;

    fn new() -> Option<Self> {
        // One RMT symbol: `high` ticks high, then `low` ticks low.
        let symbol = |high: u32, low: u32| rmt_symbol_word_t { val: high | 1 << 15 | low << 16 };
        let started = unsafe {
            let channel_config = rmt_tx_channel_config_t {
                gpio_num: PIN_LED,
                clk_src: soc_periph_rmt_clk_src_t_RMT_CLK_SRC_DEFAULT,
                resolution_hz: Self::RESOLUTION_HZ,
                mem_block_symbols: 48,
                trans_queue_depth: 2,
                ..Default::default()
            };
            let mut channel: rmt_channel_handle_t = null_mut();
            let mut encoder: rmt_encoder_handle_t = null_mut();
            // WS2812 timing: a 0 is 0.3 us high then 0.9 us low; a 1 the
            // other way round.
            let mut encoder_config = rmt_bytes_encoder_config_t {
                bit0: symbol(3, 9),
                bit1: symbol(9, 3),
                ..Default::default()
            };
            encoder_config.flags.set_msb_first(1);
            esp!(rmt_new_tx_channel(&channel_config, &mut channel))
                .and_then(|_| esp!(rmt_new_bytes_encoder(&encoder_config, &mut encoder)))
                .and_then(|_| esp!(rmt_enable(channel)))
                .map(|_| Self { channel, encoder })
        };
        match started {
            Ok(led) => {
                led.set([0; 3]); // it may light up at power-on
                Some(led)
            }
            Err(e) => {
                warn!("RGB LED not available: {e}");
                None
            }
        }
    }

    /// Show a colour: red, green, blue power, 0 to 255.
    fn set(&self, rgb: [u8; 3]) {
        let grb = [rgb[1], rgb[0], rgb[2]];
        let config = rmt_transmit_config_t::default();
        unsafe {
            if esp!(rmt_transmit(self.channel, self.encoder, grb.as_ptr() as *const _, grb.len(), &config)).is_ok() {
                // A few tens of microseconds; the buffer must outlive it.
                let _ = rmt_tx_wait_all_done(self.channel, 10);
            }
        }
    }
}

/// The BOOT button, with a press reported once (on going down).
struct Button {
    was_down: std::cell::Cell<bool>,
}

impl Button {
    fn new() -> Self {
        unsafe {
            gpio_reset_pin(PIN_BUTTON as _);
            gpio_set_direction(PIN_BUTTON as _, gpio_mode_t_GPIO_MODE_INPUT);
            gpio_set_pull_mode(PIN_BUTTON as _, gpio_pull_mode_t_GPIO_PULLUP_ONLY);
        }
        Self { was_down: std::cell::Cell::new(false) }
    }

    /// Whether the button has been pressed since the last call. Polled
    /// every BUTTON_POLL, which also rides out contact bounce.
    fn pressed(&self) -> bool {
        let down = unsafe { gpio_get_level(PIN_BUTTON as _) } == 0;
        let pressed = down && !self.was_down.get();
        self.was_down.set(down);
        pressed
    }
}

/// CPU load per core, from how long each core's FreeRTOS idle task ran
/// (needs CONFIG_FREERTOS_GENERATE_RUN_TIME_STATS; the run-time clock is
/// esp_timer, in microseconds).
struct CpuLoad {
    idle: [u32; 2],
    at: u64,
}

impl CpuLoad {
    fn new() -> Self {
        let mut c = Self { idle: [0; 2], at: 0 };
        c.sample();
        c
    }

    /// Load since the last call, in percent.
    fn sample(&mut self) -> [Option<f32>; 2] {
        let now = unsafe { esp_timer_get_time() } as u64;
        let mut tasks: Vec<TaskStatus_t> = Vec::with_capacity(32);
        let mut total: u32 = 0;
        let n = unsafe {
            let capacity = uxTaskGetNumberOfTasks() + 4;
            tasks.reserve(capacity as usize);
            let n = uxTaskGetSystemState(tasks.as_mut_ptr(), capacity, &mut total);
            tasks.set_len(n as usize);
            n
        };
        let mut idle = [None; 2];
        for t in &tasks[..n as usize] {
            let name = unsafe { std::ffi::CStr::from_ptr(t.pcTaskName) }.to_bytes();
            match name {
                b"IDLE0" => idle[0] = Some(t.ulRunTimeCounter as u32),
                b"IDLE1" => idle[1] = Some(t.ulRunTimeCounter as u32),
                _ => {}
            }
        }
        let elapsed = now.saturating_sub(self.at).max(1) as f32;
        let mut load = [None; 2];
        for core in 0..2 {
            if let Some(i) = idle[core] {
                let ran = i.wrapping_sub(self.idle[core]) as f32;
                if self.at != 0 {
                    load[core] = Some((100.0 - ran / elapsed * 100.0).clamp(0.0, 100.0));
                }
                self.idle[core] = i;
            }
        }
        self.at = now;
        load
    }
}

/// The ESP32-S3's built-in temperature sensor (on the chip itself, so it
/// reads warmer than the air around the board).
struct Thermometer {
    handle: temperature_sensor_handle_t,
}

impl Thermometer {
    fn new() -> Option<Self> {
        // The sensor has a few fixed ranges; 20 to 100 C suits a board that
        // runs warm.
        let config = temperature_sensor_config_t {
            range_min: 20,
            range_max: 100,
            // The clock source is left at 0, which the driver takes as its default.
            ..Default::default()
        };
        let mut handle: temperature_sensor_handle_t = null_mut();
        let started = unsafe {
            esp!(temperature_sensor_install(&config, &mut handle)).and_then(|_| esp!(temperature_sensor_enable(handle)))
        };
        match started {
            Ok(()) => Some(Self { handle }),
            Err(e) => {
                warn!("Temperature sensor not available: {e}");
                None
            }
        }
    }

    fn read(&self) -> Option<f32> {
        let mut celsius = 0.0f32;
        (unsafe { temperature_sensor_get_celsius(self.handle, &mut celsius) } == 0).then_some(celsius)
    }
}

/// The SD card's size and free space, in bytes.
fn sd_space() -> Option<(u64, u64)> {
    let path = std::ffi::CString::new(SD_MOUNT).ok()?;
    let (mut total, mut free) = (0u64, 0u64);
    (unsafe { esp_vfs_fat_info(path.as_ptr(), &mut total, &mut free) } == 0).then_some((total, free))
}

fn wifi() -> (Option<i8>, Option<u8>) {
    let mut ap: wifi_ap_record_t = Default::default();
    if unsafe { esp_wifi_sta_get_ap_info(&mut ap) } == 0 {
        (Some(ap.rssi), Some(ap.primary))
    } else {
        (None, None)
    }
}
