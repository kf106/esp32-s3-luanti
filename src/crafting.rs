//! Crafting recipes and the 3x3 grid, as Luanti does it for minetest_game's
//! shaped recipes (CraftDefinitionShaped in src/craftdef.cpp): the items in
//! the grid, cut down to the box around them, must match a recipe's
//! pattern exactly (it can sit anywhere in the grid, but is not mirrored).
//! Each crafting uses up one item from every filled cell.

use crate::content::{self, *};
use crate::farming;
use crate::fences;
use crate::plants;
use crate::inventory::Stack;

pub const GRID_WIDTH: usize = 3;
pub const GRID_SIZE: usize = GRID_WIDTH * GRID_WIDTH;

/// One cell of a recipe pattern.
#[derive(Clone, Copy, Debug)]
pub enum Ingredient {
    Empty,
    Item(u16),
    /// Any item in this group ("group:wood" in a minetest_game recipe).
    Group(&'static str),
}

use Ingredient::{Empty as E, Group as G, Item as I};

pub struct Recipe {
    pub width: usize,
    /// Rows of `width` cells, top row first.
    pub pattern: &'static [Ingredient],
    pub output: u16,
    pub count: u16,
}

/// minetest_game's recipes for the items we have (crafting.lua,
/// craftitems.lua, torch.lua, tools.lua).
pub const RECIPES: &[Recipe] = &[
    // Trunk to four planks.
    Recipe { width: 1, pattern: &[I(CONTENT_BIRCH_TREE)], output: CONTENT_BIRCH_WOOD, count: 4 },
    Recipe { width: 1, pattern: &[I(CONTENT_FIR_TREE)], output: CONTENT_FIR_WOOD, count: 4 },
    Recipe { width: 1, pattern: &[I(CONTENT_APPLE_TREE)], output: CONTENT_APPLE_WOOD, count: 4 },
    Recipe { width: 1, pattern: &[I(CONTENT_JUNGLE_TREE)], output: CONTENT_JUNGLE_WOOD, count: 4 },
    Recipe { width: 1, pattern: &[I(CONTENT_ACACIA_TREE)], output: CONTENT_ACACIA_WOOD, count: 4 },
    // A bush stem to one plank of its wood.
    Recipe { width: 1, pattern: &[I(plants::BUSH_STEM)], output: CONTENT_APPLE_WOOD, count: 1 },
    Recipe { width: 1, pattern: &[I(plants::ACACIA_BUSH_STEM)], output: CONTENT_ACACIA_WOOD, count: 1 },
    Recipe { width: 1, pattern: &[I(plants::FIR_BUSH_STEM)], output: CONTENT_FIR_WOOD, count: 1 },
    // Five cactus in a plus: a large cactus seedling.
    Recipe {
        width: 3,
        pattern: &[E, I(plants::CACTUS), E, I(plants::CACTUS), I(plants::CACTUS), I(plants::CACTUS), E, I(plants::CACTUS), E],
        output: plants::LARGE_CACTUS_SEEDLING,
        count: 1,
    },
    // Any planks to four sticks.
    Recipe { width: 1, pattern: &[G("wood")], output: ITEM_STICK, count: 4 },
    // Coal lump on a stick: four torches.
    Recipe { width: 1, pattern: &[I(ITEM_COAL_LUMP), G("stick")], output: CONTENT_TORCH, count: 4 },
    // Four sand to sandstone, and back.
    Recipe {
        width: 2,
        pattern: &[I(CONTENT_SAND), I(CONTENT_SAND), I(CONTENT_SAND), I(CONTENT_SAND)],
        output: CONTENT_SANDSTONE,
        count: 1,
    },
    Recipe { width: 1, pattern: &[I(CONTENT_SANDSTONE)], output: CONTENT_SAND, count: 4 },
    // Bricks from four, blocks from nine (crafting.lua).
    Recipe { width: 2, pattern: &[I(CONTENT_STONE); 4], output: CONTENT_STONE_BRICK, count: 4 },
    Recipe { width: 3, pattern: &[I(CONTENT_STONE); 9], output: CONTENT_STONE_BLOCK, count: 9 },
    Recipe { width: 2, pattern: &[I(CONTENT_SANDSTONE); 4], output: CONTENT_SANDSTONE_BRICK, count: 4 },
    Recipe { width: 3, pattern: &[I(CONTENT_SANDSTONE); 9], output: CONTENT_SANDSTONE_BLOCK, count: 9 },
    // Nine lumps or ingots to a block, and back (crafting.lua,
    // craftitems.lua).
    Recipe { width: 3, pattern: &[I(ITEM_COAL_LUMP); 9], output: CONTENT_COAL_BLOCK, count: 1 },
    Recipe { width: 1, pattern: &[I(CONTENT_COAL_BLOCK)], output: ITEM_COAL_LUMP, count: 9 },
    Recipe { width: 3, pattern: &[I(ITEM_STEEL_INGOT); 9], output: CONTENT_STEEL_BLOCK, count: 1 },
    Recipe { width: 1, pattern: &[I(CONTENT_STEEL_BLOCK)], output: ITEM_STEEL_INGOT, count: 9 },
    // Ladders: an H of sticks makes 5, of steel ingots 15.
    Recipe { width: 3, pattern: &ladder(G("stick")), output: CONTENT_LADDER_WOOD, count: 5 },
    Recipe { width: 3, pattern: &ladder(I(ITEM_STEEL_INGOT)), output: CONTENT_LADDER_STEEL, count: 15 },
    // Tools: planks for wooden ones, stone or cobblestone for stone ones.
    Recipe { width: 3, pattern: &pick(G("wood")), output: TOOL_PICK_WOOD, count: 1 },
    Recipe { width: 3, pattern: &pick(G("stone")), output: TOOL_PICK_STONE, count: 1 },
    Recipe { width: 1, pattern: &shovel(G("wood")), output: TOOL_SHOVEL_WOOD, count: 1 },
    Recipe { width: 1, pattern: &shovel(G("stone")), output: TOOL_SHOVEL_STONE, count: 1 },
    Recipe { width: 2, pattern: &axe(G("wood")), output: TOOL_AXE_WOOD, count: 1 },
    Recipe { width: 2, pattern: &axe(G("stone")), output: TOOL_AXE_STONE, count: 1 },
    Recipe { width: 1, pattern: &sword(G("wood")), output: TOOL_SWORD_WOOD, count: 1 },
    Recipe { width: 1, pattern: &sword(G("stone")), output: TOOL_SWORD_STONE, count: 1 },
    Recipe { width: 3, pattern: &pick(I(ITEM_STEEL_INGOT)), output: TOOL_PICK_STEEL, count: 1 },
    Recipe { width: 1, pattern: &shovel(I(ITEM_STEEL_INGOT)), output: TOOL_SHOVEL_STEEL, count: 1 },
    Recipe { width: 2, pattern: &axe(I(ITEM_STEEL_INGOT)), output: TOOL_AXE_STEEL, count: 1 },
    Recipe { width: 1, pattern: &sword(I(ITEM_STEEL_INGOT)), output: TOOL_SWORD_STEEL, count: 1 },
    // Chests: a ring of planks, with a steel ingot in the middle for a
    // locked one (chests.lua).
    Recipe { width: 3, pattern: &ring(G("wood"), E), output: CONTENT_CHEST, count: 1 },
    Recipe { width: 3, pattern: &ring(G("wood"), I(ITEM_STEEL_INGOT)), output: CONTENT_CHEST_LOCKED, count: 1 },
    // Doors: two columns of three planks, steel ingots or glass
    // (doors/init.lua).
    Recipe { width: 2, pattern: &[G("wood"); 6], output: ITEM_DOOR_WOOD, count: 1 },
    Recipe { width: 2, pattern: &[I(ITEM_STEEL_INGOT); 6], output: ITEM_DOOR_STEEL, count: 1 },
    Recipe { width: 2, pattern: &[I(CONTENT_GLASS); 6], output: ITEM_DOOR_GLASS, count: 1 },
    // Trapdoors: two rows of three planks make 2, a square of four steel
    // ingots 1.
    Recipe { width: 3, pattern: &[G("wood"); 6], output: crate::doors::FIRST_TRAPDOOR, count: 2 },
    Recipe { width: 2, pattern: &[I(ITEM_STEEL_INGOT); 4], output: crate::doors::FIRST_TRAPDOOR + 2, count: 1 },
    // Farming (farming/init.lua, hoes.lua): hoes, wool and string from
    // cotton, straw from wheat and back.
    Recipe { width: 2, pattern: &hoe(G("wood")), output: TOOL_HOE_WOOD, count: 1 },
    Recipe { width: 2, pattern: &hoe(G("stone")), output: TOOL_HOE_STONE, count: 1 },
    Recipe { width: 2, pattern: &hoe(I(ITEM_STEEL_INGOT)), output: TOOL_HOE_STEEL, count: 1 },
    Recipe { width: 2, pattern: &[I(ITEM_COTTON); 4], output: farming::WOOL_WHITE, count: 1 },
    Recipe { width: 1, pattern: &[I(ITEM_COTTON); 2], output: ITEM_STRING, count: 2 },
    Recipe { width: 3, pattern: &[I(ITEM_WHEAT); 9], output: farming::STRAW, count: 3 },
    Recipe { width: 1, pattern: &[I(farming::STRAW)], output: ITEM_WHEAT, count: 3 },
    // Beds (beds.lua): three wool on three planks; the fancy bed has a
    // stick on top at the right.
    Recipe {
        width: 3,
        pattern: &[I(farming::WOOL_WHITE), I(farming::WOOL_WHITE), I(farming::WOOL_WHITE), G("wood"), G("wood"), G("wood")],
        output: crate::beds::BED_BOTTOM,
        count: 1,
    },
    Recipe {
        width: 3,
        pattern: &[
            E,
            E,
            I(ITEM_STICK),
            I(farming::WOOL_WHITE),
            I(farming::WOOL_WHITE),
            I(farming::WOOL_WHITE),
            G("wood"),
            G("wood"),
            G("wood"),
        ],
        output: crate::beds::FANCY_BOTTOM,
        count: 1,
    },
    // Fences: planks either side of sticks, two rows, make 4
    // (default.register_fence); gates: sticks either side of planks
    // (doors.register_fencegate).
    Recipe { width: 3, pattern: &fence(CONTENT_APPLE_WOOD), output: fences::FENCE_APPLE, count: 4 },
    Recipe { width: 3, pattern: &fence(CONTENT_FIR_WOOD), output: fences::FENCE_FIR, count: 4 },
    Recipe { width: 3, pattern: &fence(CONTENT_BIRCH_WOOD), output: fences::FENCE_BIRCH, count: 4 },
    Recipe { width: 3, pattern: &fence(CONTENT_JUNGLE_WOOD), output: fences::FENCE_JUNGLE, count: 4 },
    Recipe { width: 3, pattern: &fence(CONTENT_ACACIA_WOOD), output: fences::FENCE_ACACIA, count: 4 },
    // Rails: two planks above two more, a row apart, make 16
    // (default.register_fence_rail).
    Recipe { width: 2, pattern: &rail(CONTENT_APPLE_WOOD), output: fences::RAIL_APPLE, count: 16 },
    Recipe { width: 2, pattern: &rail(CONTENT_FIR_WOOD), output: fences::RAIL_FIR, count: 16 },
    Recipe { width: 2, pattern: &rail(CONTENT_BIRCH_WOOD), output: fences::RAIL_BIRCH, count: 16 },
    Recipe { width: 2, pattern: &rail(CONTENT_JUNGLE_WOOD), output: fences::RAIL_JUNGLE, count: 16 },
    Recipe { width: 2, pattern: &rail(CONTENT_ACACIA_WOOD), output: fences::RAIL_ACACIA, count: 16 },
    Recipe { width: 3, pattern: &gate(CONTENT_APPLE_WOOD), output: fences::GATE_APPLE, count: 1 },
    Recipe { width: 3, pattern: &gate(CONTENT_FIR_WOOD), output: fences::GATE_FIR, count: 1 },
    Recipe { width: 3, pattern: &gate(CONTENT_BIRCH_WOOD), output: fences::GATE_BIRCH, count: 1 },
    Recipe { width: 3, pattern: &gate(CONTENT_JUNGLE_WOOD), output: fences::GATE_JUNGLE, count: 1 },
    Recipe { width: 3, pattern: &gate(CONTENT_ACACIA_WOOD), output: fences::GATE_ACACIA, count: 1 },
    // A bucket: three steel ingots in a V (bucket mod).
    Recipe {
        width: 3,
        pattern: &[I(ITEM_STEEL_INGOT), E, I(ITEM_STEEL_INGOT), E, I(ITEM_STEEL_INGOT), E],
        output: ITEM_BUCKET_EMPTY,
        count: 1,
    },
    // A ring of stone (furnace.lua).
    Recipe {
        width: 3,
        pattern: &[G("stone"), G("stone"), G("stone"), G("stone"), E, G("stone"), G("stone"), G("stone"), G("stone")],
        output: CONTENT_FURNACE,
        count: 1,
    },
];

/// What a furnace makes of an item, and how long it takes in seconds
/// (minetest_game's cooking recipes, all with Luanti's default 3 s).
pub fn cook_result(id: u16) -> Option<(u16, f32)> {
    match id {
        ITEM_IRON_LUMP => Some((ITEM_STEEL_INGOT, 3.0)),
        CONTENT_COBBLE => Some((CONTENT_STONE, 3.0)),
        // group:sand: glass.
        CONTENT_SAND => Some((CONTENT_GLASS, 3.0)),
        // farming: flour bakes into bread, slowly.
        ITEM_FLOUR => Some((ITEM_BREAD, 15.0)),
        _ => None,
    }
}

/// How long an item burns in a furnace, in seconds (0: it isn't fuel).
/// minetest_game's fuel recipes, with birch as its aspen and fir as its
/// pine.
pub fn burn_time(id: u16) -> f32 {
    match id {
        ITEM_COAL_LUMP => 40.0,
        CONTENT_FIR_TREE => 26.0,
        CONTENT_APPLE_TREE => 30.0,
        CONTENT_APPLE_WOOD => 7.0,
        CONTENT_APPLE_SAPLING => 5.0,
        CONTENT_APPLE_LEAVES => 4.0,
        CONTENT_BIRCH_TREE => 22.0,
        CONTENT_FIR_WOOD => 6.0,
        CONTENT_BIRCH_WOOD => 5.0,
        CONTENT_BIRCH_LEAVES | CONTENT_FIR_NEEDLES | CONTENT_JUNGLE_LEAVES | CONTENT_ACACIA_LEAVES => 4.0,
        CONTENT_JUNGLE_TREE => 38.0,
        CONTENT_ACACIA_TREE => 34.0,
        CONTENT_JUNGLE_WOOD => 9.0,
        CONTENT_ACACIA_WOOD => 8.0,
        CONTENT_JUNGLE_SAPLING | CONTENT_ACACIA_SAPLING => 6.0,
        farming::JUNGLEGRASS => 3.0,
        plants::BUSH_LEAVES | plants::BLUEBERRY_LEAVES | plants::BLUEBERRY_LEAVES_WITH_BERRIES | plants::ACACIA_BUSH_LEAVES
        | plants::FIR_BUSH_NEEDLES => 4.0,
        plants::BUSH_STEM => 7.0,
        plants::ACACIA_BUSH_STEM => 8.0,
        plants::FIR_BUSH_STEM => 6.0,
        plants::BUSH_SAPLING => 3.0,
        plants::ACACIA_BUSH_SAPLING => 4.0,
        plants::FIR_BUSH_SAPLING => 2.0,
        plants::CACTUS => 15.0,
        plants::LARGE_CACTUS_SEEDLING => 5.0,
        plants::PAPYRUS => 3.0,
        id if id == plants::FERN[0] || id == plants::MARRAM[0] => 2.0,
        CONTENT_TORCH => 4.0,
        CONTENT_BIRCH_SAPLING => 4.0,
        CONTENT_FIR_SAPLING => 5.0,
        ITEM_STICK => 1.0,
        TOOL_PICK_WOOD | TOOL_AXE_WOOD => 6.0,
        TOOL_SWORD_WOOD => 5.0,
        TOOL_SHOVEL_WOOD => 4.0,
        CONTENT_CHEST | CONTENT_CHEST_LOCKED => 30.0,
        CONTENT_COAL_BLOCK => 370.0,
        CONTENT_LADDER_WOOD => 7.0,
        ITEM_DOOR_WOOD => 14.0,
        ITEM_WHEAT | ITEM_COTTON | ITEM_STRING => 1.0,
        TOOL_HOE_WOOD => 5.0,
        farming::STRAW => 3.0,
        id if id == farming::GRASS[0] || id == farming::DRY_GRASS[0] => 2.0,
        crate::doors::FIRST_TRAPDOOR => 7.0,
        // A lava bucket burns for a minute (the bucket mod), leaving the
        // empty bucket (see furnace.rs).
        ITEM_BUCKET_LAVA => 60.0,
        crate::beds::BED_BOTTOM => 12.0,
        crate::beds::FANCY_BOTTOM => 13.0,
        id if fences::burn_time(id) > 0.0 => fences::burn_time(id),
        id => crate::stairs::burn_time(id, burn_time),
    }
}

/// A fence: two rows of planks, stick, planks.
const fn fence(planks: u16) -> [Ingredient; 6] {
    [I(planks), G("stick"), I(planks), I(planks), G("stick"), I(planks)]
}

/// A rail: two planks, an empty row, two planks.
const fn rail(planks: u16) -> [Ingredient; 6] {
    [I(planks), I(planks), E, E, I(planks), I(planks)]
}

/// A gate: two rows of stick, planks, stick.
const fn gate(planks: u16) -> [Ingredient; 6] {
    [G("stick"), I(planks), G("stick"), G("stick"), I(planks), G("stick")]
}

/// minetest_game's ladder shape: two sides joined in the middle.
const fn ladder(m: Ingredient) -> [Ingredient; 9] {
    [m, E, m, m, m, m, m, E, m]
}

/// Eight of `m` round the edge of the grid, `middle` in the middle.
const fn ring(m: Ingredient, middle: Ingredient) -> [Ingredient; 9] {
    [m, m, m, m, middle, m, m, m, m]
}

/// A shapeless recipe: these items, one each, anywhere in the grid and
/// nothing else (CraftDefinitionShapeless).
pub struct Shapeless {
    pub items: &'static [Ingredient],
    pub output: u16,
    pub count: u16,
}

/// minetest_game's shapeless recipes for what we have.
pub const SHAPELESS: &[Shapeless] = &[
    // A chest and a steel ingot make a locked chest (chests.lua).
    Shapeless { items: &[I(CONTENT_CHEST), I(ITEM_STEEL_INGOT)], output: CONTENT_CHEST_LOCKED, count: 1 },
    // Four wheat, anyhow, make flour (farming/init.lua).
    Shapeless { items: &[I(ITEM_WHEAT); 4], output: ITEM_FLOUR, count: 1 },
];

impl Shapeless {
    fn fits(&self, grid: &[Option<Stack>; GRID_SIZE]) -> bool {
        let filled: Vec<Stack> = grid.iter().flatten().copied().collect();
        if filled.len() != self.items.len() {
            return false;
        }
        // Match each ingredient to a different filled cell (few enough to
        // try greedily, most specific ingredients first).
        let mut used = vec![false; filled.len()];
        let mut wanted: Vec<Ingredient> = self.items.to_vec();
        wanted.sort_by_key(|i| matches!(i, G(_)));
        wanted.iter().all(|&want| {
            let found = filled.iter().enumerate().find(|(n, s)| !used[*n] && matches(Some(**s), want));
            found.map(|(n, _)| used[n] = true).is_some()
        })
    }
}

// The tool shapes of tools.lua, for a material.
const STICK: Ingredient = G("stick");
const fn pick(m: Ingredient) -> [Ingredient; 9] {
    [m, m, m, E, STICK, E, E, STICK, E]
}
const fn shovel(m: Ingredient) -> [Ingredient; 3] {
    [m, STICK, STICK]
}
const fn axe(m: Ingredient) -> [Ingredient; 6] {
    [m, m, m, STICK, E, STICK]
}
const fn sword(m: Ingredient) -> [Ingredient; 3] {
    [m, m, STICK]
}
const fn hoe(m: Ingredient) -> [Ingredient; 6] {
    [m, m, E, STICK, E, STICK]
}

/// The box (min x, min y, width, height) around the filled cells of a
/// grid of `width` columns, or None if it is empty (craftGetBounds).
fn bounds(filled: impl Iterator<Item = bool>, width: usize) -> Option<(usize, usize, usize, usize)> {
    let mut b: Option<(usize, usize, usize, usize)> = None;
    for (i, f) in filled.enumerate() {
        if !f {
            continue;
        }
        let (x, y) = (i % width, i / width);
        b = Some(match b {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    }
    b.map(|(x0, y0, x1, y1)| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

fn matches(cell: Option<Stack>, want: Ingredient) -> bool {
    match (cell, want) {
        (None, E) => true,
        (Some(s), I(id)) => s.id == id,
        (Some(s), G(group)) => content::group_of(s.id, group) > 0,
        _ => false,
    }
}

impl Recipe {
    fn fits(&self, grid: &[Option<Stack>; GRID_SIZE]) -> bool {
        fits(grid, self.width, self.pattern)
    }
}

/// Whether the grid holds `pattern` (rows of `width` cells), anywhere in it.
fn fits(grid: &[Option<Stack>; GRID_SIZE], width: usize, pattern: &[Ingredient]) -> bool {
    let Some((gx, gy, gw, gh)) = bounds(grid.iter().map(Option::is_some), GRID_WIDTH) else {
        return false;
    };
    let Some((rx, ry, rw, rh)) = bounds(pattern.iter().map(|c| !matches!(c, E)), width) else {
        return false;
    };
    if (gw, gh) != (rw, rh) {
        return false;
    }
    (0..gh).all(|y| {
        (0..gw).all(|x| {
            let cell = grid[(gy + y) * GRID_WIDTH + gx + x];
            matches(cell, pattern[(ry + y) * width + rx + x])
        })
    })
}

/// The stairs mod's recipes (see stairs::recipes).
fn stair_result(grid: &[Option<Stack>; GRID_SIZE]) -> Option<(u16, u16)> {
    crate::stairs::recipes()
        .find(|(width, cells, item, _, _)| {
            let pattern = cells.map(|c| if c == 0 { E } else { I(*item) });
            // Empty cells after the pattern change nothing: only the box
            // round the filled ones counts.
            fits(grid, *width, &pattern)
        })
        .map(|(_, _, _, output, count)| (output, count))
}

/// What the grid would make: (item, count).
pub fn result(grid: &[Option<Stack>; GRID_SIZE]) -> Option<(u16, u16)> {
    RECIPES
        .iter()
        .find(|r| r.fits(grid))
        .map(|r| (r.output, r.count))
        .or_else(|| SHAPELESS.iter().find(|r| r.fits(grid)).map(|r| (r.output, r.count)))
        .or_else(|| stair_result(grid))
}

/// Craft once: use one item from each filled cell and give the result.
pub fn craft(grid: &mut [Option<Stack>; GRID_SIZE]) -> Option<(u16, u16)> {
    let made = result(grid)?;
    for cell in grid.iter_mut() {
        if let Some(s) = cell {
            s.count -= 1;
            if s.count == 0 {
                *cell = None;
            }
        }
    }
    Some(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(cells: &[(usize, u16, u16)]) -> [Option<Stack>; GRID_SIZE] {
        let mut g = [None; GRID_SIZE];
        for &(i, id, count) in cells {
            g[i] = Some(Stack::new(id, count));
        }
        g
    }

    #[test]
    fn anywhere_in_the_grid() {
        assert_eq!(result(&grid(&[(4, CONTENT_BIRCH_TREE, 1)])), Some((CONTENT_BIRCH_WOOD, 4)));
        assert_eq!(result(&grid(&[(8, CONTENT_FIR_WOOD, 1)])), Some((ITEM_STICK, 4)));
        // Coal above a stick, in the right-hand column.
        assert_eq!(result(&grid(&[(2, ITEM_COAL_LUMP, 1), (5, ITEM_STICK, 1)])), Some((CONTENT_TORCH, 4)));
        // Upside down is no torch.
        assert_eq!(result(&grid(&[(5, ITEM_COAL_LUMP, 1), (2, ITEM_STICK, 1)])), None);
        // Not in line.
        assert_eq!(result(&grid(&[(0, ITEM_COAL_LUMP, 1), (4, ITEM_STICK, 1)])), None);
    }

    #[test]
    fn tools() {
        let (w, c, s) = (CONTENT_BIRCH_WOOD, CONTENT_COBBLE, ITEM_STICK);
        let pick = grid(&[(0, w, 1), (1, CONTENT_FIR_WOOD, 1), (2, w, 1), (4, s, 1), (7, s, 1)]);
        assert_eq!(result(&pick), Some((TOOL_PICK_WOOD, 1)));
        let pick = grid(&[(0, c, 1), (1, CONTENT_STONE, 1), (2, c, 1), (4, s, 1), (7, s, 1)]);
        assert_eq!(result(&pick), Some((TOOL_PICK_STONE, 1)));
        // Mixed wood and stone makes nothing.
        let mixed = grid(&[(0, c, 1), (1, w, 1), (2, c, 1), (4, s, 1), (7, s, 1)]);
        assert_eq!(result(&mixed), None);
        // Axe in the right two columns; its mirror image is no axe.
        assert_eq!(result(&grid(&[(1, w, 1), (2, w, 1), (4, w, 1), (5, s, 1), (8, s, 1)])), Some((TOOL_AXE_WOOD, 1)));
        assert_eq!(result(&grid(&[(1, w, 1), (2, w, 1), (5, w, 1), (4, s, 1), (7, s, 1)])), None);
        assert_eq!(result(&grid(&[(2, c, 1), (5, s, 1), (8, s, 1)])), Some((TOOL_SHOVEL_STONE, 1)));
        assert_eq!(result(&grid(&[(0, w, 1), (3, w, 1), (6, s, 1)])), Some((TOOL_SWORD_WOOD, 1)));
        // Furnace: a ring of stone and cobblestone, mixed.
        let ring: Vec<(usize, u16, u16)> =
            [0, 1, 2, 3, 5, 6, 7, 8].iter().map(|&i| (i, if i % 2 == 0 { c } else { CONTENT_STONE }, 1)).collect();
        assert_eq!(result(&grid(&ring)), Some((CONTENT_FURNACE, 1)));
        // Chests from planks; locked with a steel ingot in the middle, or
        // from a chest and an ingot anywhere.
        let planks: Vec<(usize, u16, u16)> = [0, 1, 2, 3, 5, 6, 7, 8].iter().map(|&i| (i, w, 1)).collect();
        assert_eq!(result(&grid(&planks)), Some((CONTENT_CHEST, 1)));
        let mut locked = planks.clone();
        locked.push((4, ITEM_STEEL_INGOT, 1));
        assert_eq!(result(&grid(&locked)), Some((CONTENT_CHEST_LOCKED, 1)));
        assert_eq!(result(&grid(&[(7, CONTENT_CHEST, 1), (2, ITEM_STEEL_INGOT, 1)])), Some((CONTENT_CHEST_LOCKED, 1)));
        assert_eq!(result(&grid(&[(7, CONTENT_CHEST, 1), (2, ITEM_STEEL_INGOT, 1), (3, ITEM_STICK, 1)])), None);
        // Bricks, blocks, ladders.
        let two: Vec<(usize, u16, u16)> = [4, 5, 7, 8].iter().map(|&i| (i, CONTENT_STONE, 1)).collect();
        assert_eq!(result(&grid(&two)), Some((CONTENT_STONE_BRICK, 4)));
        let nine: Vec<(usize, u16, u16)> = (0..9).map(|i| (i, ITEM_STEEL_INGOT, 1)).collect();
        assert_eq!(result(&grid(&nine)), Some((CONTENT_STEEL_BLOCK, 1)));
        let h: Vec<(usize, u16, u16)> = [0, 2, 3, 4, 5, 6, 8].iter().map(|&i| (i, s, 1)).collect();
        assert_eq!(result(&grid(&h)), Some((CONTENT_LADDER_WOOD, 5)));
        assert_eq!(cook_result(CONTENT_SAND), Some((CONTENT_GLASS, 3.0)));
        let mut full = ring.clone();
        full.push((4, c, 1));
        assert_eq!(result(&grid(&full)), None);
    }

    #[test]
    fn stairs_and_slabs() {
        use crate::stairs::{shaped, Shape};
        let c = CONTENT_COBBLE;
        let stair = shaped(Shape::Stair, c).unwrap();
        let slab = shaped(Shape::Slab, c).unwrap();
        let steps: Vec<(usize, u16, u16)> = [2, 4, 5, 6, 7, 8].iter().map(|&i| (i, c, 1)).collect();
        assert_eq!(result(&grid(&steps)), Some((stair, 8)));
        // Mirrored: no stairs.
        let mirrored: Vec<(usize, u16, u16)> = [0, 3, 4, 6, 7, 8].iter().map(|&i| (i, c, 1)).collect();
        assert_eq!(result(&grid(&mirrored)), None);
        assert_eq!(result(&grid(&[(3, c, 1), (4, c, 1), (5, c, 1)])), Some((slab, 6)));
        let four: Vec<(usize, u16, u16)> = [1, 2, 4, 5].iter().map(|&i| (i, stair, 1)).collect();
        assert_eq!(result(&grid(&four)), Some((c, 3)));
        assert_eq!(result(&grid(&[(2, slab, 1), (5, slab, 1)])), Some((c, 1)));
        // Side by side, two slabs are nothing.
        assert_eq!(result(&grid(&[(4, slab, 1), (5, slab, 1)])), None);
        // Wooden ones are fuel, for their share of the planks' time.
        assert_eq!(burn_time(shaped(Shape::Stair, CONTENT_APPLE_WOOD).unwrap()), 5.0);
        assert_eq!(burn_time(stair), 0.0);
        // Corner stairs: 7 inner, 6 outer.
        let inner: Vec<(usize, u16, u16)> = [1, 3, 5, 6, 7, 8].iter().map(|&i| (i, c, 1)).collect();
        assert_eq!(result(&grid(&inner)), Some((shaped(Shape::Inner, c).unwrap(), 7)));
        let outer: Vec<(usize, u16, u16)> = [4, 6, 7, 8].iter().map(|&i| (i, c, 1)).collect();
        assert_eq!(result(&grid(&outer)), Some((shaped(Shape::Outer, c).unwrap(), 6)));
        // Two stone in a row are still nothing, three a slab.
        assert_eq!(result(&grid(&[(0, CONTENT_STONE, 1), (1, CONTENT_STONE, 1)])), None);
    }

    #[test]
    fn farming() {
        let (w, s, c) = (CONTENT_BIRCH_WOOD, ITEM_STICK, ITEM_COTTON);
        assert_eq!(result(&grid(&[(1, w, 1), (2, w, 1), (5, s, 1), (8, s, 1)])), Some((TOOL_HOE_WOOD, 1)));
        // The axe's shape is still an axe.
        assert_eq!(result(&grid(&[(1, w, 1), (2, w, 1), (4, w, 1), (5, s, 1), (8, s, 1)])), Some((TOOL_AXE_WOOD, 1)));
        assert_eq!(result(&grid(&[(0, c, 1), (1, c, 1), (3, c, 1), (4, c, 1)])), Some((farming::WOOL_WHITE, 1)));
        assert_eq!(result(&grid(&[(2, c, 1), (5, c, 1)])), Some((ITEM_STRING, 2)));
        assert_eq!(result(&grid(&[(0, ITEM_WHEAT, 1), (4, ITEM_WHEAT, 1), (6, ITEM_WHEAT, 1), (8, ITEM_WHEAT, 1)])), Some((ITEM_FLOUR, 1)));
        let nine: Vec<(usize, u16, u16)> = (0..9).map(|i| (i, ITEM_WHEAT, 1)).collect();
        assert_eq!(result(&grid(&nine)), Some((farming::STRAW, 3)));
        assert_eq!(cook_result(ITEM_FLOUR), Some((ITEM_BREAD, 15.0)));
        assert_eq!(burn_time(farming::GRASS[0]), 2.0);
    }

    #[test]
    fn beds() {
        let (w, a, x) = (farming::WOOL_WHITE, CONTENT_APPLE_WOOD, CONTENT_FIR_WOOD);
        let simple = [(3, w, 1), (4, w, 1), (5, w, 1), (6, a, 1), (7, x, 1), (8, a, 1)];
        assert_eq!(result(&grid(&simple)), Some((crate::beds::BED_BOTTOM, 1)));
        let mut fancy = simple.to_vec();
        fancy.push((2, ITEM_STICK, 1));
        assert_eq!(result(&grid(&fancy)), Some((crate::beds::FANCY_BOTTOM, 1)));
        // Six planks are still trapdoors.
        let planks: Vec<(usize, u16, u16)> = (3..9).map(|i| (i, a, 1)).collect();
        assert_eq!(result(&grid(&planks)), Some((crate::doors::FIRST_TRAPDOOR, 2)));
        assert_eq!(burn_time(crate::beds::FANCY_BOTTOM), 13.0);
    }

    #[test]
    fn fences() {
        let (a, f, s) = (CONTENT_APPLE_WOOD, CONTENT_FIR_WOOD, ITEM_STICK);
        let fence = [(0, f, 1), (1, s, 1), (2, f, 1), (3, f, 1), (4, s, 1), (5, f, 1)];
        assert_eq!(result(&grid(&fence)), Some((fences::FENCE_FIR, 4)));
        let gate = [(3, s, 1), (4, a, 1), (5, s, 1), (6, s, 1), (7, a, 1), (8, s, 1)];
        assert_eq!(result(&grid(&gate)), Some((fences::GATE_APPLE, 1)));
        // Woods don't mix.
        let mixed = [(0, f, 1), (1, s, 1), (2, a, 1), (3, f, 1), (4, s, 1), (5, f, 1)];
        assert_eq!(result(&grid(&mixed)), None);
        assert_eq!(burn_time(fences::FENCE_BIRCH), 5.0);
        assert_eq!(burn_time(fences::GATE_APPLE), 7.0);
        assert_eq!(burn_time(fences::GATE_APPLE + 1), 0.0);
        let rail = [(0, a, 1), (1, a, 1), (6, a, 1), (7, a, 1)];
        assert_eq!(result(&grid(&rail)), Some((fences::RAIL_APPLE, 16)));
        assert_eq!(burn_time(fences::RAIL_BIRCH), 3.0);
        let j = CONTENT_JUNGLE_WOOD;
        let rail = [(0, j, 1), (1, j, 1), (6, j, 1), (7, j, 1)];
        assert_eq!(result(&grid(&rail)), Some((fences::RAIL_JUNGLE, 16)));
        assert_eq!(result(&grid(&[(4, CONTENT_ACACIA_TREE, 1)])), Some((CONTENT_ACACIA_WOOD, 4)));
        assert_eq!(burn_time(fences::GATE_JUNGLE), 9.0);
        assert_eq!(burn_time(crate::stairs::LATER.first_slab() + 1), 4.0);
        let c = plants::CACTUS;
        let plus = [(1, c, 1), (3, c, 1), (4, c, 1), (5, c, 1), (7, c, 1)];
        assert_eq!(result(&grid(&plus)), Some((plants::LARGE_CACTUS_SEEDLING, 1)));
        assert_eq!(result(&grid(&[(0, plants::FIR_BUSH_STEM, 1)])), Some((CONTENT_FIR_WOOD, 1)));
        assert_eq!(burn_time(plants::CACTUS), 15.0);
    }

    #[test]
    fn uses_one_of_each() {
        let mut g = grid(&[(0, ITEM_COAL_LUMP, 3), (3, ITEM_STICK, 1)]);
        assert_eq!(craft(&mut g), Some((CONTENT_TORCH, 4)));
        assert_eq!(g[0], Some(Stack::new(ITEM_COAL_LUMP, 2)));
        assert_eq!(g[3], None);
        assert_eq!(craft(&mut g), None);
    }
}
