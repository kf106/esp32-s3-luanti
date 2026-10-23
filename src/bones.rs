//! Bones, as minetest_game's bones mod (mods/bones/init.lua): when a
//! player dies with anything in their inventory, a bones block is left
//! where they died, holding it all.
//!
//! - The bones go where the player's feet were, if that is air, liquid or
//!   something built over (a plant, a snow layer); otherwise in the
//!   nearest air beside it. Then they drop through any air below until
//!   they rest on something: a player killed by a fall was last heard of
//!   a little above where they landed (the client sends its position ten
//!   times a second, and reports fall damage on landing). With nowhere
//!   to put them, everything is dropped on the ground instead, with a
//!   bones item.
//! - They hold 32 stacks (the main inventory, then the craft grid); what
//!   doesn't fit is dropped.
//! - Only the dead player can take from them, at first. After 20 minutes
//!   of game time (share_bones_time) they are "old bones", and anyone may.
//! - Right-click opens them; punching them takes everything that fits.
//!   Once empty they vanish, and the bones block goes into the inventory.
//! - Nothing can be put into bones.
//!
//! The server keeps them as chests (chest.rs) flagged as bones.

use crate::content::*;
use crate::inventory::{self, Stack};
use crate::world::{NodePos, CONTENT_AIR};

pub const BONES: u16 = crate::panes::BAR_TRAPDOOR + 2;

/// minetest_game's share_bones_time: seconds of game time until anyone may
/// take from someone's bones.
pub const SHARE_TIME: u64 = 1200;

/// How far bones drop to find something to rest on.
const SETTLE_MAX: i32 = 64;

/// How often bones are checked for having become old (their node timer).
pub const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

/// minetest_game's may_replace (without protection): bones may go into
/// air, a liquid, or anything built over.
pub fn may_replace(id: u16) -> bool {
    id == CONTENT_AIR || is_liquid(id) || buildable_to(id)
}

/// Where bones go for a player who died at `feet` (rounded): there, if
/// `may_replace` allows, or else the nearest air within a node of it
/// (minetest.find_node_near). None: nowhere, so drop everything.
pub fn spot(feet: NodePos, node: &dyn Fn(NodePos) -> u16) -> Option<NodePos> {
    let mut pos = unsettled_spot(feet, node)?;
    for _ in 0..SETTLE_MAX {
        let below = (pos.0, pos.1 - 1, pos.2);
        if node(below) != CONTENT_AIR {
            break;
        }
        pos = below;
    }
    Some(pos)
}

fn unsettled_spot(feet: NodePos, node: &dyn Fn(NodePos) -> u16) -> Option<NodePos> {
    if may_replace(node(feet)) {
        return Some(feet);
    }
    let mut near: Vec<NodePos> = (-1..=1)
        .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (dx, dy, dz))))
        .filter(|&d| d != (0, 0, 0))
        .map(|(dx, dy, dz)| (feet.0 + dx, feet.1 + dy, feet.2 + dz))
        .collect();
    near.sort_by_key(|p| (p.0 - feet.0).pow(2) + (p.1 - feet.1).pow(2) + (p.2 - feet.2).pow(2));
    near.into_iter().find(|&p| node(p) == CONTENT_AIR)
}

/// Pack a dead player's stacks into the bones' 32 slots (inv:add_item, in
/// order). Returns the slots and what didn't fit.
pub fn pack(stacks: impl IntoIterator<Item = Stack>) -> ([Option<Stack>; crate::chest::SLOTS], Vec<Stack>) {
    let mut slots = [None; crate::chest::SLOTS];
    let mut left = Vec::new();
    for stack in stacks {
        let over = inventory::add_to(&mut slots, stack);
        if over > 0 {
            left.push(Stack { count: over, ..stack });
        }
    }
    (slots, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_bones_go() {
        // Open ground: where the feet were.
        let open = |p: NodePos| if p.1 < 0 { CONTENT_STONE } else { CONTENT_AIR };
        assert_eq!(spot((0, 0, 0), &open), Some((0, 0, 0)));
        // In a one-node hole with air above: the air above.
        let hole = |p: NodePos| if p == (0, 1, 0) { CONTENT_AIR } else { CONTENT_STONE };
        assert_eq!(spot((0, 0, 0), &hole), Some((0, 1, 0)));
        // In water, or a snow layer: there.
        assert_eq!(spot((0, 0, 0), &|_| CONTENT_WATER), Some((0, 0, 0)));
        // Shut in stone: nowhere.
        assert_eq!(spot((0, 0, 0), &|_| CONTENT_STONE), None);
        // Last heard of in mid-air (a fall): down onto the ground.
        assert_eq!(spot((5, 7, 5), &open), Some((5, 0, 5)));
        // Into water, they stop at its surface (only air is fallen through).
        let lake = |p: NodePos| if p.1 < 0 { CONTENT_STONE } else if p.1 < 3 { CONTENT_WATER } else { CONTENT_AIR };
        assert_eq!(spot((0, 6, 0), &lake), Some((0, 3, 0)));
    }

    #[test]
    fn packing() {
        let many = (0..40).map(|_| Stack::new(CONTENT_COBBLE, 99));
        let (slots, left) = pack(many);
        assert!(slots.iter().all(|s| s.is_some()));
        assert_eq!(left.len(), 8);
        let (slots, left) = pack([Stack::new(CONTENT_DIRT, 5), Stack::new(CONTENT_DIRT, 7)]);
        assert_eq!((slots[0], slots[1], left.len()), (Some(Stack::new(CONTENT_DIRT, 12)), None, 0));
    }
}
