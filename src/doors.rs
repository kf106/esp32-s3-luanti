// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Keir Finlow-Bates
// Based on minetest_game's doors mod (mods/doors, MIT licence):
//   Copyright (C) 2012-2016 PilzAdam
//   Copyright (C) 2014-2016 BlockMen
//   Copyright (C) 2015-2016 sofar (sofar@foo-projects.org)
//   Copyright (C) 2012-2016 Various Minetest Game developers and contributors

//! Doors and trapdoors, as minetest_game's doors mod (mods/doors/init.lua):
//! wooden, glass and steel doors, two nodes tall, and wooden and steel
//! trapdoors, opened and closed with a right-click. Steel ones belong to
//! whoever placed them, and only they can open or dig them.
//!
//! A door is its bottom node, drawn with a model two nodes tall, and a
//! hidden node above it so nothing can be built into its top half. The
//! bottom node is one of four, as in minetest_game:
//!   a  closed, hinge on the left    (state 0)
//!   c  a opened                     (state 1)
//!   b  closed, hinge on the right   (state 2; placed next to another
//!                                    door, so a pair opens outwards)
//!   d  b opened                     (state 3)
//! so the node is the whole state: minetest_game also keeps it in the
//! node's metadata, which we don't have, and don't need.
//!
//! A trapdoor is one node, closed (a thin panel at the bottom) or open
//! (standing against one side), turned by facedir like other blocks.
//!
//! Steel doors' and trapdoors' owners are kept in `world/doors.txt`,
//! "<x> <y> <z> <owner> [<key secret>]" (keys.rs).

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::content::*;
use crate::storage;
use crate::world::NodePos;

pub struct Door {
    /// The item (minetest_game's door craftitem).
    pub item: u16,
    /// The four nodes a, b, c and d have consecutive ids from here.
    pub first: u16,
    pub locked: bool,
    /// Closing and opening sounds, with their gains.
    pub close: (&'static str, f32),
    pub open: (&'static str, f32),
}

pub const FIRST_DOOR: u16 = crate::stairs::END;
pub const DOORS: [Door; 4] = [
    Door {
        item: ITEM_DOOR_WOOD,
        first: FIRST_DOOR,
        locked: false,
        close: ("doors_door_close", 0.13),
        open: ("doors_door_open", 0.06),
    },
    Door {
        item: ITEM_DOOR_STEEL,
        first: FIRST_DOOR + 4,
        locked: true,
        close: ("doors_steel_door_close", 0.2),
        open: ("doors_steel_door_open", 0.2),
    },
    Door {
        item: ITEM_DOOR_GLASS,
        first: FIRST_DOOR + 8,
        locked: false,
        close: ("doors_glass_door_close", 0.25),
        open: ("doors_glass_door_open", 0.3),
    },
    // xpanes' steel bar door, which came later (panes.rs).
    Door {
        item: ITEM_DOOR_STEEL_BAR,
        first: crate::panes::BAR_DOOR,
        locked: true,
        close: ("xpanes_steel_bar_door_close", 0.13),
        open: ("xpanes_steel_bar_door_open", 0.15),
    },
];
/// minetest_game's doors:hidden, above every door.
pub const HIDDEN: u16 = FIRST_DOOR + 12;

pub struct Trapdoor {
    /// The closed node, which is also the item; the open one is next.
    pub closed: u16,
    pub locked: bool,
    pub close: (&'static str, f32),
    pub open: (&'static str, f32),
}

pub const FIRST_TRAPDOOR: u16 = HIDDEN + 1;
pub const TRAPDOORS: [Trapdoor; 3] = [
    Trapdoor {
        closed: FIRST_TRAPDOOR,
        locked: false,
        close: ("doors_door_close", 0.13),
        open: ("doors_door_open", 0.06),
    },
    Trapdoor {
        closed: FIRST_TRAPDOOR + 2,
        locked: true,
        close: ("doors_steel_door_close", 0.2),
        open: ("doors_steel_door_open", 0.2),
    },
    Trapdoor {
        closed: crate::panes::BAR_TRAPDOOR,
        locked: true,
        close: ("xpanes_steel_bar_door_close", 0.13),
        open: ("xpanes_steel_bar_door_open", 0.15),
    },
];

/// The trapdoor a node is, and whether it is the open one.
pub fn trapdoor_of(id: u16) -> Option<(&'static Trapdoor, bool)> {
    TRAPDOORS.iter().find(|t| t.closed == id || t.closed + 1 == id).map(|t| (t, id != t.closed))
}

pub fn is_trapdoor(id: u16) -> bool {
    trapdoor_of(id).is_some()
}

/// Opening or closing a trapdoor (doors.trapdoor_toggle): the other node,
/// turned the same way, and whether it opened.
pub fn trapdoor_toggle(id: u16) -> Option<(u16, bool)> {
    let (t, open) = trapdoor_of(id)?;
    Some(if open { (t.closed, false) } else { (t.closed + 1, true) })
}

/// A trapdoor's box: closed, a panel two pixels thick at the bottom; open,
/// standing at the back (+Z before turning).
pub fn trapdoor_box(open: bool) -> [f32; 6] {
    if open {
        [-0.5, -0.5, 6.0 / 16.0, 0.5, 0.5, 0.5]
    } else {
        [-0.5, -0.5, -0.5, 0.5, -6.0 / 16.0, 0.5]
    }
}

/// Whether a door or trapdoor node is a steel one, with an owner.
pub fn is_locked(id: u16) -> bool {
    door_of(id).is_some_and(|(d, _)| d.locked) || trapdoor_of(id).is_some_and(|(t, _)| t.locked)
}

/// The door that a door node (any of its four) belongs to, and which of
/// the four it is (0 a, 1 b, 2 c, 3 d).
pub fn door_of(id: u16) -> Option<(&'static Door, u16)> {
    DOORS.iter().find(|d| (d.first..d.first + 4).contains(&id)).map(|d| (d, id - d.first))
}

/// The door an item places.
pub fn door_item(item: u16) -> Option<&'static Door> {
    DOORS.iter().find(|d| d.item == item)
}

pub fn is_door(id: u16) -> bool {
    door_of(id).is_some()
}

/// The model a door node is drawn with: a and d hinge on one side, b and
/// c on the other.
pub fn mesh_of(id: u16) -> &'static str {
    match door_of(id) {
        Some((_, 1 | 2)) => "door_b.b3d",
        _ => "door_a.b3d",
    }
}

/// The state (0..3) of each variant a, b, c, d.
const STATE: [u8; 4] = [0, 2, 1, 3];

/// minetest_game's transform table: for the state a door goes to, the
/// variant (0 a, 1 b, 2 c, 3 d) and the param2 for each param2 it had.
const TRANSFORM: [(u16, [u8; 4]); 4] = [
    (0, [3, 0, 1, 2]),
    (2, [1, 2, 3, 0]),
    (1, [1, 2, 3, 0]),
    (3, [3, 0, 1, 2]),
];

/// Opening or closing a door (doors.door_toggle): the new node and param2,
/// and whether it opened. None for something that isn't a door, or a door
/// turned in a way doors can't be.
pub fn toggle(id: u16, param2: u8) -> Option<(u16, u8, bool)> {
    let (door, variant) = door_of(id)?;
    let state = STATE[variant as usize] ^ 1;
    let (to, param2s) = TRANSFORM[state as usize];
    let param2 = *param2s.get(param2 as usize)?;
    Some((door.first + to, param2, state % 2 == 1))
}

/// The facedir for a look direction (builtin's dir_to_facedir, level).
pub fn facedir_of(look: [f32; 3]) -> u8 {
    if look[0].abs() > look[2].abs() {
        if look[0] < 0.0 { 3 } else { 1 }
    } else if look[2] < 0.0 {
        2
    } else {
        0
    }
}

/// Where a door placed facing `dir` looks for a neighbour to pair with.
pub fn beside(pos: NodePos, dir: u8) -> NodePos {
    let (dx, dz) = [(-1, 0), (0, 1), (1, 0), (0, -1)][dir as usize & 3];
    (pos.0 + dx, pos.1, pos.2 + dz)
}

/// Placing a door facing `dir` (the door item's on_place): the bottom
/// node and param2, and the hidden node's param2. Next to another door it
/// hinges on the other side, so the two make a pair.
pub fn placed(door: &Door, dir: u8, next_to_door: bool) -> (u16, u8, u8) {
    if next_to_door {
        (door.first + 1, dir, (dir + 3) % 4)
    } else {
        (door.first, dir, dir)
    }
}

/// The owners of steel doors, and the secrets of their keys.
pub struct Owners {
    path: Option<PathBuf>,
    map: BTreeMap<NodePos, (String, Option<String>)>,
}

impl Owners {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut map = BTreeMap::new();
        match storage::read(&path) {
            Ok(text) => {
                for line in String::from_utf8_lossy(&text).lines() {
                    let f: Vec<&str> = line.split_whitespace().collect();
                    match (f.as_slice(), parse_pos(&f)) {
                        ([_, _, _, owner], Some(pos)) => {
                            map.insert(pos, (owner.to_string(), None));
                        }
                        ([_, _, _, owner, secret], Some(pos)) => {
                            map.insert(pos, (owner.to_string(), Some(secret.to_string())));
                        }
                        _ => warn!("Skipping door line \"{line}\""),
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => warn!("Reading {}: {e}", path.display()),
        }
        Self { path: Some(path), map }
    }

    /// Whether `player` may open or dig the door at `pos`: anyone, unless
    /// it has an owner (default.can_interact_with_node, without keys).
    pub fn allows(&self, pos: NodePos, player: &str) -> bool {
        self.map.get(&pos).map_or(true, |(o, _)| o.eq_ignore_ascii_case(player))
    }

    /// The same, for a player wielding a key with `key`'s secret (or
    /// nothing): the right key opens and digs it too.
    pub fn allows_with(&self, pos: NodePos, player: &str, key: Option<&str>) -> bool {
        self.allows(pos, player) || crate::keys::fits(self.secret(pos), key)
    }

    pub fn owner(&self, pos: NodePos) -> Option<&str> {
        self.map.get(&pos).map(|(o, _)| o.as_str())
    }

    /// The secret of the keys to the door at `pos`, once one has been cut.
    pub fn secret(&self, pos: NodePos) -> Option<&str> {
        self.map.get(&pos).and_then(|(_, s)| s.as_deref())
    }

    pub fn set(&mut self, pos: NodePos, owner: &str) {
        self.map.insert(pos, (owner.to_string(), None));
        self.save();
    }

    /// Give the door at `pos` a key secret, if it has an owner and none
    /// yet; returns its secret.
    pub fn lock_secret(&mut self, pos: NodePos, new: String) -> Option<String> {
        let (_, secret) = self.map.get_mut(&pos)?;
        if secret.is_none() {
            *secret = Some(new);
            let s = secret.clone();
            self.save();
            return s;
        }
        secret.clone()
    }

    pub fn remove(&mut self, pos: NodePos) {
        if self.map.remove(&pos).is_some() {
            self.save();
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let text: String = self
            .map
            .iter()
            .map(|(p, (o, secret))| match secret {
                Some(s) => format!("{} {} {} {o} {s}\n", p.0, p.1, p.2),
                None => format!("{} {} {} {o}\n", p.0, p.1, p.2),
            })
            .collect();
        if let Err(e) = storage::write(path, text.as_bytes()) {
            warn!("Saving {}: {e}", path.display());
            crate::status::problem();
        }
    }
}

fn parse_pos(f: &[&str]) -> Option<NodePos> {
    Some((f.first()?.parse().ok()?, f.get(1)?.parse().ok()?, f.get(2)?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_and_closing() {
        let wood = &DOORS[0];
        let (a, b, c, d) = (wood.first, wood.first + 1, wood.first + 2, wood.first + 3);
        // Placed facing +Z: a, turned 0; opened it becomes c turned 1, and
        // closed again a turned 0.
        assert_eq!(placed(wood, 0, false), (a, 0, 0));
        assert_eq!(toggle(a, 0), Some((c, 1, true)));
        assert_eq!(toggle(c, 1), Some((a, 0, false)));
        // Every way round, opening and closing gets back where it started.
        for v in [a, b] {
            for p2 in 0..4 {
                let (open, op2, opened) = toggle(v, p2).unwrap();
                assert!(opened);
                assert_eq!(toggle(open, op2), Some((v, p2, false)));
            }
        }
        // The pair: b, opening as d.
        assert_eq!(placed(wood, 2, true), (b, 2, 1));
        assert_eq!(toggle(b, 2), Some((d, 1, true)));
        // A door turned upside down (param2 over 3) stays as it is.
        assert_eq!(toggle(a, 20), None);
        assert_eq!(mesh_of(a), "door_a.b3d");
        assert_eq!(mesh_of(c), "door_b.b3d");
        assert!(door_of(HIDDEN).is_none() && door_of(FIRST_DOOR - 1).is_none());
        assert_eq!(door_of(FIRST_DOOR + 6).map(|(d, v)| (d.item, v)), Some((ITEM_DOOR_STEEL, 2)));
        assert_eq!(facedir_of([0.2, -0.5, -0.9]), 2);
        assert_eq!(beside((5, 1, 5), 1), (5, 1, 6));
    }

    #[test]
    fn trapdoors() {
        let (wood, steel) = (FIRST_TRAPDOOR, FIRST_TRAPDOOR + 2);
        assert_eq!(trapdoor_toggle(wood), Some((wood + 1, true)));
        assert_eq!(trapdoor_toggle(wood + 1), Some((wood, false)));
        assert_eq!(trapdoor_toggle(steel), Some((steel + 1, true)));
        assert!(trapdoor_toggle(HIDDEN).is_none() && trapdoor_toggle(steel + 2).is_none());
        assert!(is_locked(steel + 1) && !is_locked(wood) && is_locked(FIRST_DOOR + 5) && !is_locked(FIRST_DOOR));
    }

    #[test]
    fn owners() {
        let dir = std::env::temp_dir().join(format!("esp_doors_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("doors.txt");
        let _ = std::fs::remove_file(&path);
        let mut o = Owners::open(&path);
        o.set((1, -2, 3), "Steve");
        let again = Owners::open(&path);
        assert!(again.allows((1, -2, 3), "steve") && !again.allows((1, -2, 3), "Jim"));
        assert!(again.allows((0, 0, 0), "Jim"));
        // A key: the secret is made once and kept.
        assert_eq!(o.lock_secret((1, -2, 3), "00ff00ff00ff00ff".into()).as_deref(), Some("00ff00ff00ff00ff"));
        assert_eq!(o.lock_secret((1, -2, 3), "1111222233334444".into()).as_deref(), Some("00ff00ff00ff00ff"));
        assert_eq!(o.lock_secret((9, 9, 9), "1111222233334444".into()), None, "nobody's door takes no key");
        let again = Owners::open(&path);
        assert_eq!(again.secret((1, -2, 3)), Some("00ff00ff00ff00ff"));
        assert!(again.allows_with((1, -2, 3), "Jim", Some("00ff00ff00ff00ff")) && !again.allows_with((1, -2, 3), "Jim", Some("00ff00ff00ff00fe")));
        assert_eq!(again.owner((1, -2, 3)), Some("Steve"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
