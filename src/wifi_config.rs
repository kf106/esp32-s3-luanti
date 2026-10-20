//! The Wi-Fi networks the server may join, kept in a text file on the SD
//! card so they can be changed on a PC:
//!
//! ```text
//! # Networks in order of preference
//! network My Home Wi-Fi
//! password correct horse battery staple
//!
//! network Office
//! password hunter2
//! ```
//!
//! Each `network` line starts an entry; the `password` line after it is
//! optional (none for an open network). Everything after the first space
//! is the value, spaces included. Lines starting with `#` and blank lines
//! are ignored.
//!
//! Networks given when building (WIFI_SSID and WIFI_PASS) are written into
//! the file the first time that firmware starts, so later hand edits are
//! kept: they are written again only if the built-in network changes.

use log::{info, warn};

use crate::storage;

/// Wi-Fi limits: SSIDs up to 32 bytes, WPA passphrases 8 to 63 characters
/// (64 for a raw hex key).
const SSID_MAX: usize = 32;
const PASSWORD_MAX: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub ssid: String,
    pub password: String,
}

/// Read the networks from the file's text, skipping (with a warning)
/// entries Wi-Fi couldn't use.
pub fn parse(text: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
        match key {
            "network" => networks.push(Network { ssid: value.to_string(), password: String::new() }),
            "password" => match networks.last_mut() {
                Some(net) => net.password = value.to_string(),
                None => warn!("wifi.txt line {}: a password before any network", n + 1),
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
    networks
}

/// The file's text for these networks.
pub fn format(networks: &[Network]) -> String {
    let mut text = String::from(
        "# Wi-Fi networks for the Luanti server, in order of preference.\n\
         # The server joins the first one it can see. Edit freely; each\n\
         # \"network\" line starts an entry, and its \"password\" line is\n\
         # optional (leave it out for an open network).\n",
    );
    for net in networks {
        text.push_str(&format!("\nnetwork {}\n", net.ssid));
        if !net.password.is_empty() {
            text.push_str(&format!("password {}\n", net.password));
        }
    }
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

/// The networks to try: from the file, with the built-in one (if any)
/// written into it the first time this firmware sees the card.
pub fn load(files: &Files, built_in: Option<Network>) -> Vec<Network> {
    let text = storage::read(std::path::Path::new(files.networks))
        .map(|d| String::from_utf8_lossy(&d).into_owned())
        .unwrap_or_default();
    let mut networks = parse(&text);
    if let Some(net) = built_in.filter(|n| !n.ssid.is_empty()) {
        let print = fingerprint(&net);
        let written = storage::read(std::path::Path::new(files.written))
            .map(|d| String::from_utf8_lossy(&d).trim().to_string())
            .unwrap_or_default();
        if written != print {
            if merge(&mut networks, &net) {
                info!("Writing the built-in network \"{}\" to {}", net.ssid, files.networks);
            }
            let saved = storage::write(std::path::Path::new(files.networks), format(&networks).as_bytes())
                .and_then(|_| storage::write(std::path::Path::new(files.written), print.as_bytes()));
            if let Err(e) = saved {
                warn!("Saving {}: {e}", files.networks);
            }
        }
    }
    networks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let nets = vec![
            Network { ssid: "My Home Wi-Fi".into(), password: "correct horse battery staple".into() },
            Network { ssid: "Cafe".into(), password: String::new() },
        ];
        assert_eq!(parse(&format(&nets)), nets);
    }
}
