mod auth;
mod content;
mod font;
mod inventory;
mod items;
mod lcd;
mod messages;
mod player;
mod protocol;
mod screen;
mod server;
mod sha1;
mod sha256;
mod skins;
mod srp;
mod status;
mod storage;
mod terrain;
mod world;
mod zstd;

use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::net::UdpSocket;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::fs::fatfs::Fatfs;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::sd::mmc::{SdMmcHostConfiguration, SdMmcHostDriver};
use esp_idf_svc::hal::sd::{SdCardConfiguration, SdCardDriver};
use esp_idf_svc::io::vfs::MountedFatfs;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::info;

const MOUNT: &str = "/sd";
const SSID: &str = env!("WIFI_SSID");
const PASS: &str = env!("WIFI_PASS");
const PORT: u16 = 30000;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log_heap("start");

    let p = Peripherals::take()?;
    let pins = p.pins;

    // SD card (Waveshare ESP32-S3-LCD-1.47 TF slot, 4-bit SDMMC)
    let host = SdMmcHostDriver::new_4bits(
        p.sdmmc1,
        pins.gpio15, // CMD
        pins.gpio14, // CLK
        pins.gpio16, // D0
        pins.gpio18, // D1
        pins.gpio17, // D2
        pins.gpio21, // D3
        None::<AnyIOPin>, // no card-detect line
        None::<AnyIOPin>, // no write-protect line
        &SdMmcHostConfiguration::new(),
    )?;
    let sd = SdCardDriver::new_mmc(host, &SdCardConfiguration::new())?;
    let fatfs = Fatfs::new_sdcard(0, sd)?;
    let _mounted = MountedFatfs::mount(fatfs, MOUNT, 4)?;
    info!("SD mounted at {MOUNT}");
    for entry in fs::read_dir(MOUNT)? {
        info!("  {:?}", entry?.file_name());
    }

    // Wi-Fi
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(p.modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().expect("SSID too long"),
        password: PASS.try_into().expect("password too long"),
        ..Default::default()
    }))?;
    wifi.start()?;
    info!("Connecting to {SSID}...");
    wifi.connect()?;
    wifi.wait_netif_up()?;
    let ip = wifi.wifi().sta_netif().get_ip_info()?.ip;
    info!("Connected, IP address {ip}");
    // Power saving makes the radio doze between beacons, which delays and
    // drops outgoing packets. A server should stay awake.
    esp_idf_svc::sys::esp!(unsafe {
        esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE)
    })?;
    log_heap("after Wi-Fi");

    // Luanti server
    let auth = auth::AuthStore::open(format!("{MOUNT}/auth"))?;
    let world = world::World::open(format!("{MOUNT}/world"))?;
    let inventories = inventory::InventoryStore::open(format!("{MOUNT}/players"))?;
    let players = player::PlayerStore::open(format!("{MOUNT}/players"))?;
    let media = content::Media::load(&format!("{MOUNT}/media"));
    let socket = UdpSocket::bind(("0.0.0.0", PORT))?;
    let status = status::Status::new(format!("{ip}:{PORT}"));
    lcd::start(status.clone(), p.spi2, pins.gpio40, pins.gpio45);
    let sent = std::sync::Arc::as_ptr(&status) as usize;
    let mut server = server::Server::new(server::Setup { socket, auth, world, inventories, players, media, status });
    // Guard against the compiler problem that once lost an argument on the
    // way into Server::new (see server::Setup).
    if server.status_data_addr() != sent {
        log::error!("Server was built with a corrupted status pointer; expect a crash");
    }
    log_heap("server ready");
    info!("Luanti server listening on {ip}:{PORT}");
    server.run()?;

    Ok(())
}

/// The server's own memory (map caches, packets, inventories and so on)
/// goes in the 8 MB of PSRAM. Left to the default, every allocation under
/// 16 KB goes in the ESP32's 300 KB or so of internal RAM until that is
/// full, and then Wi-Fi buffers and new tasks (which must be in internal
/// RAM) run short: sends fail with "Not enough space" and threads can't
/// start. Internal RAM is used only if PSRAM is full.
struct PreferPsram;

const PSRAM: u32 = esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT;
/// heap_caps_malloc's alignment (as Rust's own allocator assumes on ESP-IDF).
const MALLOC_ALIGN: usize = 4;

unsafe impl GlobalAlloc for PreferPsram {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        use esp_idf_svc::sys::*;
        let p: *mut u8 = if layout.align() <= MALLOC_ALIGN {
            heap_caps_malloc(layout.size(), PSRAM).cast()
        } else {
            heap_caps_aligned_alloc(layout.align(), layout.size(), PSRAM).cast()
        };
        if p.is_null() {
            System.alloc(layout)
        } else {
            p
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // Frees from any heap, PSRAM or internal, aligned or not.
        esp_idf_svc::sys::heap_caps_free(ptr.cast());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.align() <= MALLOC_ALIGN {
            // Moves the block to PSRAM if it was in internal RAM.
            let p: *mut u8 = esp_idf_svc::sys::heap_caps_realloc(ptr.cast(), new_size, PSRAM).cast();
            if !p.is_null() {
                return p;
            }
        }
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new = self.alloc(new_layout);
        if !new.is_null() {
            std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
            self.dealloc(ptr, layout);
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: PreferPsram = PreferPsram;

fn log_heap(label: &str) {
    use esp_idf_svc::sys::*;
    unsafe {
        info!(
            "[{label}] internal free {} KB (largest block {} KB), PSRAM free {} KB",
            heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT) / 1024,
            heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT) / 1024,
            heap_caps_get_free_size(MALLOC_CAP_SPIRAM) / 1024,
        );
    }
}
