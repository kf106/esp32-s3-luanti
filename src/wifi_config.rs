//! The Wi-Fi networks the server may join, and the hotspot it can run
//! itself, kept in a text file on the SD card so they can be changed on a
//! PC:
//!
//! ```text
//! # Networks in order of preference
//! network My Home Wi-Fi
//! password correct horse battery staple
//!
//! network Office
//! password hunter2
//!
//! # The board's own hotspot (hold BOOT at start-up)
//! hotspot MTG-A1B2
//! password minetest
//! ```
//!
//! Each `network` line starts an entry; the `password` line after it is
//! optional (none for an open network). The `hotspot` line names the
//! network the board makes itself, and the `password` line after it is
//! that network's password (left out, the hotspot is open). Everything
//! after the first space is the value, spaces included. Lines starting
//! with `#` and blank lines are ignored.
//!
//! Networks given when building (WIFI_SSID and WIFI_PASS) are written into
//! the file the first time that firmware starts, so later hand edits are
//! kept: they are written again only if the built-in network changes. A
//! file with no `hotspot` entry gets the default one added at start-up:
//! "MTG-" and the last four hex digits of the board's hotspot MAC
//! address, so that boards near each other have different names.

use log::{info, warn};

use crate::status::Qr;
use crate::storage;

/// Wi-Fi limits: SSIDs up to 32 bytes, WPA passphrases 8 to 63 characters
/// (64 for a raw hex key).
const SSID_MAX: usize = 32;
const PASSWORD_MAX: usize = 64;
const PASSPHRASE_MIN: usize = 8;

/// The hotspot written into the file when it has none: this and the
/// board's id (see `default_hotspot`). (WPA2 needs a password of at least
/// 8 characters.)
const DEFAULT_HOTSPOT_PREFIX: &str = "MTG-";
const DEFAULT_HOTSPOT_PASSWORD: &str = "minetest";
/// The old default hotspot. A file still holding it, untouched, gets the
/// new default.
const OLD_DEFAULT_HOTSPOT_SSID: &str = "Luanti";
const OLD_DEFAULT_HOTSPOT_PASSWORD: &str = "LuantiLuanti";

/// A board's id for its hotspot's name: the last two bytes of its hotspot
/// MAC address, as four hex digits (as the end of the address a phone or
/// laptop shows).
pub fn board_id(mac: [u8; 6]) -> String {
    format!("{:02X}{:02X}", mac[4], mac[5])
}

#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub ssid: String,
    pub password: String,
}

impl Network {
    /// The default hotspot of the board with this id (see `board_id`).
    pub fn default_hotspot(id: &str) -> Self {
        Self { ssid: format!("{DEFAULT_HOTSPOT_PREFIX}{id}"), password: DEFAULT_HOTSPOT_PASSWORD.into() }
    }

    /// The old default hotspot.
    fn old_default_hotspot() -> Self {
        Self { ssid: OLD_DEFAULT_HOTSPOT_SSID.into(), password: OLD_DEFAULT_HOTSPOT_PASSWORD.into() }
    }
}

/// What the file holds.
#[derive(Debug, PartialEq)]
pub struct Parsed {
    /// Networks to join, in order of preference.
    pub networks: Vec<Network>,
    /// The hotspot entry exactly as written, if there is one (it may not be
    /// usable; see `hotspot`).
    pub hotspot: Option<Network>,
}

/// The settings to use.
pub struct Config {
    pub networks: Vec<Network>,
    /// The hotspot to run: the file's, or the default if the file's can't
    /// be used.
    pub hotspot: Network,
}

/// Read the file's text, skipping (with a warning) networks Wi-Fi couldn't
/// use. The hotspot entry is kept as written.
pub fn parse(text: &str) -> Parsed {
    enum Last {
        Nothing,
        Network,
        Hotspot,
    }
    let mut networks: Vec<Network> = Vec::new();
    let mut hotspot: Option<Network> = None;
    let mut last = Last::Nothing;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
        match key {
            "network" => {
                networks.push(Network { ssid: value.to_string(), password: String::new() });
                last = Last::Network;
            }
            "hotspot" => {
                if hotspot.is_some() {
                    warn!("wifi.txt line {}: a second hotspot; this one is used", n + 1);
                }
                hotspot = Some(Network { ssid: value.to_string(), password: String::new() });
                last = Last::Hotspot;
            }
            "password" => match last {
                Last::Network => networks.last_mut().unwrap().password = value.to_string(),
                Last::Hotspot => hotspot.as_mut().unwrap().password = value.to_string(),
                Last::Nothing => warn!("wifi.txt line {}: a password before any network", n + 1),
            },
            other => warn!("wifi.txt line {}: unknown setting \"{other}\"", n + 1),
        }
    }
    networks.retain(|net| {
        let ok = !net.ssid.is_empty() && net.ssid.len() <= SSID_MAX && net.password.len() <= PASSWORD_MAX;
        if !ok {
            warn!("wifi.txt: skipping network \"{}\": name or password too long (or empty name)", net.ssid);
        }
        ok
    });
    Parsed { networks, hotspot }
}

/// Whether the board can run this hotspot: a name of 1 to 32 bytes, and no
/// password (open) or one WPA2 accepts. Otherwise, what is wrong.
pub fn check_hotspot(net: &Network) -> Result<(), String> {
    if net.ssid.is_empty() || net.ssid.len() > SSID_MAX {
        return Err(format!("its name must be 1 to {SSID_MAX} characters"));
    }
    let pw = &net.password;
    let hex_key = pw.len() == 64 && pw.bytes().all(|b| b.is_ascii_hexdigit());
    if !pw.is_empty() && !hex_key && !(PASSPHRASE_MIN..PASSWORD_MAX).contains(&pw.len()) {
        return Err(format!("its password must be {PASSPHRASE_MIN} to {} characters", PASSWORD_MAX - 1));
    }
    Ok(())
}

const HEADER: &str = "# Wi-Fi networks for the Luanti server, in order of preference.\n\
                      # The server joins the first one it can see. Edit freely; each\n\
                      # \"network\" line starts an entry, and its \"password\" line is\n\
                      # optional (leave it out for an open network).\n";

/// The hotspot's entry, with a note on what it is for.
fn hotspot_text(net: &Network) -> String {
    let mut text = String::from(
        "\n# The board's own Wi-Fi hotspot, for playing where there is no\n\
         # network: hold the BOOT button while the screen asks, just after\n\
         # switching on. Players join this network and connect to 192.168.4.1.\n\
         # The password needs 8 to 63 characters (leave it out for an open\n\
         # hotspot).\n",
    );
    text.push_str(&format!("hotspot {}\n", net.ssid));
    if !net.password.is_empty() {
        text.push_str(&format!("password {}\n", net.password));
    }
    text
}

/// The file's text for these networks and hotspot.
pub fn format(networks: &[Network], hotspot: &Network) -> String {
    let mut text = String::from(HEADER);
    for net in networks {
        text.push_str(&format!("\nnetwork {}\n", net.ssid));
        if !net.password.is_empty() {
            text.push_str(&format!("password {}\n", net.password));
        }
    }
    text.push_str(&hotspot_text(hotspot));
    text
}

/// Put `built_in` first in `networks`: update its password if the network
/// is listed, otherwise add it. Returns whether anything changed.
pub fn merge(networks: &mut Vec<Network>, built_in: &Network) -> bool {
    match networks.iter().position(|n| n.ssid == built_in.ssid) {
        Some(0) if networks[0] == *built_in => false,
        Some(i) => {
            networks.remove(i);
            networks.insert(0, built_in.clone());
            true
        }
        None => {
            networks.insert(0, built_in.clone());
            true
        }
    }
}

/// A short fingerprint of a network (FNV-1a), to tell whether the built-in
/// one has been written to the card already.
fn fingerprint(net: &Network) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in net.ssid.bytes().chain([0]).chain(net.password.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Paths of the networks file and of the note saying which built-in
/// network was last written into it.
pub struct Files<'a> {
    pub networks: &'a str,
    pub written: &'a str,
}

/// What the file should now say, given its current text (None if there is
/// no file), the built-in network if it is new to this card, and this
/// board's default hotspot. None if it needs no change.
fn updated(text: Option<&str>, parsed: &mut Parsed, built_in: Option<&Network>, default: &Network) -> Option<String> {
    // The old default hotspot, untouched, becomes this board's default:
    // only its two lines change, so the rest of the file stays as it was.
    let renamed = match (text, &parsed.hotspot) {
        (Some(t), Some(h)) if *h == Network::old_default_hotspot() => {
            let mut out = String::with_capacity(t.len() + 8);
            let mut in_hotspot = false;
            for line in t.split_inclusive('\n') {
                let trimmed = line.trim_end_matches(['\r', '\n']).trim();
                if trimmed == format!("hotspot {OLD_DEFAULT_HOTSPOT_SSID}") {
                    out.push_str(&format!("hotspot {}\n", default.ssid));
                    in_hotspot = true;
                } else if in_hotspot && trimmed == format!("password {OLD_DEFAULT_HOTSPOT_PASSWORD}") {
                    out.push_str(&format!("password {}\n", default.password));
                    in_hotspot = false;
                } else {
                    if trimmed.starts_with("network ") {
                        in_hotspot = false;
                    }
                    out.push_str(line);
                }
            }
            parsed.hotspot = Some(default.clone());
            Some(out)
        }
        _ => None,
    };
    let merged = built_in.is_some_and(|net| merge(&mut parsed.networks, net));
    if merged {
        let hotspot = parsed.hotspot.clone().unwrap_or_else(|| default.clone());
        return Some(format(&parsed.networks, &hotspot));
    }
    if renamed.is_some() {
        return renamed;
    }
    if parsed.hotspot.is_none() {
        // Added at the end, so the rest of the file stays as it was.
        let mut text = match text {
            Some(t) if !t.trim().is_empty() => t.trim_end().to_string() + "\n",
            _ => HEADER.to_string(),
        };
        text.push_str(&hotspot_text(default));
        return Some(text);
    }
    None
}

/// The networks to try and the hotspot: from the file, with the built-in
/// network (if any) written into it the first time this firmware sees the
/// card, and the default hotspot added if the file has none. `id` is the
/// board's (see `board_id`), for the default hotspot's name.
pub fn load(files: &Files, built_in: Option<Network>, id: &str) -> Config {
    let default = Network::default_hotspot(id);
    let text = storage::read(std::path::Path::new(files.networks)).map(|d| String::from_utf8_lossy(&d).into_owned()).ok();
    let mut parsed = parse(text.as_deref().unwrap_or(""));
    // The built-in network, if this card hasn't had it yet.
    let mut new_built_in: Option<(Network, String)> = None;
    if let Some(net) = built_in.filter(|n| !n.ssid.is_empty()) {
        let print = fingerprint(&net);
        let written = storage::read(std::path::Path::new(files.written))
            .map(|d| String::from_utf8_lossy(&d).trim().to_string())
            .unwrap_or_default();
        if written != print {
            info!("Writing the built-in network \"{}\" to {}", net.ssid, files.networks);
            new_built_in = Some((net, print));
        }
    }
    let had_hotspot = parsed.hotspot.clone();
    if let Some(new_text) = updated(text.as_deref(), &mut parsed, new_built_in.as_ref().map(|(n, _)| n), &default) {
        match had_hotspot {
            None => info!("Adding the default hotspot \"{}\" to {}", default.ssid, files.networks),
            Some(h) if h != default && parsed.hotspot.as_ref() == Some(&default) => {
                info!("Renaming the hotspot \"{}\" to \"{}\" in {}", h.ssid, default.ssid, files.networks)
            }
            _ => {}
        }
        let mut saved = storage::write(std::path::Path::new(files.networks), new_text.as_bytes());
        if let Some((_, print)) = &new_built_in {
            saved = saved.and_then(|_| storage::write(std::path::Path::new(files.written), print.as_bytes()));
        }
        if let Err(e) = saved {
            warn!("Saving {}: {e}", files.networks);
        }
    }
    let hotspot = match parsed.hotspot {
        None => default,
        Some(net) => match check_hotspot(&net) {
            Ok(()) => net,
            Err(why) => {
                warn!("wifi.txt: can't use hotspot \"{}\" ({why}); using \"{}\" instead", net.ssid, default.ssid);
                crate::status::problem();
                default
            }
        },
    };
    Config { networks: parsed.networks, hotspot }
}

/// The text of a Wi-Fi join code, which phone cameras offer to join:
/// `WIFI:T:WPA;S:name;P:password;;`, with `\ ; , : "` escaped.
pub fn join_text(net: &Network) -> String {
    let escape = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if matches!(c, '\\' | ';' | ',' | ':' | '"') {
                out.push('\\');
            }
            out.push(c);
        }
        out
    };
    if net.password.is_empty() {
        format!("WIFI:T:nopass;S:{};;", escape(&net.ssid))
    } else {
        format!("WIFI:T:WPA;S:{};P:{};;", escape(&net.ssid), escape(&net.password))
    }
}

/// The join code for `net` as a QR code (medium error correction, which
/// copes with a slightly blurry photo of the screen).
pub fn join_qr(net: &Network) -> Option<Qr> {
    use qrcodegen::{QrCode, QrCodeEcc};
    let code = QrCode::encode_text(&join_text(net), QrCodeEcc::Medium).ok()?;
    let size = code.size() as usize;
    let mut dark = Vec::with_capacity(size * size);
    for y in 0..size as i32 {
        for x in 0..size as i32 {
            dark.push(code.get_module(x, y));
        }
    }
    Some(Qr { size, dark })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(ssid: &str, password: &str) -> Network {
        Network { ssid: ssid.into(), password: password.into() }
    }

    #[test]
    fn round_trip() {
        let nets = vec![net("My Home Wi-Fi", "correct horse battery staple"), net("Cafe", "")];
        let hotspot = net("Den", "drawbridge");
        assert_eq!(parse(&format(&nets, &hotspot)), Parsed { networks: nets, hotspot: Some(hotspot) });
    }

    #[test]
    fn password_follows_its_entry() {
        let p = parse("hotspot Den\npassword one two three\nnetwork Home\npassword four\nnetwork Cafe\n");
        assert_eq!(p.hotspot, Some(net("Den", "one two three")));
        assert_eq!(p.networks, vec![net("Home", "four"), net("Cafe", "")]);
    }

    #[test]
    fn hotspot_added_without_touching_the_rest() {
        let text = "# mine\nnetwork Home\npassword four\n";
        let mut p = parse(text);
        let new = updated(Some(text), &mut p, None, &Network::default_hotspot("A1B2")).unwrap();
        assert!(new.starts_with(text));
        let again = parse(&new);
        assert_eq!(again.hotspot, Some(net("MTG-A1B2", "minetest")));
        assert_eq!(again.networks, vec![net("Home", "four")]);
        // Once there, nothing more to do.
        let mut again = again;
        assert_eq!(updated(Some(&new), &mut again, None, &Network::default_hotspot("A1B2")), None);
    }

    #[test]
    fn new_file_gets_header_and_hotspot() {
        let mut p = parse("");
        let new = updated(None, &mut p, None, &Network::default_hotspot("00FF")).unwrap();
        assert!(new.starts_with(HEADER));
        assert_eq!(parse(&new).hotspot, Some(net("MTG-00FF", "minetest")));
    }

    #[test]
    fn old_default_hotspot_takes_the_board_id() {
        assert_eq!(board_id([0x24, 0x6f, 0x28, 0x9a, 0x0b, 0xc4]), "0BC4");
        let default = Network::default_hotspot("0BC4");
        let text = "# mine\nnetwork Home\n\n# note\nhotspot Luanti\npassword LuantiLuanti\n";
        let mut p = parse(text);
        let new = updated(Some(text), &mut p, None, &default).unwrap();
        assert_eq!(new, text.replace("hotspot Luanti\npassword LuantiLuanti\n", "hotspot MTG-0BC4\npassword minetest\n"));
        assert_eq!(p.hotspot, Some(default.clone()));
        // With a new built-in network too, the renamed hotspot is kept.
        let mut p = parse(text);
        let new = updated(Some(text), &mut p, Some(&net("Work", "secret99")), &default).unwrap();
        assert_eq!(parse(&new).hotspot, Some(default.clone()));
        // A hand-set password is left alone.
        let text = "hotspot Luanti\npassword mysecret1\n";
        assert_eq!(updated(Some(text), &mut parse(text), None, &default), None);
    }

    #[test]
    fn built_in_keeps_listed_hotspot() {
        let text = "network Home\nhotspot Den\n";
        let mut p = parse(text);
        let new = updated(Some(text), &mut p, Some(&net("Work", "secret99")), &Network::default_hotspot("A1B2")).unwrap();
        let again = parse(&new);
        assert_eq!(again.networks, vec![net("Work", "secret99"), net("Home", "")]);
        assert_eq!(again.hotspot, Some(net("Den", "")));
    }

    #[test]
    fn hotspot_checks() {
        assert!(check_hotspot(&Network::default_hotspot("A1B2")).is_ok());
        assert!(check_hotspot(&net("Open", "")).is_ok());
        assert!(check_hotspot(&net("Short", "Luanti")).is_err());
        assert!(check_hotspot(&net("", "longenough")).is_err());
        assert!(check_hotspot(&net("Hex", &"a".repeat(64))).is_ok());
        assert!(check_hotspot(&net("NotHex", &"z".repeat(64))).is_err());
    }

    #[test]
    fn join_text_escapes() {
        assert_eq!(join_text(&net("Luanti", "LuantiLuanti")), "WIFI:T:WPA;S:Luanti;P:LuantiLuanti;;");
        assert_eq!(join_text(&net("a;b", "")), "WIFI:T:nopass;S:a\\;b;;");
    }
}
