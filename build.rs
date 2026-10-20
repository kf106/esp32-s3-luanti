//! Build script: ESP-IDF set-up, plus the optional SD card reset.
//!
//! `RESET_SD=1 cargo run --release` builds firmware that, the first time it
//! starts, deletes the game data on the SD card (accounts, world, players)
//! so everything is generated afresh. Each such build gets its own token,
//! which the firmware records on the card once it has reset it, so a
//! reboot doesn't wipe the card again. Build normally afterwards.
//!
//! `TEST=1 cargo run --release` builds firmware for testing that gives every
//! player, once, a test kit: 50 of each crafting material (see
//! inventory.rs). Each such build has its own token, so flashing another one
//! hands out a fresh kit. Build normally for play.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    embuild::espidf::sysenv::output();

    println!("cargo:rerun-if-env-changed=RESET_SD");
    if std::env::var("RESET_SD").is_ok_and(|v| v == "1") {
        let token = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        println!("cargo:rustc-env=RESET_SD_TOKEN={token}");
        // A path that never exists makes Cargo run this script on every
        // build, so each reset build gets a fresh token.
        println!("cargo:rerun-if-changed=.reset-sd-every-build");
        println!("cargo:warning=This firmware will delete the accounts, world and players on the SD card when it starts");
    }

    println!("cargo:rerun-if-env-changed=TEST");
    if std::env::var("TEST").is_ok_and(|v| v == "1") {
        let token = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        println!("cargo:rustc-env=TEST_KIT_TOKEN={token}");
        println!("cargo:rerun-if-changed=.test-kit-every-build");
        println!("cargo:warning=This firmware gives every player a test kit of crafting materials");
    }
}
