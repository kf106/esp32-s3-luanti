//! Beds, as minetest_game's beds mod (mods/beds): a simple bed and a fancy
//! bed, each two nodes long (a bottom, which is the item, and a top),
//! placed the way the player looks. Right-clicking one at night lies the
//! player down in it; when everyone online is in bed the night is
//! skipped. Using a bed, day or night, makes it where the player respawns
//! after dying.
//!
//! The pure parts are here, with the store of respawn points; the server
//! moves the players.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::storage;
use crate::world::NodePos;

/// Node ids, after farming's.
pub const BED_BOTTOM: u16 = 131;
pub const BED_TOP: u16 = 132;
pub const FANCY_BOTTOM: u16 = 133;
pub const FANCY_TOP: u16 = 134;

pub struct Bed {
    pub bottom: u16,
    pub top: u16,
}

pub const BEDS: [Bed; 2] = [
    Bed { bottom: BED_BOTTOM, top: BED_TOP },
    Bed { bottom: FANCY_BOTTOM, top: FANCY_TOP },
];

/// The bed a node belongs to, and whether it is the top half.
pub fn bed_of(id: u16) -> Option<(&'static Bed, bool)> {
    BEDS.iter().find_map(|b| {
        if b.bottom == id {
            Some((b, false))
        } else if b.top == id {
            Some((b, true))
        } else {
            None
        }
    })
}

pub fn is_bed(id: u16) -> bool {
    bed_of(id).is_some()
}

pub fn is_bed_bottom(id: u16) -> bool {
    matches!(bed_of(id), Some((_, false)))
}

/// One step in a facedir direction (minetest.facedir_to_dir, for the four
/// upright turns).
pub fn facedir_dir(param2: u8) -> (i32, i32) {
    [(0, 1), (1, 0), (0, -1), (-1, 0)][param2 as usize & 3]
}

/// Where the top half goes for a bottom at `pos` turned `param2`, or the
/// bottom for a top (get_other_bed_pos).
pub fn other_half(pos: NodePos, param2: u8, top: bool) -> NodePos {
    let (dx, dz) = facedir_dir(param2);
    if top {
        (pos.0 - dx, pos.1, pos.2 - dz)
    } else {
        (pos.0 + dx, pos.1, pos.2 + dz)
    }
}

/// The node boxes (beds.lua), in nodes.
pub fn boxes(id: u16) -> &'static [[f32; 6]] {
    const SIMPLE: [[f32; 6]; 1] = [[-0.5, -0.5, -0.5, 0.5, 0.0625, 0.5]];
    const FANCY_BOTTOM_BOXES: [[f32; 6]; 6] = [
        [-0.5, -0.5, -0.5, -0.375, -0.065, -0.4375],
        [0.375, -0.5, -0.5, 0.5, -0.065, -0.4375],
        [-0.5, -0.375, -0.5, 0.5, -0.125, -0.4375],
        [-0.5, -0.375, -0.5, -0.4375, -0.125, 0.5],
        [0.4375, -0.375, -0.5, 0.5, -0.125, 0.5],
        [-0.4375, -0.3125, -0.4375, 0.4375, -0.0625, 0.5],
    ];
    const FANCY_TOP_BOXES: [[f32; 6]; 7] = [
        [-0.5, -0.5, 0.4375, -0.375, 0.1875, 0.5],
        [0.375, -0.5, 0.4375, 0.5, 0.1875, 0.5],
        [-0.5, 0.0, 0.4375, 0.5, 0.125, 0.5],
        [-0.5, -0.375, 0.4375, 0.5, -0.125, 0.5],
        [-0.5, -0.375, -0.5, -0.4375, -0.125, 0.5],
        [0.4375, -0.375, -0.5, 0.5, -0.125, 0.5],
        [-0.4375, -0.3125, -0.5, 0.4375, -0.0625, 0.4375],
    ];
    match id {
        FANCY_BOTTOM => &FANCY_BOTTOM_BOXES,
        FANCY_TOP => &FANCY_TOP_BOXES,
        _ => &SIMPLE,
    }
}

/// The selection box: the bottom's covers the whole bed; the top's is
/// small, for digging a stray top (api.lua).
pub fn selection_box(id: u16) -> [f32; 6] {
    match id {
        BED_BOTTOM => [-0.5, -0.5, -0.5, 0.5, 0.0625, 1.5],
        FANCY_BOTTOM => [-0.5, -0.5, -0.5, 0.5, 0.06, 1.5],
        _ => [-0.3, -0.3, -0.3, 0.3, -0.1, 0.3],
    }
}

/// The day, when nobody can sleep (beds.day_interval: 0.2 to 0.805 of the
/// day), in Luanti's time units.
const DAY_START: u16 = 4800;
const DAY_END: u16 = 19320;

pub fn is_night(time_of_day: u16) -> bool {
    !(time_of_day > DAY_START && time_of_day < DAY_END)
}

/// The time a skipped night ends at (beds.skip_night: 0.23 of the day).
pub const MORNING: u16 = 5520;

/// Where a player lies in a bed whose bottom is at `bed`: half a node
/// towards the top, just above the simple bed's mattress; and the yaw, in
/// degrees, looking towards the foot (lay_down, get_look_yaw).
pub fn lying_position(bed: NodePos, param2: u8) -> ([f32; 3], f32) {
    let (dx, dz) = facedir_dir(param2);
    let pos = [bed.0 as f32 + dx as f32 / 2.0, bed.1 as f32 + 0.07, bed.2 as f32 + dz as f32 / 2.0];
    let yaw = match param2 & 3 {
        0 => 180.0,
        1 => 90.0,
        3 => -90.0,
        _ => 0.0,
    };
    (pos, yaw)
}

/// The window everyone in bed sees (beds.formspec and update_formspecs).
/// `skip` offers the "Force night skip" button.
pub fn formspec(label: &str, label_x: f32, skip: bool) -> String {
    let mut f = format!(
        "size[8,11;true]no_prepend[]bgcolor[#080808BB;true]button_exit[2,10;4,0.75;leave;Leave Bed]label[{label_x},9;{label}]"
    );
    if skip {
        f.push_str("button_exit[2,6;4,0.75;force;Force night skip]");
    }
    f
}

pub const FORM: &str = "beds_form";

/// Where players respawn: the bed each last used (spawns.lua), kept in
/// `world/beds_spawns` as minetest_game keeps it: "x y z name" lines.
pub struct Spawns {
    path: Option<PathBuf>,
    map: BTreeMap<String, NodePos>,
}

impl Spawns {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut map = BTreeMap::new();
        match storage::read(&path) {
            Ok(text) => {
                for line in String::from_utf8_lossy(&text).lines() {
                    match parse_line(line) {
                        Some((pos, name)) => {
                            map.insert(name, pos);
                        }
                        None => warn!("Skipping bed spawn line \"{line}\""),
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => warn!("Reading {}: {e}", path.display()),
        }
        Self { path: Some(path), map }
    }

    pub fn get(&self, name: &str) -> Option<NodePos> {
        self.map.get(name).copied()
    }

    pub fn set(&mut self, name: &str, pos: NodePos) {
        if self.map.insert(name.to_string(), pos) != Some(pos) {
            self.save();
        }
    }

    /// A bed has gone: nobody respawns there now.
    pub fn remove_at(&mut self, pos: NodePos) {
        let before = self.map.len();
        self.map.retain(|_, p| *p != pos);
        if self.map.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let text: String = self.map.iter().map(|(n, p)| format!("{}.0 {}.0 {}.0 {n}\n", p.0, p.1, p.2)).collect();
        if let Err(e) = storage::write(path, text.as_bytes()) {
            warn!("Saving {}: {e}", path.display());
            crate::status::problem();
        }
    }
}

/// "x y z name", the coordinates written "%.1f".
fn parse_line(line: &str) -> Option<(NodePos, String)> {
    let mut f = line.splitn(4, ' ');
    let mut coord = || f.next()?.parse::<f32>().ok().map(|v| v.round() as i32);
    let pos = (coord()?, coord()?, coord()?);
    let name = f.next()?.trim();
    (!name.is_empty()).then(|| (pos, name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_and_nights() {
        assert!(matches!(bed_of(FANCY_TOP), Some((b, true)) if b.bottom == FANCY_BOTTOM));
        assert!(is_bed_bottom(BED_BOTTOM) && !is_bed_bottom(BED_TOP) && !is_bed(130));
        // Facing +X, the top is east of the bottom, and back again.
        let bottom = (5, 2, -3);
        assert_eq!(other_half(bottom, 1, false), (6, 2, -3));
        assert_eq!(other_half((6, 2, -3), 1, true), bottom);
        assert!(is_night(0) && is_night(4800) && !is_night(4801) && !is_night(12000) && is_night(19320) && is_night(23999));
        let (p, yaw) = lying_position((0, 10, 0), 0);
        assert_eq!((p[2], yaw), (0.5, 180.0));
        assert!((p[1] - 10.07).abs() < 1e-4);
    }

    #[test]
    fn spawns_kept() {
        assert_eq!(parse_line("12.0 -4.0 7.0 Steve the Bold"), Some(((12, -4, 7), "Steve the Bold".to_string())));
        assert_eq!(parse_line("1 2"), None);
        let dir = std::env::temp_dir().join(format!("esp_beds_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("beds_spawns");
        let _ = std::fs::remove_file(&path);
        let mut s = Spawns::open(&path);
        s.set("jim", (1, 2, 3));
        s.set("ann", (4, 5, 6));
        s.remove_at((4, 5, 6));
        let again = Spawns::open(&path);
        assert_eq!((again.get("jim"), again.get("ann")), (Some((1, 2, 3)), None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
