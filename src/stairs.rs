//! Stairs and slabs, as minetest_game's stairs mod (mods/stairs/init.lua):
//! a stair and a slab for each building block, crafted from it and back,
//! turned to face the player who places them, and upside down when placed
//! against the underside of a block or the upper half of its side.
//!
//! Their content ids come in sets: for each set, all the stairs in its
//! materials' order, then all the slabs, the inner corner stairs and the
//! outer corner stairs. The first set follows on from the early nodes; the
//! second, for the jungle and acacia wood that came later, from the fences.

use crate::content::*;

/// A block that stairs and slabs are made of.
pub struct Material {
    pub base: u16,
    /// minetest_game's worldaligntex: the texture continues across
    /// neighbouring nodes rather than starting again in each.
    pub world_aligned: bool,
}

/// minetest_game's materials for those of our blocks it makes stairs of
/// (its aspen and pine wood for our birch and fir).
pub const MATERIALS: [Material; 10] = [
    Material { base: CONTENT_BIRCH_WOOD, world_aligned: false },
    Material { base: CONTENT_FIR_WOOD, world_aligned: false },
    Material { base: CONTENT_APPLE_WOOD, world_aligned: false },
    Material { base: CONTENT_STONE, world_aligned: true },
    Material { base: CONTENT_COBBLE, world_aligned: true },
    Material { base: CONTENT_STONE_BRICK, world_aligned: false },
    Material { base: CONTENT_STONE_BLOCK, world_aligned: true },
    Material { base: CONTENT_SANDSTONE, world_aligned: true },
    Material { base: CONTENT_SANDSTONE_BRICK, world_aligned: false },
    Material { base: CONTENT_SANDSTONE_BLOCK, world_aligned: true },
];

/// The woods added later: minetest_game's jungle and acacia.
pub const LATER_MATERIALS: [Material; 2] = [
    Material { base: CONTENT_JUNGLE_WOOD, world_aligned: false },
    Material { base: CONTENT_ACACIA_WOOD, world_aligned: false },
];

/// A run of ids: the stairs, slabs, inner and outer corners of some
/// materials.
pub struct Set {
    pub first: u16,
    pub materials: &'static [Material],
}

impl Set {
    const fn count(&self) -> u16 {
        self.materials.len() as u16
    }
    pub const fn first_stair(&self) -> u16 {
        self.first
    }
    pub const fn first_slab(&self) -> u16 {
        self.first + self.count()
    }
    pub const fn first_inner(&self) -> u16 {
        self.first + 2 * self.count()
    }
    pub const fn first_outer(&self) -> u16 {
        self.first + 3 * self.count()
    }
    /// One past its last id.
    pub const fn end(&self) -> u16 {
        self.first + 4 * self.count()
    }
}

/// The building blocks added after those: minetest_game's desert and
/// silver sandstones and stones, brick, obsidian, ice, snow and the metal
/// blocks.
pub const BUILDING_MATERIALS: [Material; 21] = [
    Material { base: CONTENT_DESERT_STONE, world_aligned: true },
    Material { base: CONTENT_DESERT_COBBLE, world_aligned: true },
    Material { base: CONTENT_DESERT_STONE_BRICK, world_aligned: false },
    Material { base: CONTENT_DESERT_STONE_BLOCK, world_aligned: true },
    Material { base: CONTENT_DESERT_SANDSTONE, world_aligned: true },
    Material { base: CONTENT_DESERT_SANDSTONE_BRICK, world_aligned: false },
    Material { base: CONTENT_DESERT_SANDSTONE_BLOCK, world_aligned: true },
    Material { base: CONTENT_SILVER_SANDSTONE, world_aligned: true },
    Material { base: CONTENT_SILVER_SANDSTONE_BRICK, world_aligned: false },
    Material { base: CONTENT_SILVER_SANDSTONE_BLOCK, world_aligned: true },
    Material { base: CONTENT_BRICK, world_aligned: false },
    Material { base: CONTENT_OBSIDIAN, world_aligned: true },
    Material { base: CONTENT_OBSIDIAN_BRICK, world_aligned: false },
    Material { base: CONTENT_OBSIDIAN_BLOCK, world_aligned: true },
    Material { base: CONTENT_ICE, world_aligned: true },
    Material { base: CONTENT_SNOW_BLOCK, world_aligned: true },
    Material { base: CONTENT_STEEL_BLOCK, world_aligned: true },
    Material { base: CONTENT_COPPER_BLOCK, world_aligned: true },
    Material { base: CONTENT_TIN_BLOCK, world_aligned: true },
    Material { base: CONTENT_BRONZE_BLOCK, world_aligned: true },
    Material { base: CONTENT_GOLD_BLOCK, world_aligned: true },
];

pub const EARLY: Set = Set { first: 41, materials: &MATERIALS };
pub const LATER: Set = Set { first: crate::fences::RAIL_ACACIA + 1, materials: &LATER_MATERIALS };
pub const BUILDING: Set = Set { first: CONTENT_OBSIDIAN_GLASS + 1, materials: &BUILDING_MATERIALS };
const SETS: [Set; 3] = [EARLY, LATER, BUILDING];

pub const FIRST_STAIR: u16 = EARLY.first_stair();
pub const FIRST_SLAB: u16 = EARLY.first_slab();
pub const FIRST_INNER: u16 = EARLY.first_inner();
pub const FIRST_OUTER: u16 = EARLY.first_outer();
/// One past the first set's last id.
pub const END: u16 = EARLY.end();

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Stair,
    Slab,
    /// An inner corner: a stair with the back corner filled in.
    Inner,
    /// An outer corner: a stair with only one quarter of the upper step.
    Outer,
}

const SHAPES: [Shape; 4] = [Shape::Stair, Shape::Slab, Shape::Inner, Shape::Outer];

impl Shape {
    /// The boxes it is made of, in nodes (minetest_game's node_box).
    pub fn boxes(self) -> &'static [[f32; 6]] {
        match self {
            Shape::Stair => &[[-0.5, -0.5, -0.5, 0.5, 0.0, 0.5], [-0.5, 0.0, 0.0, 0.5, 0.5, 0.5]],
            Shape::Slab => &[[-0.5, -0.5, -0.5, 0.5, 0.0, 0.5]],
            Shape::Inner => &[
                [-0.5, -0.5, -0.5, 0.5, 0.0, 0.5],
                [-0.5, 0.0, 0.0, 0.5, 0.5, 0.5],
                [-0.5, 0.0, -0.5, 0.0, 0.5, 0.0],
            ],
            Shape::Outer => &[[-0.5, -0.5, -0.5, 0.5, 0.0, 0.5], [-0.5, 0.0, 0.0, 0.0, 0.5, 0.5]],
        }
    }

    /// The share of the block's burning time it burns for (the stairs
    /// mod's fuel recipes).
    fn fuel_share(self) -> f32 {
        match self {
            Shape::Stair => 0.75,
            Shape::Slab => 0.5,
            Shape::Inner => 0.875,
            Shape::Outer => 0.625,
        }
    }
}

/// A stair or slab's shape and material, or None for other nodes.
pub fn shape_of(id: u16) -> Option<(Shape, &'static Material)> {
    let set = SETS.iter().find(|s| (s.first..s.end()).contains(&id))?;
    let (k, i) = ((id - set.first) / set.count(), (id - set.first) % set.count());
    Some((SHAPES[k as usize], &set.materials[i as usize]))
}

/// The stair or slab made of `base`.
#[cfg(test)]
pub fn shaped(shape: Shape, base: u16) -> Option<u16> {
    let k = SHAPES.iter().position(|&s| s == shape)? as u16;
    SETS.iter().find_map(|s| s.materials.iter().position(|m| m.base == base).map(|i| s.first + k * s.count() + i as u16))
}

pub fn is_slab(id: u16) -> bool {
    matches!(shape_of(id), Some((Shape::Slab, _)))
}

/// How long a stair or slab burns: its share of its block's time, rounded
/// down (math.floor in the stairs mod).
pub fn burn_time(id: u16, base_time: impl Fn(u16) -> f32) -> f32 {
    match shape_of(id) {
        Some((shape, m)) => (base_time(m.base) * shape.fuel_share()).floor(),
        None => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Crafting
// ---------------------------------------------------------------------------

/// The stairs mod's recipes, for each material: a staircase of six blocks
/// makes 8 stairs, a row of three 6 slabs; four stairs in a square give 3
/// blocks back, and two slabs one above the other 1. Corner stairs: 7
/// inner from a staircase shape with a gap in the middle, 6 outer from a
/// T upside down; there's no way back from those. (width, pattern with 0
/// for empty and 1 for the item, which item, output, count.)
pub fn recipes() -> impl Iterator<Item = (usize, [u8; 9], u16, u16, u16)> {
    SETS.iter().flat_map(|s| s.materials.iter().enumerate().map(move |(i, m)| (s, i as u16, m))).flat_map(|(s, i, m)| {
        let (stair, slab) = (s.first_stair() + i, s.first_slab() + i);
        let (inner, outer) = (s.first_inner() + i, s.first_outer() + i);
        [
            (3, [0, 0, 1, 0, 1, 1, 1, 1, 1], m.base, stair, 8),
            (3, [1, 1, 1, 0, 0, 0, 0, 0, 0], m.base, slab, 6),
            (3, [0, 1, 0, 1, 0, 1, 1, 1, 1], m.base, inner, 7),
            (3, [0, 1, 0, 1, 1, 1, 0, 0, 0], m.base, outer, 6),
            (2, [1, 1, 1, 1, 0, 0, 0, 0, 0], stair, m.base, 3),
            (1, [1, 1, 0, 0, 0, 0, 0, 0, 0], slab, m.base, 1),
        ]
    })
}

// ---------------------------------------------------------------------------
// Placing
// ---------------------------------------------------------------------------

/// Eye height above the feet (player_api's eye_height).
const EYE_HEIGHT: f32 = 1.47;

/// Where the player is and looks: feet position in nodes, and pitch and yaw
/// in degrees as the client sends them.
pub struct Placer {
    pub feet: [f32; 3],
    pub pitch: f32,
    pub yaw: f32,
}

impl Placer {
    /// ObjectRef::get_look_dir.
    fn look_dir(&self) -> [f32; 3] {
        let p = (-self.pitch).to_radians();
        let y = (self.yaw + 90.0).to_radians();
        [p.cos() * y.cos(), p.sin(), p.cos() * y.sin()]
    }

    /// The height of the point on the face between `under` and `above`
    /// that the player is looking at (builtin's pointed_thing_to_face_pos).
    fn face_y(&self, under: [i16; 3], above: [i16; 3]) -> f32 {
        let Some(axis) = (0..3).find(|&i| above[i] != under[i]) else {
            return under[1] as f32;
        };
        let offset = (above[axis] - under[axis]) as f32 * 0.5;
        if axis == 1 {
            return under[1] as f32 + offset;
        }
        let camera = [self.feet[0], self.feet[1] + EYE_HEIGHT, self.feet[2]];
        let look = self.look_dir();
        let f = (under[axis] as f32 + offset - camera[axis]) / look[axis];
        camera[1] + look[1] * f
    }
}

/// minetest_game's rotate_and_place: facing away from the player, and
/// upside down when placed against the underside of a node or the upper
/// half of its side.
fn rotate_and_place(under: [i16; 3], above: [i16; 3], placer: &Placer) -> u8 {
    let diff = [0, 1, 2].map(|i| above[i] as f32 - placer.feet[i]);
    let mut param2 = facedir_param2(above, placer.feet);
    // Placed against the side of the node the player stands on.
    if under[1] == above[1] && diff[0].abs() <= 0.5 && diff[2].abs() <= 0.5 && diff[1] < 0.0 {
        param2 = (param2 + 2) % 4;
    }
    // Lua's % with a positive divisor: never negative, so only the first
    // two of the stairs mod's conditions can hold.
    let fpos = placer.face_y(under, above).rem_euclid(1.0);
    if under[1] - 1 == above[1] || (fpos > 0.0 && fpos < 0.5) {
        param2 += 20;
        param2 = match param2 {
            21 => 23,
            23 => 21,
            p => p,
        };
    }
    param2
}

/// The param2 for a stair or slab placed at `above` against `under`, which
/// holds `under_node` with `under_param2` (the stairs mod's on_place: a
/// slab against another slab takes its orientation, turned over when it
/// goes on top of an upside-down one or under an upright one).
pub fn place_param2(id: u16, under: [i16; 3], above: [i16; 3], under_node: u16, under_param2: u8, placer: &Placer) -> u8 {
    if !(is_slab(id) && is_slab(under_node)) {
        return rotate_and_place(under, above, placer);
    }
    let up = above[1] > under[1];
    let down = above[1] < under[1];
    match under_param2 {
        p if p >= 20 && up => p - 20,
        p if p <= 3 && down => p + 20,
        p => p,
    }
}

/// The block that a slab placed on the open face of the same kind of slab
/// makes of the two (older minetest_game's stairs mod, and Minecraft's
/// double slabs): its top when it is right way up, its underside when it
/// is upside down.
pub fn merged(id: u16, under: [i16; 3], above: [i16; 3], under_node: u16, under_param2: u8) -> Option<u16> {
    let (Shape::Slab, m) = shape_of(id)? else {
        return None;
    };
    let open_face = match under_param2 {
        0..=3 => under[1] + 1,
        20..=23 => under[1] - 1,
        _ => return None,
    };
    let face = above[0] == under[0] && above[2] == under[2] && above[1] == open_face;
    (under_node == id && face).then_some(m.base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placer(feet: [f32; 3], pitch: f32, yaw: f32) -> Placer {
        Placer { feet, pitch, yaw }
    }

    #[test]
    fn ids() {
        assert_eq!(shaped(Shape::Stair, CONTENT_BIRCH_WOOD), Some(41));
        assert_eq!(shaped(Shape::Slab, CONTENT_SANDSTONE_BLOCK), Some(60));
        assert_eq!(END, 81);
        assert!(matches!(shape_of(61), Some((Shape::Inner, m)) if m.base == CONTENT_BIRCH_WOOD));
        assert!(matches!(shape_of(80), Some((Shape::Outer, m)) if m.base == CONTENT_SANDSTONE_BLOCK));
        assert_eq!(burn_time(63, |_| 7.0), 6.0);
        assert_eq!(burn_time(73, |_| 7.0), 4.0);
        assert!(matches!(shape_of(45), Some((Shape::Stair, m)) if m.base == CONTENT_COBBLE));
        assert!(shape_of(40).is_none() && shape_of(81).is_none());
        assert_eq!(burn_time(41, |_| 5.0), 3.0);
        assert_eq!(burn_time(53, |_| 7.0), 3.0);
        // The later set, after the fences.
        assert_eq!(LATER.first, 175);
        assert_eq!(shaped(Shape::Slab, CONTENT_ACACIA_WOOD), Some(178));
        assert!(matches!(shape_of(182), Some((Shape::Outer, m)) if m.base == CONTENT_ACACIA_WOOD));
        assert!(shape_of(183).is_none() && shape_of(174).is_none());
        assert_eq!(BUILDING.first, 248);
        assert_eq!(BUILDING.end(), 332);
        assert_eq!(shaped(Shape::Outer, CONTENT_GOLD_BLOCK), Some(331));
    }

    #[test]
    fn placing() {
        let stair = FIRST_STAIR;
        // On top of the ground, standing to the south (-Z) of it: facing
        // away (+Z, facedir 0), right way up.
        let p = placer([0.0, 1.0, -3.0], 30.0, 0.0);
        assert_eq!(place_param2(stair, [0, 0, 0], [0, 1, 0], CONTENT_STONE, 0, &p), 0);
        // Against the underside of a block: upside down.
        assert_eq!(place_param2(stair, [0, 3, 0], [0, 2, 0], CONTENT_STONE, 0, &p), 20);
        // Against the side of a block, looking at its upper half or its
        // lower half. Yaw 0 looks along +Z; the face is at z = -0.5.
        let wall = ([0, 2, 0], [0, 2, -1]);
        let eye = 1.0 + EYE_HEIGHT;
        let looking_at = |y: f32| {
            let pitch = -((y - eye) / 2.5).atan().to_degrees();
            placer([0.0, 1.0, -3.0], pitch, 0.0)
        };
        assert_eq!(place_param2(stair, wall.0, wall.1, CONTENT_STONE, 0, &looking_at(2.3)), 20);
        assert_eq!(place_param2(stair, wall.0, wall.1, CONTENT_STONE, 0, &looking_at(1.7)), 0);
        // Facing west (-X) when the player stands to the east: facedir 3,
        // and upside down 3 + 20 = 23 swaps to 21.
        let east = placer([3.0, 1.0, 0.0], 0.0, 90.0);
        assert_eq!(place_param2(stair, [0, 3, 0], [0, 2, 0], CONTENT_STONE, 0, &east), 21);
        // Slabs on slabs.
        let slab = FIRST_SLAB;
        assert_eq!(place_param2(slab, [0, 1, 0], [0, 2, 0], slab, 22, &p), 2);
        assert_eq!(place_param2(slab, [0, 1, 0], [0, 0, 0], slab, 2, &p), 22);
        assert_eq!(place_param2(slab, [0, 1, 0], [1, 1, 0], slab + 1, 22, &p), 22);
    }

    #[test]
    fn double_slabs() {
        let slab = FIRST_SLAB + 4; // cobblestone
        // On top of an upright one, or under an upside-down one: a block.
        assert_eq!(merged(slab, [0, 1, 0], [0, 2, 0], slab, 2), Some(CONTENT_COBBLE));
        assert_eq!(merged(slab, [0, 1, 0], [0, 0, 0], slab, 21), Some(CONTENT_COBBLE));
        // Against the closed face, the side, or another kind: no.
        assert_eq!(merged(slab, [0, 1, 0], [0, 0, 0], slab, 2), None);
        assert_eq!(merged(slab, [0, 1, 0], [0, 2, 0], slab, 21), None);
        assert_eq!(merged(slab, [0, 1, 0], [1, 1, 0], slab, 0), None);
        assert_eq!(merged(slab, [0, 1, 0], [0, 2, 0], slab + 1, 0), None);
        assert_eq!(merged(FIRST_STAIR, [0, 1, 0], [0, 2, 0], FIRST_STAIR, 0), None);
    }
}
