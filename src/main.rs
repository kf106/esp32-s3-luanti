mod auth;
mod chest;
mod content;
mod crafting;
mod fasthash;
mod font;
mod furnace;
mod indicator;
mod inventory;
mod items;
mod lcd;
mod messages;
mod nature;
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
mod wifi_config;
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
/// Wi-Fi networks, editable on a PC (see wifi_config.rs).
const WIFI_FILE: &str = "/sd/wifi.txt";
/// Which built-in network was last written to WIFI_FILE.
const WIFI_WRITTEN: &str = "/sd/wifi-written";
/// How long to wait before looking for known networks again.
const WIFI_RETRY: std::time::Duration = std::time::Duration::from_secs(10);
/// How often to check that the Wi-Fi connection is still there.
const WIFI_CHECK: std::time::Duration = std::time::Duration::from_secs(2);
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
    reset_sd_if_asked();
    for entry in fs::read_dir(MOUNT)? {
        info!("  {:?}", entry?.file_name());
    }

    // The status screen starts now, so it can say what Wi-Fi is doing.
    let status = status::Status::new("Wi-Fi...".to_string());
    lcd::start(status.clone(), p.spi2, pins.gpio40, pins.gpio45);

    // Wi-Fi: the networks in wifi.txt, plus one given when building
    // (WIFI_SSID, WIFI_PASS), which is written into wifi.txt.
    let built_in = option_env!("WIFI_SSID").filter(|s| !s.is_empty()).map(|ssid| wifi_config::Network {
        ssid: ssid.to_string(),
        password: option_env!("WIFI_PASS").unwrap_or("").to_string(),
    });
    let networks = wifi_config::load(&wifi_config::Files { networks: WIFI_FILE, written: WIFI_WRITTEN }, built_in);
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(p.modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;
    // No network yet, just scanning. (The default asks for WPA2 with an
    // empty password, which the driver warns about.)
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        auth_method: esp_idf_svc::wifi::AuthMethod::None,
        ..Default::default()
    }))?;
    wifi.start()?;
    let joined = join_wifi(&mut wifi, &networks, &status);
    let ip = wifi.wifi().sta_netif().get_ip_info()?.ip;
    info!("Connected to {joined}, IP address {ip}");
    {
        let mut st = status::lock(&status);
        st.address = format!("{ip}:{PORT}");
        st.event(format!("Joined Wi-Fi {joined}"));
    }
    // Power saving makes the radio doze between beacons, which delays and
    // drops outgoing packets. A server should stay awake.
    esp_idf_svc::sys::esp!(unsafe {
        esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE)
    })?;
    log_heap("after Wi-Fi");
    // ESP-IDF doesn't rejoin a network on its own: a thread watches the
    // connection and rejoins if it drops (the router restarts, or the board
    // loses signal). The server carries on meanwhile, and players whose
    // clients don't time out (30 s) carry on playing.
    std::thread::Builder::new()
        .name("wifi".into())
        .stack_size(8 * 1024)
        .spawn({
            let status = status.clone();
            move || watch_wifi(wifi, &networks, &status, joined)
        })?;

    // Luanti server
    let auth = auth::AuthStore::open(format!("{MOUNT}/auth"))?;
    let world = world::World::open(format!("{MOUNT}/world"))?;
    let inventories = inventory::InventoryStore::open(format!("{MOUNT}/players"))?;
    let players = player::PlayerStore::open(format!("{MOUNT}/players"))?;
    let furnaces = furnace::Furnaces::open(format!("{MOUNT}/world/furnaces.txt"));
    let chests = chest::Chests::open(format!("{MOUNT}/world/chests.txt"));
    let media = content::Media::load(&format!("{MOUNT}/media"));
    let socket = UdpSocket::bind(("0.0.0.0", PORT))?;
    let sent = std::sync::Arc::as_ptr(&status) as usize;
    // An optional welcome message of your own, one chat line per line.
    let motd = fs::read_to_string(format!("{MOUNT}/motd.txt")).ok();
    if motd.is_some() {
        info!("Welcome message from {MOUNT}/motd.txt");
    }
    let mut server = server::Server::new(server::Setup { socket, motd, auth, world, inventories, players, furnaces, chests, media, status });
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

/// Join one of `networks`, trying those in range in the file's order, and
/// keep trying until one works. Returns its name.
fn join_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>, networks: &[wifi_config::Network], status: &status::SharedStatus) -> String {
    use esp_idf_svc::wifi::AuthMethod;
    let say = |text: String| {
        info!("{text}");
        status::lock(status).event(text);
    };
    if networks.is_empty() {
        say(format!("No Wi-Fi networks: add one to {WIFI_FILE}"));
    }
    loop {
        let seen = match wifi.scan() {
            Ok(aps) => aps,
            Err(e) => {
                log::warn!("Wi-Fi scan failed: {e}");
                Vec::new()
            }
        };
        // Known networks in range first (in the file's order), then the
        // rest, in case one is hidden (not broadcasting its name).
        let mut order: Vec<(&wifi_config::Network, Option<&esp_idf_svc::wifi::AccessPointInfo>)> = Vec::new();
        for net in networks {
            let ap = seen
                .iter()
                .filter(|ap| ap.ssid.as_str() == net.ssid)
                .max_by_key(|ap| ap.signal_strength);
            if ap.is_some() {
                order.push((net, ap));
            }
        }
        if order.is_empty() && !networks.is_empty() {
            say(format!("None of the {} known Wi-Fi networks is in range", networks.len()));
        }
        for net in networks {
            if !order.iter().any(|(n, _)| n.ssid == net.ssid) {
                order.push((net, None));
            }
        }
        for (net, ap) in order {
            let (Ok(ssid), Ok(password)) = (net.ssid.as_str().try_into(), net.password.as_str().try_into()) else {
                continue;
            };
            let auth_method = match ap.and_then(|ap| ap.auth_method) {
                Some(m) => m,
                None if net.password.is_empty() => AuthMethod::None,
                None => AuthMethod::WPA2Personal,
            };
            let config = Configuration::Client(ClientConfiguration {
                ssid,
                password,
                auth_method,
                channel: ap.map(|ap| ap.channel),
                ..Default::default()
            });
            say(format!("Joining Wi-Fi {}...", net.ssid));
            let joined = wifi
                .set_configuration(&config)
                .and_then(|_| wifi.connect())
                .and_then(|_| wifi.wait_netif_up());
            match joined {
                Ok(()) => return net.ssid.clone(),
                Err(e) => {
                    log::warn!("Joining {} failed: {e}", net.ssid);
                    let _ = wifi.disconnect();
                }
            }
        }
        std::thread::sleep(WIFI_RETRY);
    }
}

/// Keep the Wi-Fi connection: check it every WIFI_CHECK, and when it has
/// gone, try the same network again (most drop-outs are short), then the
/// known networks as at startup, until one works.
fn watch_wifi(mut wifi: BlockingWifi<EspWifi<'static>>, networks: &[wifi_config::Network], status: &status::SharedStatus, mut joined: String) {
    loop {
        std::thread::sleep(WIFI_CHECK);
        if wifi.is_up().unwrap_or(false) {
            continue;
        }
        let text = format!("Lost Wi-Fi {joined}");
        log::warn!("{text}");
        {
            // The screen's header and the status light show it's offline.
            let mut st = status::lock(status);
            st.event(text);
            st.address = "Wi-Fi...".to_string();
        }
        info!("Rejoining {joined}...");
        let again = wifi.connect().and_then(|_| wifi.wait_netif_up());
        if let Err(e) = again {
            log::warn!("Rejoining {joined} failed: {e}");
            let _ = wifi.disconnect();
            joined = join_wifi(&mut wifi, networks, status);
        }
        match wifi.wifi().sta_netif().get_ip_info() {
            Ok(info) => {
                info!("Back on Wi-Fi {joined}, IP address {}", info.ip);
                let mut st = status::lock(status);
                st.address = format!("{}:{PORT}", info.ip);
                st.event(format!("Rejoined Wi-Fi {joined}"));
            }
            Err(e) => log::warn!("Rejoined {joined}, but no IP address: {e}"),
        }
    }
}

/// Game data on the SD card that a reset deletes. Extra media (skins and
/// textures in /sd/media) are kept.
const RESET_DIRS: [&str; 3] = ["auth", "world", "players"];

/// If this firmware was built with RESET_SD=1 (see build.rs), delete the
/// game data on the SD card, once: the build's token is written to the card
/// afterwards, and a card already reset by this build is left alone.
fn reset_sd_if_asked() {
    let Some(token) = option_env!("RESET_SD_TOKEN") else {
        return;
    };
    let marker = format!("{MOUNT}/reset-done");
    if fs::read_to_string(&marker).is_ok_and(|t| t.trim() == token) {
        info!("SD card reset: already done by this firmware, so the card is left as it is");
        return;
    }
    log::warn!("SD card reset: deleting {}", RESET_DIRS.join(", "));
    for dir in RESET_DIRS {
        let path = format!("{MOUNT}/{dir}");
        match remove_tree(std::path::Path::new(&path)) {
            Ok(n) => info!("  {path}: {n} files deleted"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("  {path}: {e}"),
        }
    }
    if let Err(e) = fs::write(&marker, token) {
        log::warn!("SD card reset: couldn't record it ({e}); it will happen again at the next start");
    }
}

/// Delete a directory and everything in it, returning how many files went.
/// (Done by hand: FAT on ESP-IDF supports only the basic file operations.)
fn remove_tree(dir: &std::path::Path) -> std::io::Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            count += remove_tree(&path)?;
        } else {
            fs::remove_file(&path)?;
            count += 1;
        }
    }
    fs::remove_dir(dir)?;
    Ok(count)
}

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
