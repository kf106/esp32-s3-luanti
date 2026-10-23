//! Glass panes, obsidian glass panes and steel bars, as minetest_game's
//! xpanes mod (mods/xpanes/init.lua), with its steel bar door and
//! trapdoor.
//!
//! Each pane is two nodes. The flat one is a thin sheet across the node,
//! turned by facedir, and is also the item. The other is a "connected"
//! node box: a thin post with an arm out to each side that joins
//! something, the joining done by the client, like a fence's. The server
//! picks between them (xpanes' update_pane) whenever a node is placed or
//! dug beside a pane: on its own or in a straight line, a pane is the
//! flat sheet, turned to run along the line; at a corner, a T or a cross,
//! it is the connected one.
//!
//! A pane joins other panes, and stone, wood and trees (its connects_to;
//! minetest_game's glass is in none of those groups).

use crate::content::*;

pub const PANE_FLAT: u16 = crate::colours::LAST_WOOL + 1;
pub const PANE: u16 = PANE_FLAT + 1;
pub const OBSIDIAN_PANE_FLAT: u16 = PANE_FLAT + 2;
pub const OBSIDIAN_PANE: u16 = PANE_FLAT + 3;
pub const BAR_FLAT: u16 = PANE_FLAT + 4;
pub const BAR: u16 = PANE_FLAT + 5;
/// The steel bar door's four nodes (doors.rs), then its trapdoor, closed
/// and open.
pub const BAR_DOOR: u16 = PANE_FLAT + 6;
pub const BAR_TRAPDOOR: u16 = BAR_DOOR + 4;

/// Each pane's flat and connected nodes.
const PANES: [(u16, u16); 3] = [(PANE_FLAT, PANE), (OBSIDIAN_PANE_FLAT, OBSIDIAN_PANE), (BAR_FLAT, BAR)];

/// The groups a pane joins, besides other panes (its connects_to).
pub const JOINS: [&str; 5] = ["pane", "stone", "glass", "wood", "tree"];

pub fn is_pane(id: u16) -> bool {
    PANES.iter().any(|&(f, c)| id == f || id == c)
}

pub fn is_flat(id: u16) -> bool {
    PANES.iter().any(|&(f, _)| id == f)
}

/// The flat sheet: 1/16 of a node thick, across the node along X.
pub const FLAT_BOX: [f32; 6] = [-0.5, -0.5, -1.0 / 32.0, 0.5, 0.5, 1.0 / 32.0];

const T: f32 = 1.0 / 32.0;
/// The connected pane: a thin post, and an arm to each side it joins.
pub const CONNECTED_BOX: crate::fences::Connected = crate::fences::Connected {
    fixed: &[[-T, -0.5, -T, T, 0.5, T]],
    front: &[[-T, -0.5, -0.5, T, 0.5, -T]],
    left: &[[-0.5, -0.5, -T, -T, 0.5, T]],
    back: &[[-T, -0.5, T, T, 0.5, 0.5]],
    right: &[[T, -0.5, -T, 0.5, 0.5, T]],
};

/// Whether a pane joins this node.
fn joins(id: u16) -> bool {
    is_pane(id) || JOINS.iter().any(|g| group_of(id, g) > 0)
}

/// The four sides of a node in facedir order (minetest.facedir_to_dir):
/// +Z, +X, -Z, -X.
pub const SIDES: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];

/// xpanes' update_pane: what a pane node should be, given what is on its
/// four sides (in SIDES order). None if it stays as it is.
pub fn update(id: u16, param2: u8, sides: [u16; 4]) -> Option<(u16, u8)> {
    let &(flat, connected) = PANES.iter().find(|&&(f, c)| id == f || id == c)?;
    let c: [bool; 4] = sides.map(joins);
    let count = c.iter().filter(|&&b| b).count();
    let any = (0..4).rev().find(|&d| c[d]).map_or(param2, |d| d as u8);
    let want = match count {
        0 => (flat, any),
        1 => (flat, (any + 1) % 4),
        2 if (c[0] && c[2]) || (c[1] && c[3]) => (flat, (any + 1) % 4),
        _ => (connected, 0),
    };
    (want != (id, param2)).then_some(want)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::CONTENT_AIR;

    #[test]
    fn pane_shapes() {
        let a = CONTENT_AIR;
        let p = PANE_FLAT;
        // Alone it stays as placed.
        assert_eq!(update(p, 1, [a; 4]), None);
        // One neighbour, on +X: a sheet running east-west.
        assert_eq!(update(p, 1, [a, p, a, a]), Some((p, 2)));
        // Neighbours north and south: a sheet running north-south.
        assert_eq!(update(p, 0, [p, a, p, a]), Some((p, 3)));
        // A corner, or more: the connected node.
        assert_eq!(update(p, 0, [p, p, a, a]), Some((PANE, 0)));
        assert_eq!(update(PANE, 0, [p, p, p, p]), None);
        // Back to a line: flat again. Stone counts; glass doesn't.
        assert_eq!(update(PANE, 0, [CONTENT_STONE, a, p, a]), Some((p, 3)));
        assert_eq!(update(p, 0, [CONTENT_GLASS, a, a, a]), None);
        assert_eq!(update(BAR, 0, [a, BAR_FLAT, a, a]), Some((BAR_FLAT, 2)));
        assert_eq!(update(CONTENT_STONE, 0, [p; 4]), None, "not a pane");
        assert!(is_flat(OBSIDIAN_PANE_FLAT) && !is_flat(OBSIDIAN_PANE) && is_pane(OBSIDIAN_PANE));
    }
}
