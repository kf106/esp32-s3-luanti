//! Terrain generation: heights, biomes, caves, ores and trees.
//!
//! Everything is a pure function of the node position (and the fixed seeds
//! below), so any node can be worked out on its own, and generating a whole
//! map block gives exactly the same nodes as asking one at a time. The block
//! path only caches intermediate values; both paths share the arithmetic.
//!
//! The noise is gradient (Perlin) noise built from integer hashing and
//! plain + and *, so it gives identical results on any machine.

use crate::farming;
use crate::content::{
    CONTENT_APPLE, CONTENT_APPLE_LEAVES, CONTENT_APPLE_TREE, CONTENT_BIRCH_LEAVES, CONTENT_BIRCH_TREE, CONTENT_COAL, CONTENT_DIRT, CONTENT_FIR_NEEDLES,
    CONTENT_FIR_TREE, CONTENT_GRASS, CONTENT_IRON, CONTENT_SAND, CONTENT_SANDSTONE, CONTENT_SNOW,
    CONTENT_LAVA, CONTENT_STONE,
};
use crate::world::CONTENT_AIR;

/// Bump when the generator changes, so worlds made by an older one are set
/// aside rather than mixed with new terrain (see World::open).
pub const GENERATOR_VERSION: u32 = 2;

const SEED_HILLS: u32 = 0x1A2B_3C4D;
const SEED_MOUNTAINS: u32 = 0x5E6F_7081;
const SEED_TEMPERATURE: u32 = 0x92A3_B4C5;
const SEED_CAVE_A: u32 = 0xD6E7_F809;
const SEED_CAVE_B: u32 = 0x1B2C_3D4E;
const SEED_ORE: u32 = 0x5F60_7182;
const SEED_ENTRANCE: u32 = 0x93A4_B5C6;
const SEED_TREES: u32 = 0xA7B8_C9DA;
const SEED_FOREST: u32 = 0xEB0C_1D2E;
const SEED_GRASS: u32 = 0x2F40_5162;
const SEED_COTTON: u32 = 0x7384_95A6;
const SEED_LAVA: u32 = 0xB7C8_D9EA;

/// Deep caves are flooded with lava up to this height, in about half the
/// land (where the "lava" noise is positive). Between flooded and dry
/// caves a band of solid stone (where the noise is near zero) seals the
/// lava in, so no wall of lava stands open beside a dry tunnel.
///
/// Added without bumping GENERATOR_VERSION: only caves this deep change,
/// so existing worlds keep their changes.
pub const LAVA_LEVEL: i32 = -64;
const LAVA_DAM: f32 = 0.04;

/// Caves are sampled every CAVE_GRID nodes and interpolated in between.
const CAVE_GRID: i32 = 4;
/// Tunnels are where both cave noises are near zero: a^2 + b^2 below this.
const CAVE_WIDTH: f32 = 0.006;
/// Snow covers everything at or above this height.
const SNOW_LINE: i32 = 36;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Biome {
    Grassland,
    Desert,
    Snow,
}

/// What is known about a column of nodes before looking at caves and ores.
#[derive(Clone, Copy, Debug)]
pub struct Column {
    /// The y of the generated surface node.
    pub height: i32,
    pub biome: Biome,
    /// Caves reach no higher than this: a few nodes under the surface,
    /// except at the occasional cave entrance.
    pub cave_top: i32,
    /// What deep caves (at or below LAVA_LEVEL) hold here.
    pub deep: Deep,
}

/// What fills caves at or below LAVA_LEVEL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Deep {
    Air,
    Lava,
    /// No cave: the stone between lava and air.
    Stone,
}

// ---------------------------------------------------------------------------
// Noise
// ---------------------------------------------------------------------------

fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn floor_split(v: f32) -> (i32, f32) {
    let i = v as i32 - (v < 0.0 && v != (v as i32) as f32) as i32;
    (i, v - i as f32)
}

/// 2D gradient noise, roughly -1..1.
fn noise2(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, fx) = floor_split(x);
    let (iz, fz) = floor_split(z);
    let grad = |cx: i32, cz: i32, dx: f32, dz: f32| {
        // Eight directions around the circle
        const G: [(f32, f32); 8] = [
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, 1.0),
            (0.0, -1.0),
            (0.7071, 0.7071),
            (-0.7071, 0.7071),
            (0.7071, -0.7071),
            (-0.7071, -0.7071),
        ];
        let g = G[(hash(cx, 0, cz, seed) & 7) as usize];
        g.0 * dx + g.1 * dz
    };
    let (u, v) = (fade(fx), fade(fz));
    let a = lerp(grad(ix, iz, fx, fz), grad(ix + 1, iz, fx - 1.0, fz), u);
    let b = lerp(grad(ix, iz + 1, fx, fz - 1.0), grad(ix + 1, iz + 1, fx - 1.0, fz - 1.0), u);
    lerp(a, b, v) * 1.4
}

/// 3D gradient noise, roughly -1..1.
fn noise3(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (ix, fx) = floor_split(x);
    let (iy, fy) = floor_split(y);
    let (iz, fz) = floor_split(z);
    let grad = |cx: i32, cy: i32, cz: i32, dx: f32, dy: f32, dz: f32| {
        // The 12 edge directions of a cube (Perlin's improved noise)
        match hash(cx, cy, cz, seed) % 12 {
            0 => dx + dy,
            1 => -dx + dy,
            2 => dx - dy,
            3 => -dx - dy,
            4 => dx + dz,
            5 => -dx + dz,
            6 => dx - dz,
            7 => -dx - dz,
            8 => dy + dz,
            9 => -dy + dz,
            10 => dy - dz,
            _ => -dy - dz,
        }
    };
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let x00 = lerp(grad(ix, iy, iz, fx, fy, fz), grad(ix + 1, iy, iz, fx - 1.0, fy, fz), u);
    let x10 = lerp(
        grad(ix, iy + 1, iz, fx, fy - 1.0, fz),
        grad(ix + 1, iy + 1, iz, fx - 1.0, fy - 1.0, fz),
        u,
    );
    let x01 = lerp(
        grad(ix, iy, iz + 1, fx, fy, fz - 1.0),
        grad(ix + 1, iy, iz + 1, fx - 1.0, fy, fz - 1.0),
        u,
    );
    let x11 = lerp(
        grad(ix, iy + 1, iz + 1, fx, fy - 1.0, fz - 1.0),
        grad(ix + 1, iy + 1, iz + 1, fx - 1.0, fy - 1.0, fz - 1.0),
        u,
    );
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Several octaves of 2D noise, each half the size and strength of the
/// last, scaled back to roughly -1..1.
fn fbm2(x: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut total) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * noise2(x * freq, z * freq, seed.wrapping_add(o * 7919));
        total += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / total
}

// ---------------------------------------------------------------------------
// Columns: height and biome
// ---------------------------------------------------------------------------

pub fn column(x: i32, z: i32) -> Column {
    let (fx, fz) = (x as f32, z as f32);
    // Rolling hills
    let hills = fbm2(fx / 160.0, fz / 160.0, 4, SEED_HILLS) * 22.0;
    // Mountain ranges rise where a broad noise is high
    let m = fbm2(fx / 600.0, fz / 600.0, 3, SEED_MOUNTAINS);
    let mountains = if m > 0.0 { m * m * 280.0 } else { 0.0 };
    let height = (4.0 + hills + mountains).round() as i32;

    let temperature = fbm2(fx / 800.0, fz / 800.0, 2, SEED_TEMPERATURE);
    let biome = if height >= SNOW_LINE || temperature < -0.18 {
        Biome::Snow
    } else if temperature > 0.18 {
        Biome::Desert
    } else {
        Biome::Grassland
    };
    // No cave mouths under water.
    let entrance = height > WATER_LEVEL && noise2(fx / 90.0, fz / 90.0, SEED_ENTRANCE) > 0.55;
    let cave_top = if entrance { height } else { height - 4 };
    let lava = noise2(fx / 200.0, fz / 200.0, SEED_LAVA);
    let deep = if lava > LAVA_DAM {
        Deep::Lava
    } else if lava > -LAVA_DAM {
        Deep::Stone
    } else {
        Deep::Air
    };
    Column { height, biome, cave_top, deep }
}

/// Lakes and ponds fill low ground with still water up to this height
/// (Luanti's default water_level). About an eighth of the land is under
/// it, mostly a few nodes deep.
pub const WATER_LEVEL: i32 = 1;

/// The layered node at height y, before caves and ores.
fn layer(y: i32, col: &Column) -> u16 {
    let depth = col.height - y; // 0 at the surface
    if depth < 0 {
        return CONTENT_AIR;
    }
    // Lake beds and shores are sand, as Minetest Game's beaches.
    if col.height <= WATER_LEVEL && col.biome != Biome::Desert {
        return match depth {
            0..=1 => CONTENT_SAND,
            2..=3 => CONTENT_DIRT,
            _ => CONTENT_STONE,
        };
    }
    match col.biome {
        Biome::Grassland => match depth {
            0 => CONTENT_GRASS,
            1..=3 => CONTENT_DIRT,
            _ => CONTENT_STONE,
        },
        Biome::Snow => match depth {
            0 => CONTENT_SNOW,
            1..=3 => CONTENT_DIRT,
            _ => CONTENT_STONE,
        },
        Biome::Desert => match depth {
            0..=2 => CONTENT_SAND,
            3..=6 => CONTENT_SANDSTONE,
            _ => CONTENT_STONE,
        },
    }
}

// ---------------------------------------------------------------------------
// Caves
// ---------------------------------------------------------------------------

/// The two cave noises at a grid point (grid coordinates = node / 4).
pub fn cave_sample(gx: i32, gy: i32, gz: i32) -> (f32, f32) {
    let (x, y, z) = ((gx * CAVE_GRID) as f32, (gy * CAVE_GRID) as f32, (gz * CAVE_GRID) as f32);
    (
        noise3(x / 40.0, y / 28.0, z / 40.0, SEED_CAVE_A),
        noise3(x / 40.0, y / 28.0, z / 40.0, SEED_CAVE_B),
    )
}

/// Whether a node is inside a tunnel, given the samples at the 8 grid
/// corners around it, in the order (x, y, z) = 000, 100, 010, 110, 001,
/// 101, 011, 111.
pub fn cave_from_corners(c: &[(f32, f32); 8], x: i32, y: i32, z: i32) -> bool {
    if !cell_may_have_cave(c) {
        return false;
    }
    let t = |v: i32| v.rem_euclid(CAVE_GRID) as f32 / CAVE_GRID as f32;
    let (tx, ty, tz) = (t(x), t(y), t(z));
    let tri = |i: usize| -> f32 {
        let get = |k: usize| if i == 0 { c[k].0 } else { c[k].1 };
        let x00 = lerp(get(0), get(1), tx);
        let x10 = lerp(get(2), get(3), tx);
        let x01 = lerp(get(4), get(5), tx);
        let x11 = lerp(get(6), get(7), tx);
        lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
    };
    let (a, b) = (tri(0), tri(1));
    a * a + b * b < CAVE_WIDTH
}

/// Whether any node between these corners can be in a tunnel. Interpolated
/// values stay between the corner values, so if every corner of either
/// noise is clearly on one side of zero, the tunnel condition (both near
/// zero) can't hold anywhere in the cell. The margin covers rounding.
pub fn cell_may_have_cave(c: &[(f32, f32); 8]) -> bool {
    let limit = CAVE_WIDTH.sqrt() * 1.02;
    let clear = |get: &dyn Fn(usize) -> f32| {
        (0..8).all(|k| get(k) > limit) || (0..8).all(|k| get(k) < -limit)
    };
    !(clear(&|k| c[k].0) || clear(&|k| c[k].1))
}

fn grid_of(v: i32) -> i32 {
    v.div_euclid(CAVE_GRID)
}

pub fn corners_at(x: i32, y: i32, z: i32, sample: &mut impl FnMut(i32, i32, i32) -> (f32, f32)) -> [(f32, f32); 8] {
    let (gx, gy, gz) = (grid_of(x), grid_of(y), grid_of(z));
    let mut c = [(0.0, 0.0); 8];
    for (k, item) in c.iter_mut().enumerate() {
        let (dx, dy, dz) = ((k & 1) as i32, ((k >> 1) & 1) as i32, ((k >> 2) & 1) as i32);
        *item = sample(gx + dx, gy + dy, gz + dz);
    }
    c
}

// ---------------------------------------------------------------------------
// Ores
// ---------------------------------------------------------------------------

/// Ore cells are 8x8x8 nodes. A cell holds at most one small cluster, kept
/// inside the cell, so a node's ore depends on its own cell only.
const ORE_CELL: i32 = 8;

fn ore_at(x: i32, y: i32, z: i32) -> Option<u16> {
    let (cx, cy, cz) = (x.div_euclid(ORE_CELL), y.div_euclid(ORE_CELL), z.div_euclid(ORE_CELL));
    let h = hash(cx, cy, cz, SEED_ORE);
    // Coal anywhere, in half the cells (about 1 stone node in 60). Iron
    // only below y = 0, and rarer.
    let ore = match h % 100 {
        0..=49 => CONTENT_COAL,
        50..=64 if y < 0 => CONTENT_IRON,
        _ => return None,
    };
    // Cluster centre, 1..6 within the cell, so the 3x3x3 around it fits.
    let centre = |shift: u32| 1 + ((h >> shift) % 6) as i32;
    let (ox, oy, oz) = (
        cx * ORE_CELL + centre(8),
        cy * ORE_CELL + centre(12),
        cz * ORE_CELL + centre(16),
    );
    let (dx, dy, dz) = (x - ox, y - oy, z - oz);
    if dx.abs() > 1 || dy.abs() > 1 || dz.abs() > 1 {
        return None;
    }
    // An irregular blob: about two thirds of the 3x3x3, always the centre.
    let inside = (dx, dy, dz) == (0, 0, 0) || hash(x, y, z, SEED_ORE ^ 0x55) % 3 != 0;
    inside.then_some(ore)
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

/// The finishing steps shared by single nodes and whole blocks.
pub fn finish(x: i32, y: i32, z: i32, col: &Column, is_cave: impl FnOnce() -> bool) -> u16 {
    let base = layer(y, col);
    if base == CONTENT_AIR {
        return CONTENT_AIR;
    }
    if y <= col.cave_top && is_cave() {
        match col.deep {
            _ if y > LAVA_LEVEL => return CONTENT_AIR,
            Deep::Air => return CONTENT_AIR,
            Deep::Lava => return CONTENT_LAVA,
            Deep::Stone => {}
        }
    }
    if base == CONTENT_STONE {
        if let Some(ore) = ore_at(x, y, z) {
            return ore;
        }
    }
    base
}

/// The generated node at one position. `sample` gives the cave noises at
/// a grid point: `cave_sample`, possibly through a cache.
pub fn node(
    x: i32,
    y: i32,
    z: i32,
    col: &Column,
    sample: &mut impl FnMut(i32, i32, i32) -> (f32, f32),
) -> u16 {
    finish(x, y, z, col, || {
        let c = corners_at(x, y, z, sample);
        cave_from_corners(&c, x, y, z)
    })
}

// ---------------------------------------------------------------------------
// Trees
// ---------------------------------------------------------------------------
//
// The ground is divided into TREE_CELL x TREE_CELL patches, each with one
// possible tree at a spot picked by hashing the patch. Whether it grows
// depends on the biome and on a broad "forest" noise, so there are woods
// and clearings with the odd lone tree between. Birches grow in the
// lowland grass, firs higher up and in the snow, nothing in the desert.
//
// Trees only fill air: a node that the terrain makes solid stays solid.

/// Size of the patches that each hold at most one tree.
const TREE_CELL: i32 = 5;
/// How far a tree's leaves reach sideways from its trunk.
pub const TREE_REACH: i32 = 3;
/// Firs replace birches from this height up.
const FIR_LINE: i32 = 22;
/// No trees above this height (bare mountain tops).
const TREE_LINE: i32 = 70;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TreeKind {
    Birch,
    Fir,
    Apple,
}

#[derive(Clone, Copy, Debug)]
pub struct Tree {
    /// The trunk's column.
    pub x: i32,
    pub z: i32,
    /// The y of the ground node it stands on.
    pub ground: i32,
    pub kind: TreeKind,
    /// Trunk length.
    pub height: i32,
}

impl Tree {
    /// The highest y any part of the tree reaches.
    pub fn top(&self) -> i32 {
        self.ground + self.crown_top()
    }

    /// The highest part of the tree, above the ground.
    fn crown_top(&self) -> i32 {
        match self.kind {
            // minetest_game's apple tree: leaves one node above the trunk's
            // top and its last node.
            TreeKind::Apple => self.height + 2,
            _ => self.height + 1,
        }
    }

    /// The tree's node at (x, y, z), if it has one there.
    pub fn node_at(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let (dx, dz) = (x - self.x, z - self.z);
        let dy = y - self.ground; // 1 = just above the ground
        let h = self.height;
        if dx.abs() > TREE_REACH || dz.abs() > TREE_REACH || dy < 1 || dy > self.crown_top() {
            return None;
        }
        let trunk = dx == 0 && dz == 0 && dy <= h;
        match self.kind {
            TreeKind::Birch => {
                if trunk {
                    return Some(CONTENT_BIRCH_TREE);
                }
                // A rounded crown over the top of the trunk.
                let (ax, az) = (dx.abs(), dz.abs());
                let leafy = match h + 1 - dy {
                    0 => ax + az <= 1,                    // the very top
                    1 => ax <= 1 && az <= 1,
                    2 | 3 => {
                        // Wide layers, with ragged corners.
                        ax <= 2 && az <= 2 && !(ax == 2 && az == 2 && hash(x, y, z, SEED_TREES ^ 1) % 3 != 0)
                    }
                    _ => false,
                };
                leafy.then_some(CONTENT_BIRCH_LEAVES)
            }
            TreeKind::Fir => {
                if trunk {
                    return Some(CONTENT_FIR_TREE);
                }
                // A cone of needle layers, alternately wider and narrower,
                // from the tip down to two nodes above the ground.
                if dy < 3 {
                    return None;
                }
                const RADIUS: [i32; 12] = [0, 1, 1, 2, 1, 2, 2, 3, 2, 3, 3, 3];
                let r = RADIUS[((h + 1 - dy) as usize).min(RADIUS.len() - 1)];
                let leafy = match r {
                    0 => dx == 0 && dz == 0,
                    1 => dx.abs() + dz.abs() <= 1,
                    _ => dx * dx + dz * dz <= r * r,
                };
                leafy.then_some(CONTENT_FIR_NEEDLES)
            }
            TreeKind::Apple => {
                if trunk {
                    return Some(CONTENT_APPLE_TREE);
                }
                // minetest_game's add_trunk_and_leaves (size 2): a 3x4x3
                // block of leaves around the top of the trunk, from two below
                // its top to one above, then 2x2x2 clusters scattered within
                // two nodes, here picked by hashing. One leaf in eight is an
                // apple.
                let (ax, az) = (dx.abs(), dz.abs());
                let rel = dy - h; // 0: level with the trunk's last node
                if !(-2..=2).contains(&rel) || ax > 2 || az > 2 {
                    return None;
                }
                let core = ax <= 1 && az <= 1 && rel <= 1;
                let cluster = || {
                    // Clusters are 2x2x2: share one hash per pair of nodes.
                    let (cx, cy, cz) = (x.div_euclid(2), y.div_euclid(2), z.div_euclid(2));
                    hash(cx, cy, cz, SEED_TREES ^ 2) % 3 == 0
                };
                if !core && !cluster() {
                    return None;
                }
                if hash(x, y, z, SEED_TREES ^ 3) % 8 == 0 {
                    Some(CONTENT_APPLE)
                } else {
                    Some(CONTENT_APPLE_LEAVES)
                }
            }
        }
    }
}

/// The tree in a patch, if one grows there.
fn tree_in_cell(cx: i32, cz: i32) -> Option<Tree> {
    let h = hash(cx, 0x7EE, cz, SEED_TREES);
    let x = cx * TREE_CELL + (h % TREE_CELL as u32) as i32;
    let z = cz * TREE_CELL + ((h >> 8) % TREE_CELL as u32) as i32;
    let col = column(x, z);
    // At a cave entrance the ground may be hollowed out.
    // Nor in lakes or on their sandy shores.
    if col.cave_top >= col.height || col.height >= TREE_LINE || col.height <= WATER_LEVEL {
        return None;
    }
    let kind = match col.biome {
        Biome::Desert => return None,
        Biome::Snow => TreeKind::Fir,
        Biome::Grassland if col.height >= FIR_LINE => TreeKind::Fir,
        Biome::Grassland => TreeKind::Birch,
    };
    // A few percent of patches in the open, most of them in a wood.
    let forest = fbm2(x as f32 / 220.0, z as f32 / 220.0, 2, SEED_FOREST);
    let chance = 4 + ((forest - 0.05) * 300.0).clamp(0.0, 66.0) as u32;
    let roll = (h >> 16) % 100;
    // Apple trees came later: they grow only in lowland patches that had
    // no tree, so the trees of worlds made before stay as they were. About
    // one lowland tree in three is an apple tree.
    let kind = if roll < chance {
        kind
    } else if kind == TreeKind::Birch && roll < chance + chance / 2 {
        TreeKind::Apple
    } else {
        return None;
    };
    let height = match kind {
        TreeKind::Birch => 5 + ((h >> 24) % 3) as i32,
        TreeKind::Fir => 7 + ((h >> 24) % 4) as i32,
        // minetest_game: random(4, 5).
        TreeKind::Apple => 4 + ((h >> 24) % 2) as i32,
    };
    Some(Tree { x, z, ground: col.height, kind, height })
}

/// The plant growing on the grassland surface of a column, if any: tall
/// grass in drifts that come and go across the land (minetest_game's grass
/// decorations, five heights), and here and there a patch of wild cotton
/// (minetest_game grows it in savanna, which we don't have). Not on sand,
/// snow or a cave mouth. It stands in the node above the surface, which
/// was empty in worlds made before.
pub fn plant_on(x: i32, z: i32, col: &Column) -> Option<u16> {
    if col.biome != Biome::Grassland || col.height <= WATER_LEVEL || col.cave_top >= col.height {
        return None;
    }
    let h = hash(x, 0x6A55, z, SEED_GRASS);
    if noise2(x as f32 / 60.0, z as f32 / 60.0, SEED_COTTON) > 0.55 && h % 16 == 0 {
        return Some(farming::COTTON_WILD);
    }
    let density = ((fbm2(x as f32 / 100.0, z as f32 / 100.0, 2, SEED_GRASS) + 0.3) * 0.25).clamp(0.0, 0.2);
    ((h >> 8) % 1000 < (density * 1000.0) as u32).then(|| farming::GRASS[((h >> 20) % 5) as usize])
}

/// Every tree that could have a node in the columns from (x0, z0) to
/// (x1, z1) inclusive.
pub fn trees_in(x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<Tree> {
    let cell = |v: i32| v.div_euclid(TREE_CELL);
    let mut trees = Vec::new();
    for cz in cell(z0 - TREE_REACH)..=cell(z1 + TREE_REACH) {
        for cx in cell(x0 - TREE_REACH)..=cell(x1 + TREE_REACH) {
            if let Some(t) = tree_in_cell(cx, cz) {
                let reaches = t.x + TREE_REACH >= x0 && t.x - TREE_REACH <= x1 && t.z + TREE_REACH >= z0 && t.z - TREE_REACH <= z1;
                if reaches {
                    trees.push(t);
                }
            }
        }
    }
    trees
}

/// The tree node at (x, y, z) among `trees`, if any; a trunk wins over
/// another tree's leaves. The caller makes sure the terrain is air there.
/// Birches and firs come first: apple trees (added later) only fill what
/// they leave empty, so the trees of older worlds stay exactly as they were.
pub fn tree_node_in(trees: &[Tree], x: i32, y: i32, z: i32) -> Option<u16> {
    let pick = |apple: bool| {
        let mut found = None;
        for t in trees.iter().filter(|t| (t.kind == TreeKind::Apple) == apple) {
            match t.node_at(x, y, z) {
                Some(id) if id == CONTENT_BIRCH_TREE || id == CONTENT_FIR_TREE || id == CONTENT_APPLE_TREE => return Some(id),
                Some(id) => found = found.or(Some(id)),
                None => {}
            }
        }
        found
    };
    pick(false).or_else(|| pick(true))
}
