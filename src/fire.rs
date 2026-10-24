// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from minetest_game's fire mod (mods/fire/init.lua) and the
// burning of dropped items in its default mod (mods/default/item_entity.lua):
//   Copyright (C) 2012 Perttu Ahola (celeron55) <celeron55@gmail.com>
//   Copyright (C) 2012-2018 Various Minetest Game developers and contributors
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! Fire, as minetest_game's fire mod has it: flint and steel lights a
//! flame on anything flammable (or a permanent one on a coal block); a
//! flame burns whoever stands in it and goes out once nothing flammable
//! is left beside it; and, while fire is on (the admin's `/fire on` or
//! `/fire off`, kept in `world/settings.txt` as minetest_game's
//! enable_fire setting), flames and lava set flammable nodes near them
//! alight, and flames burn flammable nodes away.
//!
//! The board can't keep up with a forest fire, so this fire is bounded,
//! unlike minetest_game's: each flame remembers in its param2 how many
//! nodes it is from the flame (or lava) that started it (each new flame
//! adding how far it is from the one that lit it), and none is lit further
//! than MAX_STEPS from there, so a fire burns out within that many nodes
//! of where it began (and a node more, for what the furthest flames burn);
//! and no more than MAX_FLAMES burn at once.
//!
//! The pure parts are here; the server applies them.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use log::{info, warn};

use crate::content;
use crate::nature::Rng;
use crate::storage;
use crate::world::NodePos;

/// The fire mod's fire:basic_flame and fire:permanent_flame.
pub const BASIC_FLAME: u16 = crate::farming::DESERT_SAND_SOIL_WET + 1;
pub const PERMANENT_FLAME: u16 = BASIC_FLAME + 1;

pub const FLAME_TILE: &str = "fire_basic_flame_animated.png";
pub const FLAME_IMAGE: &str = "fire_basic_flame.png";
/// A flame's light_source (13); only the client draws it, as for lava.
pub const FLAME_GLOW: u8 = 13;
/// HP a second for standing in a flame (damage_per_second).
pub const FLAME_DAMAGE: u16 = 4;

/// How far fire spreads from where it started, in steps of one node
/// (see the module's notes), and how many flames may burn at once. The PC
/// build allows much more.
pub const MAX_STEPS: u8 = if cfg!(pc) { 32 } else { 8 };
pub const MAX_FLAMES: usize = if cfg!(pc) { 4096 } else { 256 };

/// Flint and steel's uses (add_wear_by_uses(66)).
pub const FLINT_USES: u32 = 66;

/// The "Ignite flame" ABM: every 7 s, a flammable node beside an igniter
/// catches fire one time in 12.
const IGNITE_INTERVAL: Duration = Duration::from_secs(7);
const IGNITE_CHANCE: u32 = 12;
/// "Remove flammable nodes": every 5 s, a flame burns away a flammable
/// node beside it one time in 18.
const BURN_INTERVAL: Duration = Duration::from_secs(5);
const BURN_CHANCE: u32 = 18;
/// How often the crackle is updated, and how far a player hears flames.
pub const SOUND_EVERY: Duration = Duration::from_secs(3);
pub const SOUND_RADIUS: i32 = 8;
/// How often dropped items are looked at for burning (item_entity's 10 s).
pub const ITEMS_EVERY: Duration = Duration::from_secs(10);

fn fast() -> bool {
    std::env::var_os("LUANTI_FIRE_FAST").is_some()
}

/// The ABMs' interval and chance: (ignite, burn). With LUANTI_FIRE_FAST
/// set (for the tests), every second and every time.
pub fn ignite_every() -> (Duration, u32) {
    if fast() { (Duration::from_secs(1), 1) } else { (IGNITE_INTERVAL, IGNITE_CHANCE) }
}

pub fn burn_every() -> (Duration, u32) {
    if fast() { (Duration::from_secs(1), 1) } else { (BURN_INTERVAL, BURN_CHANCE) }
}

/// A new flame's timer: 30 to 60 s, after which it goes out if there is
/// nothing flammable beside it (or 1 to 2 s with LUANTI_FIRE_FAST).
pub fn flame_timer(rng: &mut Rng) -> Duration {
    if fast() { Duration::from_millis(rng.range(1000, 2000) as u64) } else { Duration::from_secs(rng.range(30, 60) as u64) }
}

pub fn is_flame(id: u16) -> bool {
    id == BASIC_FLAME || id == PERMANENT_FLAME
}

/// group:igniter: flames 2, lava 1.
pub fn igniter(id: u16) -> i16 {
    if is_flame(id) {
        2
    } else if content::is_lava(id) {
        1
    } else {
        0
    }
}

/// group:flammable.
pub fn flammable(id: u16) -> i16 {
    content::group_of(id, "flammable")
}

/// How many nodes apart two nodes are, counting diagonal steps as one
/// (as the fire spreads).
pub fn distance(a: NodePos, b: NodePos) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs()).max((a.2 - b.2).abs())
}

/// The crackle's loudness for so many flames within SOUND_RADIUS.
pub fn sound_gain(flames: usize) -> f32 {
    (0.06 * (1.0 + flames as f32 * 0.125)).min(0.18)
}

/// Whether a dropped item burns up this time it is looked at: always in
/// lava, otherwise by chance, from its flammability and the igniter it is
/// in (item_entity's math.random(0, flammable * igniter) ~= 0).
pub fn item_burns(item: u16, node: u16, rng: &mut Rng) -> bool {
    let flammable = flammable(item);
    if flammable <= 0 {
        return false;
    }
    if content::is_lava(node) {
        return true;
    }
    let chance = flammable as u32 * igniter(node).max(0) as u32;
    chance > 0 && rng.range(0, chance) != 0
}

/// The fire setting, minetest_game's enable_fire, kept in
/// `world/settings.txt` as `enable_fire = true` or `false`. Fire is on
/// unless it says otherwise.
pub struct Settings {
    path: Option<PathBuf>,
    enabled: bool,
}

impl Settings {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let enabled = match storage::read(&path) {
            Ok(text) => parse_enabled(&String::from_utf8_lossy(&text)).unwrap_or(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => true,
            Err(e) => {
                warn!("Reading {}: {e}", path.display());
                true
            }
        };
        if !enabled {
            info!("Fire spreading is off (enable_fire = false in {})", path.display());
        }
        Self { path: Some(path), enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        let Some(path) = &self.path else {
            return;
        };
        if let Err(e) = storage::write(path, format!("enable_fire = {on}\n").as_bytes()) {
            warn!("Saving {}: {e}", path.display());
            crate::status::problem();
        }
    }
}

fn parse_enabled(text: &str) -> Option<bool> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "enable_fire").then(|| matches!(v.trim(), "true" | "yes" | "1" | "on"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert_eq!(igniter(BASIC_FLAME), 2);
        assert_eq!(igniter(content::CONTENT_LAVA_FLOWING), 1);
        assert_eq!(igniter(content::CONTENT_STONE), 0);
        assert!(flammable(content::CONTENT_APPLE_WOOD) > 0 && flammable(content::CONTENT_STONE) == 0);
        assert_eq!(distance((0, 0, 0), (1, -1, 1)), 1);
        assert_eq!(distance((0, 0, 0), (2, 0, -1)), 2);
        assert!((sound_gain(1) - 0.0675).abs() < 1e-6);
        assert_eq!(sound_gain(100), 0.18);
        assert!(content::floodable(BASIC_FLAME) && content::buildable_to(BASIC_FLAME) && !content::walkable(BASIC_FLAME));
        assert_eq!(content::damage_per_second(PERMANENT_FLAME), 4);
        assert_eq!(content::drop_of(BASIC_FLAME), None);
    }

    #[test]
    fn items() {
        let mut rng = Rng::new();
        // Planks in lava: always; stone: never.
        assert!(item_burns(content::CONTENT_APPLE_WOOD, content::CONTENT_LAVA, &mut rng));
        assert!(!item_burns(content::CONTENT_STONE, BASIC_FLAME, &mut rng));
        assert!(!item_burns(content::CONTENT_APPLE_WOOD, crate::world::CONTENT_AIR, &mut rng));
        // In a flame: flammable 2 (planks) times igniter 2, so 4 in 5.
        let burnt = (0..10_000).filter(|_| item_burns(content::CONTENT_APPLE_WOOD, BASIC_FLAME, &mut rng)).count();
        assert!((7500..8500).contains(&burnt), "{burnt}");
    }

    #[test]
    fn settings() {
        assert_eq!(parse_enabled("enable_fire = false\n"), Some(false));
        assert_eq!(parse_enabled("x = 1\nenable_fire=true"), Some(true));
        assert_eq!(parse_enabled(""), None);
        let dir = std::env::temp_dir().join(format!("esp-fire-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.txt");
        let _ = std::fs::remove_file(&path);
        let mut s = Settings::open(&path);
        assert!(s.enabled());
        s.set_enabled(false);
        assert!(!Settings::open(&path).enabled());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
