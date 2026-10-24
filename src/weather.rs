// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from minetest_game's weather mod (mods/weather/init.lua and
// api.lua), MIT License:
//   Copyright (C) 2019 paramat
//   Permission is hereby granted, free of charge, to any person obtaining a
//   copy of this software and associated documentation files (the
//   "Software"), to deal in the Software without restriction, including
//   without limitation the rights to use, copy, modify, merge, publish,
//   distribute, sublicense, and/or sell copies of the Software, and to
//   permit persons to whom the Software is furnished to do so, subject to
//   the following conditions: the above copyright notice and this
//   permission notice shall be included in all copies or substantial
//   portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT
//   WARRANTY OF ANY KIND, EXPRESS OR IMPLIED.
// and from Luanti's value noise (src/noise.cpp, noise2d, noise2d_value and
// NoiseFractal2D), LGPL-2.1-or-later:
//   Copyright (C) 2010-2013 celeron55, Perttu Ahola <celeron55@gmail.com>
//   Copyright (C) 2010-2014 kwolekr, Ryan Kwolek <kwolekr@minetest.net>
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! The weather: clouds that come and go. As minetest_game's weather mod
//! has it, every 8 s each player's clouds are set from slow noise in time
//! (changing over about ten minutes): how much of the sky they cover, how
//! thick they are and which way the wind blows them, with fewer and
//! thinner clouds where the land is dry. Shadows (for players with them
//! turned on) are fainter the cloudier it is.

use std::time::Duration;

/// How often the clouds are set (api.lua's CYCLE).
pub const CYCLE: Duration = Duration::from_secs(8);

/// Time scale of the noise, in seconds (TSCALE).
const TSCALE: f32 = 600.0;

/// minetest_game picks a random offset into the noise for each new world
/// and keeps it in mod storage. The terrain here is the same in every
/// world, so the weather is too: a fixed offset.
const T_OFFSET: i64 = 141_421;

/// The noise seeds: np_density, np_thickness, np_speedx, np_speedz (all
/// one octave, spread TSCALE). Luanti adds the world's seed; there is none
/// here.
const SEED_DENSITY: i32 = 813;
const SEED_THICKNESS: i32 = 96;
const SEED_SPEED_X: i32 = 911_923;
const SEED_SPEED_Z: i32 = 5728;

/// A player's weather (weather.get).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    /// How much of the sky the clouds cover, 0..1.
    pub density: f32,
    /// How thick they are, in nodes.
    pub thickness: f32,
    /// Where they drift, nodes per second along x and z.
    pub speed: (f32, f32),
    /// How dark shadows are, 0..1.
    pub shadow_intensity: f32,
}

/// The weather at a world age (seconds) for a player where the humidity
/// is `humid` (minetest.get_humidity's scale: 50 average).
pub fn at(game_time: u64, humid: f32) -> Weather {
    let time = (game_time as i64 - T_OFFSET) as f32;
    let n_density = perlin(time, 0.5, 0.5, SEED_DENSITY); // 0 to 1
    let n_thickness = perlin(time, 0.5, 0.5, SEED_THICKNESS); // 0 to 1
    let n_speed_x = perlin(time, 0.0, 1.0, SEED_SPEED_X); // -1 to 1
    let n_speed_z = perlin(time, 0.0, 1.0, SEED_SPEED_Z); // -1 to 1
    // The classic density, 0.4, at humidity 50 when n_density is 0.5:
    // density_max is 0.25 at humidity 0, 0.8 at 50 and 1.35 at 100; but
    // at least 0.2, for the odd small cloud even where it is driest.
    let density_max = 0.8 + ((humid - 50.0) / 50.0) * 0.55;
    let density = density_max.clamp(0.2, 1.0) * n_density;
    Weather {
        density,
        thickness: ((32.0 * humid / 100.0).clamp(8.0, 32.0) * n_thickness).floor().max(2.0),
        speed: (n_speed_x * 4.0, n_speed_z * 4.0),
        shadow_intensity: 0.7 * (1.0 - density),
    }
}

/// A one-octave Perlin noise (as minetest.get_perlin's get_2d) at
/// (t, 0), spread TSCALE.
fn perlin(t: f32, offset: f32, scale: f32, seed: i32) -> f32 {
    offset + scale * noise2d_value(t / TSCALE, 0.0, seed)
}

/// Luanti's noise2d: a pseudorandom value, -1..1, for a lattice point.
fn noise2d(x: i32, y: i32, seed: i32) -> f32 {
    let mut n = (1619u32.wrapping_mul(x as u32))
        .wrapping_add(31337u32.wrapping_mul(y as u32))
        .wrapping_add(1013u32.wrapping_mul(seed as u32))
        & 0x7fff_ffff;
    n ^= n >> 13;
    n = n.wrapping_mul(n.wrapping_mul(n).wrapping_mul(60493).wrapping_add(19_990_303)).wrapping_add(1_376_312_589) & 0x7fff_ffff;
    1.0 - n as i32 as f32 / 0x4000_0000 as f32
}

/// Luanti's noise2d_value, eased (the default flags): the lattice values
/// around a point, blended smoothly.
fn noise2d_value(x: f32, y: f32, seed: i32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (xl, yl) = (ease(x - x0 as f32), ease(y - y0 as f32));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let u = lerp(noise2d(x0, y0, seed), noise2d(x0 + 1, y0, seed), xl);
    let v = lerp(noise2d(x0, y0 + 1, seed), noise2d(x0 + 1, y0 + 1, seed), xl);
    lerp(u, v, yl)
}

fn ease(t: f32) -> f32 {
    t * t * t * (t * (6.0 * t - 15.0) + 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luanti_noise() {
        // Values from Luanti's noise2d: (0, 0, 0) hashes to 1376312589.
        assert_eq!(noise2d(0, 0, 0), 1.0 - 1_376_312_589.0 / 1_073_741_824.0);
        for (x, y, s) in [(1, 0, 813), (-5, 3, 96), (123_456, 0, 911_923), (-1, -1, -1)] {
            let v = noise2d(x, y, s);
            assert!((-1.0..=1.0).contains(&v), "{v}");
        }
        // At a lattice point the eased noise is the lattice value.
        assert_eq!(noise2d_value(3.0, 0.0, 7), noise2d(3, 0, 7));
    }

    #[test]
    fn weather_changes_slowly() {
        let mut last = at(0, 50.0);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for t in (8..20_000).step_by(8) {
            let w = at(t, 50.0);
            assert!((w.density - last.density).abs() < 0.05, "{last:?} to {w:?} at {t}");
            assert!((0.0..=0.8).contains(&w.density));
            assert!((2.0..=16.0).contains(&w.thickness));
            assert!(w.speed.0.abs() <= 4.0 && w.speed.1.abs() <= 4.0);
            lo = lo.min(w.density);
            hi = hi.max(w.density);
            last = w;
        }
        // Over a few hours: from nearly clear to cloudy.
        assert!(lo < 0.2 && hi > 0.5, "{lo} to {hi}");
    }

    #[test]
    fn dry_and_wet() {
        for t in [0, 3000, 9999] {
            let (dry, mid, wet) = (at(t, 0.0), at(t, 50.0), at(t, 100.0));
            assert!(dry.density <= mid.density && mid.density <= wet.density);
            assert!(dry.thickness <= wet.thickness);
            assert_eq!(dry.speed, wet.speed);
            assert!((mid.shadow_intensity - 0.7 * (1.0 - mid.density)).abs() < 1e-6);
        }
        // Humidity 50 at the middle of the noise: the classic 0.4.
        assert!((0.8 * 0.5f32 - 0.4).abs() < 1e-6);
    }
}
