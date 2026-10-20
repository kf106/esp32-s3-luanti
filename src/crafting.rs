//! Crafting recipes and the 3x3 grid, as Luanti does it for minetest_game's
//! shaped recipes (CraftDefinitionShaped in src/craftdef.cpp): the items in
//! the grid, cut down to the box around them, must match a recipe's
//! pattern exactly (it can sit anywhere in the grid, but is not mirrored).
//! Each crafting uses up one item from every filled cell.

use crate::content::{self, *};
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
        CONTENT_BIRCH_LEAVES | CONTENT_FIR_NEEDLES => 4.0,
        CONTENT_TORCH => 4.0,
        CONTENT_BIRCH_SAPLING => 4.0,
        CONTENT_FIR_SAPLING => 5.0,
        ITEM_STICK => 1.0,
        TOOL_PICK_WOOD | TOOL_AXE_WOOD => 6.0,
        TOOL_SWORD_WOOD => 5.0,
        TOOL_SHOVEL_WOOD => 4.0,
        _ => 0.0,
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
        let Some((gx, gy, gw, gh)) = bounds(grid.iter().map(Option::is_some), GRID_WIDTH) else {
            return false;
        };
        let Some((rx, ry, rw, rh)) = bounds(self.pattern.iter().map(|c| !matches!(c, E)), self.width) else {
            return false;
        };
        if (gw, gh) != (rw, rh) {
            return false;
        }
        (0..gh).all(|y| {
            (0..gw).all(|x| {
                let cell = grid[(gy + y) * GRID_WIDTH + gx + x];
                matches(cell, self.pattern[(ry + y) * self.width + rx + x])
            })
        })
    }
}

/// What the grid would make: (item, count).
pub fn result(grid: &[Option<Stack>; GRID_SIZE]) -> Option<(u16, u16)> {
    RECIPES.iter().find(|r| r.fits(grid)).map(|r| (r.output, r.count))
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
        let mut full = ring.clone();
        full.push((4, c, 1));
        assert_eq!(result(&grid(&full)), None);
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
