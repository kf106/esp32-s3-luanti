//! Falling sand and gravel, as builtin's falling nodes
//! (builtin/game/falling.lua): a node in group "falling_node" with nothing
//! to rest on (air, water, plants: what isn't walkable, or is built over)
//! comes loose and falls, under gravity, until something stops it. There
//! it settles, replacing what it can build over; anything else (a torch,
//! say) makes it drop as an item instead. What was stacked on top of it
//! comes loose in turn.
//!
//! Builtin turns the node into an entity that clients animate. Here the
//! server moves the node itself, a node at a time, with node updates to
//! the players, and the world only holds it again once it has landed.

use crate::content;
use crate::world::NodePos;

/// Luanti's default gravity (movement_gravity), in nodes per second².
const GRAVITY: f32 = 9.81;
/// Below this a falling node is lost (as builtin's entity is, falling out
/// of the world).
const LOWEST: i32 = -31000;

/// Whether a node comes loose with nothing under it.
pub fn falls(id: u16) -> bool {
    content::group_of(id, "falling_node") > 0
}

/// Whether a falling node goes on down into this one (builtin's
/// check_single_for_falling: not walkable, or built over).
pub fn falls_into(id: u16) -> bool {
    !content::walkable(id) || content::buildable_to(id)
}

/// A node on its way down.
#[derive(Clone, Debug, PartialEq)]
pub struct Falling {
    pub id: u16,
    pub param2: u8,
    /// The node it is shown in.
    pub pos: NodePos,
    /// Its height, in nodes (the bottom of the node), and its speed.
    y: f32,
    speed: f32,
}

impl Falling {
    pub fn new(id: u16, param2: u8, pos: NodePos) -> Self {
        Self { id, param2, pos, y: pos.1 as f32, speed: 0.0 }
    }
}

/// What a step did with a falling node.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// Still on the way: shown a node or more lower, from `from`.
    Moved { from: NodePos },
    /// Still on the way, in the same node.
    Same,
    /// Came to rest at its position: set it there.
    Landed,
    /// Came to rest on something it can't replace: drop it as an item.
    Dropped,
    /// Fell out of the world.
    Lost,
}

/// Move a falling node on by `dt` seconds. `node` gives what the world
/// holds at a position.
pub fn step(f: &mut Falling, dt: f32, node: &dyn Fn(NodePos) -> u16) -> Step {
    f.speed += GRAVITY * dt;
    f.y -= f.speed * dt;
    let from = f.pos;
    // Down a node at a time, stopping on the first one it can't fall into.
    while f.pos.1 as f32 > f.y.floor() {
        let below = (f.pos.0, f.pos.1 - 1, f.pos.2);
        if below.1 < LOWEST {
            return Step::Lost;
        }
        if !falls_into(node(below)) {
            f.y = f.pos.1 as f32;
            // It settles where it can build over what is there (air,
            // water, a plant), otherwise it drops as an item.
            return if content::buildable_to(node(f.pos)) { Step::Landed } else { Step::Dropped };
        }
        f.pos = below;
    }
    if f.pos == from { Step::Same } else { Step::Moved { from } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::*;
    use crate::world::CONTENT_AIR;

    #[test]
    fn sand_falls_and_lands() {
        // Stone at 0, a torch at 3 on the second column, air elsewhere.
        let node = |p: NodePos| match p {
            (_, y, _) if y <= 0 => CONTENT_STONE,
            (1, 3, 0) => CONTENT_TORCH,
            (2, 2, 0) => CONTENT_WATER,
            _ => CONTENT_AIR,
        };
        assert!(falls(CONTENT_SAND) && !falls(CONTENT_DIRT));
        assert!(falls_into(CONTENT_AIR) && falls_into(CONTENT_WATER) && falls_into(CONTENT_TORCH) && !falls_into(CONTENT_STONE));
        let mut f = Falling::new(CONTENT_SAND, 0, (0, 10, 0));
        let mut steps = 0;
        let end = loop {
            steps += 1;
            match step(&mut f, 0.05, &node) {
                Step::Moved { .. } | Step::Same => continue,
                other => break other,
            }
        };
        assert_eq!((end, f.pos), (Step::Landed, (0, 1, 0)));
        // 9 nodes under gravity take about 1.35 s.
        assert!((24..=32).contains(&steps), "{steps} steps");
        // Through water too; onto a torch, it drops as an item.
        let mut f = Falling::new(CONTENT_SAND, 0, (2, 5, 0));
        while matches!(step(&mut f, 0.05, &node), Step::Moved { .. } | Step::Same) {}
        assert_eq!(f.pos, (2, 1, 0));
        let torch_over_stone = |p: NodePos| if p == (1, 1, 0) { CONTENT_TORCH } else { node(p) };
        let mut f = Falling::new(CONTENT_SAND, 0, (1, 2, 0));
        let end = loop {
            match step(&mut f, 0.05, &torch_over_stone) {
                Step::Moved { .. } | Step::Same => continue,
                other => break other,
            }
        };
        assert_eq!((end, f.pos), (Step::Dropped, (1, 1, 0)));
    }
}
