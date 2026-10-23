//! Fences and fence gates, as minetest_game's (default.register_fence in
//! mods/default/functions.lua, doors.register_fencegate in
//! mods/doors/init.lua), in each of our five woods: apple (its "wood"),
//! fir (its pine), birch (its aspen), jungle and acacia.
//!
//! A fence is a post with rails reaching out to whatever it joins. The
//! client works out the joins itself, from the node box (Luanti's
//! "connected" type: boxes for each side, drawn only when that side
//! joins) and the list of nodes a fence joins (connects_to): other fences,
//! gates, planks and trunks, as minetest_game's group:fence, group:wood and
//! group:tree. The server only sends the definitions.
//!
//! A fence rail is a fence without its posts: two bars, joining fences
//! and other rails (but not planks or trunks).
//!
//! A gate is a model, turned by facedir like a door, joining fences on
//! its left and right (connect_sides), and opened and closed with a
//! right-click.

use crate::content::*;

pub struct Wood {
    pub fence: u16,
    /// The closed gate, which is also the item; the open one is next.
    pub gate: u16,
    pub rail: u16,
    /// Seconds a fence or gate of this wood burns (minetest_game's fuel
    /// recipes: as its planks), and a rail (two less).
    pub burn: f32,
    pub rail_burn: f32,
}

pub const FIRST_FENCE: u16 = CONTENT_OBSIDIAN + 1;
pub const FENCE_APPLE: u16 = FIRST_FENCE;
pub const FENCE_FIR: u16 = FIRST_FENCE + 1;
pub const FENCE_BIRCH: u16 = FIRST_FENCE + 2;
pub const GATE_APPLE: u16 = FIRST_FENCE + 3;
pub const GATE_FIR: u16 = FIRST_FENCE + 5;
pub const GATE_BIRCH: u16 = FIRST_FENCE + 7;
pub const RAIL_APPLE: u16 = FIRST_FENCE + 9;
pub const RAIL_FIR: u16 = FIRST_FENCE + 10;
pub const RAIL_BIRCH: u16 = FIRST_FENCE + 11;
/// Jungle and acacia came later: their fences, gates and rails follow the
/// rainforest's jungle grass.
pub const FENCE_JUNGLE: u16 = crate::farming::JUNGLEGRASS + 1;
pub const FENCE_ACACIA: u16 = FENCE_JUNGLE + 1;
pub const GATE_JUNGLE: u16 = FENCE_JUNGLE + 2;
pub const GATE_ACACIA: u16 = FENCE_JUNGLE + 4;
pub const RAIL_JUNGLE: u16 = FENCE_JUNGLE + 6;
pub const RAIL_ACACIA: u16 = FENCE_JUNGLE + 7;

pub const WOODS: [Wood; 5] = [
    Wood { fence: FENCE_APPLE, gate: GATE_APPLE, rail: RAIL_APPLE, burn: 7.0, rail_burn: 5.0 },
    Wood { fence: FENCE_FIR, gate: GATE_FIR, rail: RAIL_FIR, burn: 6.0, rail_burn: 4.0 },
    Wood { fence: FENCE_BIRCH, gate: GATE_BIRCH, rail: RAIL_BIRCH, burn: 5.0, rail_burn: 3.0 },
    Wood { fence: FENCE_JUNGLE, gate: GATE_JUNGLE, rail: RAIL_JUNGLE, burn: 9.0, rail_burn: 7.0 },
    Wood { fence: FENCE_ACACIA, gate: GATE_ACACIA, rail: RAIL_ACACIA, burn: 8.0, rail_burn: 6.0 },
];

pub fn is_rail(id: u16) -> bool {
    WOODS.iter().any(|w| w.rail == id)
}

/// How long a fence, rail or gate burns (0 for anything else, and an open
/// gate, which is never in an inventory).
pub fn burn_time(id: u16) -> f32 {
    WOODS
        .iter()
        .find_map(|w| match id {
            _ if id == w.fence || id == w.gate => Some(w.burn),
            _ if id == w.rail => Some(w.rail_burn),
            _ => None,
        })
        .unwrap_or(0.0)
}

/// The wood a gate node is made of, and whether it is the open one.
pub fn gate_of(id: u16) -> Option<(&'static Wood, bool)> {
    WOODS.iter().find(|w| w.gate == id || w.gate + 1 == id).map(|w| (w, id != w.gate))
}

pub fn is_gate(id: u16) -> bool {
    gate_of(id).is_some()
}

/// Opening or closing a gate (its on_rightclick, a swap_node keeping the
/// turn): the other node, and whether it opened.
pub fn gate_toggle(id: u16) -> Option<(u16, bool)> {
    let (w, open) = gate_of(id)?;
    Some(if open { (w.gate, false) } else { (w.gate + 1, true) })
}

/// The gate sounds, at minetest_game's gain, heard within 8 nodes.
pub const GATE_OPEN: (&str, f32) = ("doors_fencegate_open", 0.15);
pub const GATE_CLOSE: (&str, f32) = ("doors_fencegate_close", 0.15);
pub const GATE_HEARING: f32 = 8.0;

/// Its models.
pub fn gate_mesh(id: u16) -> &'static str {
    match gate_of(id) {
        Some((_, true)) => "doors_fencegate_open.obj",
        _ => "doors_fencegate_closed.obj",
    }
}

/// A "connected" node box: the boxes always there, and those for each
/// side, drawn when the node joins something on that side (front -Z,
/// left -X, back +Z, right +X). Fences use no others.
pub struct Connected {
    pub fixed: &'static [[f32; 6]],
    pub front: &'static [[f32; 6]],
    pub left: &'static [[f32; 6]],
    pub back: &'static [[f32; 6]],
    pub right: &'static [[f32; 6]],
}

const S: f32 = 1.0 / 16.0;

/// A fence: a post, with two rails out to each side it joins.
pub const FENCE_BOX: Connected = Connected {
    fixed: &[[-2.0 * S, -0.5, -2.0 * S, 2.0 * S, 0.5, 2.0 * S]],
    front: &[[-S, 3.0 * S, -0.5, S, 5.0 * S, -2.0 * S], [-S, -5.0 * S, -0.5, S, -3.0 * S, -2.0 * S]],
    left: &[[-0.5, 3.0 * S, -S, -2.0 * S, 5.0 * S, S], [-0.5, -5.0 * S, -S, -2.0 * S, -3.0 * S, S]],
    back: &[[-S, 3.0 * S, 2.0 * S, S, 5.0 * S, 0.5], [-S, -5.0 * S, 2.0 * S, S, -3.0 * S, 0.5]],
    right: &[[2.0 * S, 3.0 * S, -S, 0.5, 5.0 * S, S], [2.0 * S, -5.0 * S, -S, 0.5, -3.0 * S, S]],
};

/// A rail: the two bars, short in the middle, out to each side it joins
/// (minetest_game's fixed boxes, with the lower one's y the right way up).
pub const RAIL_BOX: Connected = Connected {
    fixed: &[[-S, 3.0 * S, -S, S, 5.0 * S, S], [-S, -5.0 * S, -S, S, -3.0 * S, S]],
    front: &[[-S, 3.0 * S, -0.5, S, 5.0 * S, -S], [-S, -5.0 * S, -0.5, S, -3.0 * S, -S]],
    left: &[[-0.5, 3.0 * S, -S, -S, 5.0 * S, S], [-0.5, -5.0 * S, -S, -S, -3.0 * S, S]],
    back: &[[-S, 3.0 * S, S, S, 5.0 * S, 0.5], [-S, -5.0 * S, S, S, -3.0 * S, 0.5]],
    right: &[[S, 3.0 * S, -S, 0.5, 5.0 * S, S], [S, -5.0 * S, -S, 0.5, -3.0 * S, S]],
};

/// What a player bumps into (rails too: as tall as a fence): the post and, to each side it joins, a wall
/// as tall as the post (minetest_game without enable_fence_tall).
pub const FENCE_COLLISION: Connected = Connected {
    fixed: &[[-2.0 * S, -0.5, -2.0 * S, 2.0 * S, 0.5, 2.0 * S]],
    front: &[[-2.0 * S, -0.5, -0.5, 2.0 * S, 0.5, -2.0 * S]],
    left: &[[-0.5, -0.5, -2.0 * S, -2.0 * S, 0.5, 2.0 * S]],
    back: &[[-2.0 * S, -0.5, 2.0 * S, 2.0 * S, 0.5, 0.5]],
    right: &[[2.0 * S, -0.5, -2.0 * S, 0.5, 0.5, 2.0 * S]],
};

/// Which sides of a gate fences join (Luanti's connect_sides: left and
/// right, turned with the gate).
pub const GATE_SIDES: u8 = 8 | 32;

pub const GATE_SELECTION: [f32; 6] = [-0.5, -0.5, -4.0 * S, 0.5, 0.5, 4.0 * S];

/// A gate's collision boxes: closed, a slab across the node; open, the
/// post at the hinge and the gate swung back along the side.
pub fn gate_collision(open: bool) -> &'static [[f32; 6]] {
    if open {
        &[[-0.5, -0.5, -2.0 * S, -6.0 * S, 0.5, 2.0 * S], [-0.5, -6.0 * S, -0.5, -6.0 * S, 6.0 * S, 0.0]]
    } else {
        &[[-0.5, -0.5, -2.0 * S, 0.5, 0.5, 2.0 * S]]
    }
}

/// minetest_game's walls (mods/walls): a thick post, with lower, thinner
/// walls out to each side it joins. Cobblestone and desert cobblestone;
/// mossy cobblestone's comes with dungeons.
pub const WALL_COBBLE: u16 = crate::bones::BONES + 1;
pub const WALL_DESERT_COBBLE: u16 = WALL_COBBLE + 1;

pub const WALL_BOX: Connected = Connected {
    fixed: &[[-0.25, -0.5, -0.25, 0.25, 0.5, 0.25]],
    front: &[[-3.0 * S, -0.5, -0.5, 3.0 * S, 0.375, -0.25]],
    left: &[[-0.5, -0.5, -3.0 * S, -0.25, 0.375, 3.0 * S]],
    back: &[[-3.0 * S, -0.5, 0.25, 3.0 * S, 0.375, 0.5]],
    right: &[[0.25, -0.5, -3.0 * S, 0.5, 0.375, 3.0 * S]],
};

/// What a player bumps into: the post and, to each side, a wall as wide as
/// the post and as tall as a node.
pub const WALL_COLLISION: Connected = Connected {
    fixed: &[[-0.25, -0.5, -0.25, 0.25, 0.5, 0.25]],
    front: &[[-0.25, -0.5, -0.5, 0.25, 0.5, -0.25]],
    left: &[[-0.5, -0.5, -0.25, -0.25, 0.5, 0.25]],
    back: &[[-0.25, -0.5, 0.25, 0.25, 0.5, 0.5]],
    right: &[[0.25, -0.5, -0.25, 0.5, 0.5, 0.25]],
};

/// The groups whose nodes a wall joins.
pub const WALL_JOINS: [&str; 4] = ["wall", "stone", "fence", "wall_connected"];

/// The groups whose nodes a fence joins (its connects_to).
pub const JOINS: [&str; 5] = ["fence", "wood", "tree", "wall", "fence_connected"];
/// And a rail.
pub const RAIL_JOINS: [&str; 2] = ["fence", "wall"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates() {
        assert_eq!(gate_toggle(GATE_FIR), Some((GATE_FIR + 1, true)));
        assert_eq!(gate_toggle(GATE_FIR + 1), Some((GATE_FIR, false)));
        assert_eq!(gate_toggle(FENCE_FIR), None);
        assert!(is_gate(GATE_BIRCH + 1) && !is_gate(GATE_BIRCH + 2));
        assert_eq!(gate_mesh(GATE_APPLE + 1), "doors_fencegate_open.obj");
        assert!(is_rail(RAIL_FIR) && !is_rail(FENCE_FIR));
        assert_eq!((burn_time(RAIL_APPLE), burn_time(FENCE_FIR), burn_time(GATE_BIRCH), burn_time(GATE_BIRCH + 1)), (5.0, 6.0, 5.0, 0.0));
    }
}
