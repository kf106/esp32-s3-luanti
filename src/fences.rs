//! Fences and fence gates, as minetest_game's (default.register_fence in
//! mods/default/functions.lua, doors.register_fencegate in
//! mods/doors/init.lua), in each of our three woods: apple (its "wood"),
//! fir (its pine) and birch (its aspen).
//!
//! A fence is a post with rails reaching out to whatever it joins. The
//! client works out the joins itself, from the node box (Luanti's
//! "connected" type: boxes for each side, drawn only when that side
//! joins) and the list of nodes a fence joins (connects_to): other fences,
//! gates, planks and trunks, as minetest_game's group:fence, group:wood and
//! group:tree. The server only sends the definitions.
//!
//! A gate is a model, turned by facedir like a door, joining fences on
//! its left and right (connect_sides), and opened and closed with a
//! right-click.

use crate::content::*;

pub struct Wood {
    pub fence: u16,
    /// The closed gate, which is also the item; the open one is next.
    pub gate: u16,
    /// Seconds a fence or gate of this wood burns (minetest_game's fuel
    /// recipes: as its planks).
    pub burn: f32,
}

pub const FIRST_FENCE: u16 = CONTENT_OBSIDIAN + 1;
pub const FENCE_APPLE: u16 = FIRST_FENCE;
pub const FENCE_FIR: u16 = FIRST_FENCE + 1;
pub const FENCE_BIRCH: u16 = FIRST_FENCE + 2;
pub const GATE_APPLE: u16 = FIRST_FENCE + 3;
pub const GATE_FIR: u16 = FIRST_FENCE + 5;
pub const GATE_BIRCH: u16 = FIRST_FENCE + 7;

pub const WOODS: [Wood; 3] = [
    Wood { fence: FENCE_APPLE, gate: GATE_APPLE, burn: 7.0 },
    Wood { fence: FENCE_FIR, gate: GATE_FIR, burn: 6.0 },
    Wood { fence: FENCE_BIRCH, gate: GATE_BIRCH, burn: 5.0 },
];

pub fn is_fence(id: u16) -> bool {
    WOODS.iter().any(|w| w.fence == id)
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

/// What a player bumps into: the post and, to each side it joins, a wall
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

/// The groups whose nodes a fence joins (its connects_to).
pub const JOINS: [&str; 5] = ["fence", "wood", "tree", "wall", "fence_connected"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates() {
        assert_eq!(gate_toggle(GATE_FIR), Some((GATE_FIR + 1, true)));
        assert_eq!(gate_toggle(GATE_FIR + 1), Some((GATE_FIR, false)));
        assert_eq!(gate_toggle(FENCE_FIR), None);
        assert!(is_gate(GATE_BIRCH + 1) && !is_gate(GATE_BIRCH + 2) && is_fence(FENCE_BIRCH) && !is_fence(GATE_APPLE));
        assert_eq!(gate_mesh(GATE_APPLE + 1), "doors_fencegate_open.obj");
    }
}
