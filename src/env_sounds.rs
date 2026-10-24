// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Keir Finlow-Bates
// Based on minetest_game's env_sounds mod (mods/env_sounds/init.lua):
//   Copyright (C) 2019 paramat
// Licensed under the MIT License: permission is hereby granted, free of
// charge, to any person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy,
// modify, merge, publish, distribute, sublicense, and/or sell copies of the
// Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions: the above copyright notice and
// this permission notice shall be included in all copies or substantial
// portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT
// WARRANTY OF ANY KIND, EXPRESS OR IMPLIED.

//! Sounds of the world around each player, as minetest_game's env_sounds
//! makes them: every 3.5 s, running water where there is flowing water
//! within 8 nodes, and bubbling lava near lava. Each plays once, from the
//! middle of the nodes making it, louder the more of them there are.

use std::time::Duration;

use crate::content::{CONTENT_LAVA, CONTENT_LAVA_FLOWING, CONTENT_WATER_FLOWING};
use crate::world::NodePos;

/// How often (cyclic_update).
pub const EVERY: Duration = Duration::from_millis(3500);

/// How far around the player's eyes to listen, in nodes.
const RADIUS: i32 = 8;

/// A sound to play to one player: its name, where (in nodes) and how
/// loud.
#[derive(Debug, PartialEq)]
pub struct Heard {
    pub sound: &'static str,
    pub pos: [f32; 3],
    pub gain: f32,
}

/// What a player with their eyes at `eye` (in nodes) hears, given what is
/// at each node. minetest_game moves the box by the eye height in x and z
/// as well as up (vector.add with a number); here only up.
pub fn around(eye: [f32; 3], node: impl Fn(NodePos) -> u16) -> Vec<Heard> {
    let lo = eye.map(|v| (v - RADIUS as f32).round() as i32);
    let hi = eye.map(|v| (v + RADIUS as f32).round() as i32);
    // (count, sum of positions) of flowing water, lava sources and flowing
    // lava.
    let mut found = [(0u32, [0.0f64; 3]); 3];
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let i = match node((x, y, z)) {
                    CONTENT_WATER_FLOWING => 0,
                    CONTENT_LAVA => 1,
                    CONTENT_LAVA_FLOWING => 2,
                    _ => continue,
                };
                found[i].0 += 1;
                for (s, v) in found[i].1.iter_mut().zip([x, y, z]) {
                    *s += v as f64;
                }
            }
        }
    }
    let mut heard = Vec::new();
    let middle = |groups: &[(u32, [f64; 3])]| {
        let n: u32 = groups.iter().map(|g| g.0).sum();
        let pos = [0, 1, 2].map(|a| (groups.iter().map(|g| g.1[a]).sum::<f64>() / n as f64) as f32);
        (n, pos)
    };
    // Water: 0.04, and 0.004 for each node, up to 0.4.
    if found[0].0 > 0 {
        let (n, pos) = middle(&found[..1]);
        heard.push(Heard { sound: "env_sounds_water", pos, gain: (0.04 + n as f32 * 0.004).min(0.4) });
    }
    // Lava: 0.008 for each source and 0.002 for each flowing node, up to
    // 0.6.
    if found[1].0 + found[2].0 > 0 {
        let (_, pos) = middle(&found[1..]);
        let gain = (found[1].0 as f32 * 0.008 + found[2].0 as f32 * 0.002).min(0.6);
        heard.push(Heard { sound: "env_sounds_lava", pos, gain });
    }
    heard
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::CONTENT_WATER;

    #[test]
    fn water_and_lava() {
        // Nothing but water sources: silence.
        assert!(around([0.0, 0.0, 0.0], |_| CONTENT_WATER).is_empty());
        // Flowing water in a line, x 2..=6 at y 0, z 0.
        let h = around([0.0, 1.5, 0.0], |(x, y, z)| if y == 0 && z == 0 && (2..=6).contains(&x) { CONTENT_WATER_FLOWING } else { 0 });
        assert_eq!(h, vec![Heard { sound: "env_sounds_water", pos: [4.0, 0.0, 0.0], gain: 0.04 + 5.0 * 0.004 }]);
        // Too far: x 10 from eyes at 0 (the box is round(-8)..=round(8)).
        assert!(around([0.0, 0.0, 0.0], |(x, _, _)| if x == 9 { CONTENT_WATER_FLOWING } else { 0 }).is_empty());
        assert_eq!(around([0.6, 0.0, 0.0], |(x, y, z)| if (x, y, z) == (9, 0, 0) { CONTENT_WATER_FLOWING } else { 0 }).len(), 1);
        // A lava lake: as loud as it gets, and water nearby too.
        let h = around([0.0, 0.0, 0.0], |(_, y, _)| match y {
            ..=-1 => CONTENT_LAVA,
            0 => CONTENT_LAVA_FLOWING,
            1 => CONTENT_WATER_FLOWING,
            _ => 0,
        });
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].sound, h[0].gain), ("env_sounds_water", 0.4));
        assert_eq!((h[1].sound, h[1].gain), ("env_sounds_lava", 0.6));
        // Two lava sources and two flowing: 0.02, from between them.
        let h = around([0.0, 0.0, 0.0], |p| match p {
            (1, 0, 0) | (3, 0, 0) => CONTENT_LAVA,
            (1, 0, 2) | (3, 0, 2) => CONTENT_LAVA_FLOWING,
            _ => 0,
        });
        assert_eq!(h, vec![Heard { sound: "env_sounds_lava", pos: [2.0, 0.0, 1.0], gain: 2.0 * 0.008 + 2.0 * 0.002 }]);
    }
}
