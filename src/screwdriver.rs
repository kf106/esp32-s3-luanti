// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from minetest_game's screwdriver mod (mods/screwdriver/init.lua):
//   Copyright (C) 2013-2016 RealBadAngel, Maciej Kasatkin
//   Copyright (C) 2013-2016 Various Minetest Game developers and contributors
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! The screwdriver: a left-click turns the node pointed at about its face,
//! a right-click about its axis. It turns nodes whose param2 is a facedir
//! (chests, stairs, furnaces...) or wallmounted (signs, ladders), except
//! torches and doors, which refuse (on_rotate = false); a node that can't
//! be dug just now (a chest with something in it, someone else's locked
//! chest) isn't turned either. Each turn wears it, 200 to a screwdriver.
//!
//! What it turns, and the bed's own rule for turning both halves, are the
//! server's (server.rs); here are the turns themselves.

use crate::content::{self, Turned};

/// Uses before a screwdriver wears out.
pub const USES: u32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Left-click: about the face (screwdriver.ROTATE_FACE).
    Face,
    /// Right-click: about the axis (screwdriver.ROTATE_AXIS).
    Axis,
}

/// screwdriver.rotate.facedir: the low five bits turned; the rest kept.
pub fn facedir(param2: u8, mode: Mode) -> u8 {
    let rotation = param2 % 32;
    let other = param2 - rotation;
    let turned = match (mode, rotation) {
        (Mode::Face, r) if r < 24 => r / 4 * 4 + (r + 1) % 4,
        (Mode::Axis, r) if r < 24 => (r / 4 * 4 + 4) % 24,
        _ => 0,
    };
    turned + other
}

const WALLMOUNTED_FACE: [u8; 6] = [1, 0, 5, 4, 2, 3];
const WALLMOUNTED_AXIS: [u8; 6] = [3, 0, 5, 4, 2, 1];

fn wallmounted_step(rotation: u8, mode: Mode) -> u8 {
    let table = if mode == Mode::Face { &WALLMOUNTED_FACE } else { &WALLMOUNTED_AXIS };
    table.get(rotation as usize).copied().unwrap_or(0)
}

/// screwdriver.rotate.wallmounted: the low three bits turned. An attached
/// node (a sign, a seed) keeps turning, up to five more times, until it
/// faces something it can hang on (`holds(rotation)`).
pub fn wallmounted(param2: u8, mode: Mode, attached: bool, holds: impl Fn(u8) -> bool) -> u8 {
    let rotation = param2 % 8;
    let other = param2 - rotation;
    let mut rotation = wallmounted_step(rotation, mode);
    if attached {
        for _ in 0..5 {
            if holds(rotation) {
                break;
            }
            rotation = wallmounted_step(rotation, mode);
        }
    }
    rotation + other
}

/// How the screwdriver may turn a node: as its paramtype2 says, except
/// torches and doors, which have on_rotate = false.
pub fn turns(id: u16) -> Option<Turned> {
    if content::is_torch(id) || crate::doors::is_door(id) || id == crate::doors::HIDDEN {
        return None;
    }
    content::turned_by(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facedir_turns() {
        // ROTATE_FACE goes round within each group of four.
        assert_eq!(facedir(0, Mode::Face), 1);
        assert_eq!(facedir(3, Mode::Face), 0);
        assert_eq!(facedir(7, Mode::Face), 4);
        assert_eq!(facedir(23, Mode::Face), 20);
        // ROTATE_AXIS moves to the first of the next group.
        assert_eq!(facedir(2, Mode::Axis), 4);
        assert_eq!(facedir(9, Mode::Axis), 12);
        assert_eq!(facedir(21, Mode::Axis), 0);
        // The high bits stay; out-of-range turns go to 0.
        assert_eq!(facedir(32 + 3, Mode::Face), 32);
        assert_eq!(facedir(31, Mode::Face), 0);
        // Every facedir comes back round after four face turns.
        for p in 0..24 {
            let mut q = p;
            for _ in 0..4 {
                q = facedir(q, Mode::Face);
            }
            assert_eq!(q, p);
        }
    }

    #[test]
    fn wallmounted_turns() {
        // The tables: face [2]=5 [3]=4 [4]=2 [5]=3 [1]=0 [0]=1; axis the
        // same but [5]=1, [0]=3.
        assert_eq!(wallmounted(0, Mode::Face, false, |_| true), 1);
        assert_eq!(wallmounted(2, Mode::Face, false, |_| true), 5);
        assert_eq!(wallmounted(5, Mode::Face, false, |_| true), 3);
        assert_eq!(wallmounted(5, Mode::Axis, false, |_| true), 1);
        assert_eq!(wallmounted(0, Mode::Axis, false, |_| true), 3);
        assert_eq!(wallmounted(8 + 2, Mode::Face, false, |_| true), 8 + 5);
        // An attached sign on wall 2 with only a wall at 4 to hang on: 5,
        // 3, then 4.
        assert_eq!(wallmounted(2, Mode::Face, true, |r| r == 4), 4);
        // Nothing to hang on: it gives up after five more turns (5, then
        // 3, 4, 2, 5, 3).
        assert_eq!(wallmounted(2, Mode::Face, true, |_| false), 3);
    }

    #[test]
    fn what_turns() {
        use crate::content::*;
        assert_eq!(turns(CONTENT_CHEST), Some(Turned::Facedir));
        assert_eq!(turns(SIGN_WOOD), Some(Turned::Wallmounted));
        assert_eq!(turns(CONTENT_LADDER_WOOD), Some(Turned::Wallmounted));
        assert_eq!(turns(CONTENT_TORCH), None);
        assert_eq!(turns(CONTENT_TORCH_WALL), None);
        assert_eq!(turns(crate::doors::FIRST_DOOR), None);
        assert_eq!(turns(CONTENT_STONE), None);
        assert_eq!(turns(crate::stairs::MOSSY.first), Some(Turned::Facedir));
    }
}
