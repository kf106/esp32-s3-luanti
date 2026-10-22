//! Flowing liquids (water and lava), as the Luanti engine does them
//! (ServerMap::transformLiquidsLocal in src/servermap.cpp).
//!
//! Places where liquid may have to change wait in a queue: the
//! neighbours of every node a player (or the game) changes, and the
//! neighbours of every node the liquid itself changes. Once a second the
//! server works through what is queued at that moment, so liquid spreads
//! a node a second. Each place looks at its six neighbours and becomes:
//!   - a source, if it is one, or if it has two source neighbours (not
//!     below) and the liquid is renewable, like water;
//!   - flowing at the full level 7, beside a source (not below);
//!   - otherwise flowing at the highest level its flowing neighbours give
//!     it: one less than a neighbour at the same height, or (from above)
//!     4 more, up to 7; a liquid falling down a side doesn't spread
//!     sideways; below the liquid's range it is gone again.
//! A flowing node's param2 holds its level (0 to 7) and, in bit 3,
//! whether it flows down into the node below.

use std::collections::VecDeque;

use crate::content::{self, CONTENT_LAVA, CONTENT_LAVA_FLOWING, CONTENT_WATER, CONTENT_WATER_FLOWING};
use crate::fasthash::FastSet;
use crate::world::{NodePos, CONTENT_AIR};

const LEVEL_MASK: u8 = 0x07;
const FLOW_DOWN: u8 = 0x08;
const LEVEL_MAX: i8 = 7;
const LEVEL_SOURCE: i8 = 8;
/// How much higher liquid falling from above lands (WATER_DROP_BOOST).
const DROP_BOOST: i8 = 4;

/// A liquid: its source and flowing nodes, and how it flows.
pub struct Liquid {
    pub source: u16,
    pub flowing: u16,
    /// How slowly it reaches its level: 1 at once, 7 (lava) a step at a time.
    viscosity: i8,
    /// Whether two sources make a third between them.
    renewable: bool,
    /// How many nodes it flows sideways (levels 8 - range to 7 exist).
    range: i8,
}

/// minetest_game's water: viscosity 1, renewable, range 8.
pub const WATER: Liquid = Liquid { source: CONTENT_WATER, flowing: CONTENT_WATER_FLOWING, viscosity: 1, renewable: true, range: 8 };

/// minetest_game's lava: viscosity 7 (it creeps, a level at a time), not
/// renewable, the default range of 8.
pub const LAVA: Liquid = Liquid { source: CONTENT_LAVA, flowing: CONTENT_LAVA_FLOWING, viscosity: 7, renewable: false, range: 8 };

pub const LIQUIDS: [&Liquid; 2] = [&WATER, &LAVA];

#[derive(Clone, Copy, PartialEq)]
enum Type {
    None,
    Source,
    Flowing,
}

fn liquid_of(id: u16) -> Option<(&'static Liquid, Type)> {
    LIQUIDS.iter().find_map(|l| {
        if l.source == id {
            Some((*l, Type::Source))
        } else if l.flowing == id {
            Some((*l, Type::Flowing))
        } else {
            None
        }
    })
}

fn type_of(id: u16) -> Type {
    liquid_of(id).map_or(Type::None, |(_, t)| t)
}

/// Whether liquid flows into a node, replacing it: air, and floodable
/// nodes (torches).
fn floodable(id: u16) -> bool {
    id == CONTENT_AIR || content::floodable(id)
}

/// The places waiting to be looked at, each queued once (UniqueQueue).
#[derive(Default)]
pub struct Queue {
    order: VecDeque<NodePos>,
    queued: FastSet<NodePos>,
}

/// The most places kept waiting. Liquid pouring into a large cave could
/// queue more than the board can look at; then, as Luanti's
/// liquid_queue_purge_time does, the oldest are dropped (that liquid stops
/// where it is until something near it changes).
const QUEUE_MAX: usize = 20_000;

impl Queue {
    pub fn push(&mut self, pos: NodePos) {
        if self.queued.insert(pos) {
            self.order.push_back(pos);
            if self.order.len() > QUEUE_MAX {
                if let Some(old) = self.order.pop_front() {
                    self.queued.remove(&old);
                }
            }
        }
    }

    fn pop(&mut self) -> Option<NodePos> {
        let pos = self.order.pop_front()?;
        self.queued.remove(&pos);
        Some(pos)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }
}

/// The map as the liquids see it.
pub trait Nodes {
    fn get(&self, pos: NodePos) -> (u16, u8);
    fn set(&mut self, pos: NodePos, id: u16, param2: u8);
}

/// A node the liquid changed: what it was, and what it is now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub pos: NodePos,
    pub was: u16,
    pub id: u16,
    pub param2: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Upper,
    Same,
    Lower,
}

#[derive(Clone, Copy)]
struct Neighbour {
    param2: u8,
    side: Side,
    pos: NodePos,
}

/// Upper first, then the same level, then lower (liquid_6dirs).
const DIRS: [(i32, i32, i32); 6] = [(0, 1, 0), (0, 0, 1), (1, 0, 0), (0, 0, -1), (-1, 0, 0), (0, -1, 0)];

/// The highest level a flowing neighbour lets this node reach
/// (get_max_liquid_level).
fn max_level(nb: &Neighbour, current: i8) -> i8 {
    let level = (nb.param2 & LEVEL_MASK) as i8;
    match nb.side {
        Side::Upper => {
            if level + DROP_BOOST > current {
                (level + DROP_BOOST).min(LEVEL_MAX)
            } else if level > current {
                level
            } else {
                current
            }
        }
        Side::Lower => current,
        Side::Same => {
            if nb.param2 & FLOW_DOWN != FLOW_DOWN && level > 0 && level - 1 > current {
                level - 1
            } else {
                current
            }
        }
    }
}

/// Look at up to `max` queued places (transformLiquidsLocal), changing the
/// map as it goes. Returns what changed.
pub fn transform(map: &mut impl Nodes, queue: &mut Queue, max: usize) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut must_reflow = Vec::new();
    for _ in 0..max {
        let Some(p0) = queue.pop() else {
            break;
        };
        let (n0, p2_0) = map.get(p0);
        let t0 = type_of(n0);
        // The liquid that would be here, and what is here if it isn't.
        let mut kind: Option<&'static Liquid> = None;
        let mut floodable_node = CONTENT_AIR;
        let level0: i8 = match liquid_of(n0) {
            Some((l, Type::Source)) => {
                kind = Some(l);
                LEVEL_SOURCE
            }
            Some((l, _)) => {
                kind = Some(l);
                (p2_0 & LEVEL_MASK) as i8
            }
            None => {
                if !floodable(n0) {
                    continue;
                }
                floodable_node = n0;
                -1
            }
        };

        let mut sources = Vec::with_capacity(6);
        let mut flows: Vec<Neighbour> = Vec::with_capacity(6);
        let mut airs: Vec<Neighbour> = Vec::with_capacity(6);
        let mut flowing_down = false;
        for (i, d) in DIRS.iter().enumerate() {
            let side = match i {
                0 => Side::Upper,
                5 => Side::Lower,
                _ => Side::Same,
            };
            let pos = (p0.0 + d.0, p0.1 + d.1, p0.2 + d.2);
            let (id, param2) = map.get(pos);
            let nb = Neighbour { param2, side, pos };
            match liquid_of(id) {
                None => {
                    if floodable(id) {
                        airs.push(nb);
                        // Beside liquid, a place it may flow to is looked at
                        // whether or not this one changes.
                        if side != Side::Upper && t0 != Type::None {
                            queue.push(pos);
                        }
                        if side == Side::Lower {
                            flowing_down = true;
                        }
                    }
                }
                Some((l, Type::Source)) => {
                    if kind.is_none() {
                        kind = Some(l);
                    }
                    // Another liquid is just a wall; a source below
                    // doesn't count.
                    if kind.is_some_and(|k| std::ptr::eq(k, l)) && side != Side::Lower {
                        sources.push(nb);
                    }
                }
                Some((l, _)) => {
                    if side != Side::Same || param2 & FLOW_DOWN != FLOW_DOWN {
                        // Air takes the first liquid that can reach it.
                        let reach = max_level(&nb, -1);
                        if kind.is_none() && reach >= LEVEL_MAX + 1 - l.range {
                            kind = Some(l);
                        }
                    }
                    if kind.is_some_and(|k| std::ptr::eq(k, l)) {
                        flows.push(nb);
                        if side == Side::Lower {
                            flowing_down = true;
                        }
                    }
                }
            }
        }

        let Some(liquid) = kind else {
            continue; // air with no liquid near
        };
        let range = liquid.range.min(LEVEL_MAX + 1);
        let (new_id, new_level) = if (sources.len() >= 2 && liquid.renewable) || t0 == Type::Source {
            (liquid.source, -1)
        } else if !sources.is_empty() {
            let level = LEVEL_MAX;
            (if level >= LEVEL_MAX + 1 - range { liquid.flowing } else { floodable_node }, level)
        } else {
            let mut max = -1;
            for f in &flows {
                max = max_level(f, max);
            }
            let level = if liquid.viscosity > 1 && max != level0 {
                let inc = max - level0;
                let level = if inc < -liquid.viscosity || inc > liquid.viscosity {
                    level0 + inc / liquid.viscosity
                } else if inc < 0 {
                    level0 - 1
                } else {
                    level0 + 1
                };
                if level != max {
                    must_reflow.push(p0);
                }
                level
            } else {
                max
            };
            (if max >= LEVEL_MAX + 1 - range { liquid.flowing } else { floodable_node }, level)
        };

        // Nothing changed: on to the next.
        let new_type = type_of(new_id);
        if new_id == n0
            && (t0 != Type::Flowing
                || ((p2_0 & LEVEL_MASK) as i8 == new_level && (p2_0 & FLOW_DOWN == FLOW_DOWN) == flowing_down))
        {
            continue;
        }
        let param2 = if new_type == Type::Flowing {
            (if flowing_down { FLOW_DOWN } else { 0 }) | (new_level as u8 & LEVEL_MASK)
        } else {
            0
        };
        map.set(p0, new_id, param2);
        changes.push(Change { pos: p0, was: n0, id: new_id, param2 });

        // What may change in turn.
        match new_type {
            Type::Source | Type::Flowing => {
                for nb in flows.iter().chain(&airs) {
                    if nb.side != Side::Upper {
                        queue.push(nb.pos);
                    }
                }
            }
            Type::None => {
                for nb in &flows {
                    queue.push(nb.pos);
                }
            }
        }
    }
    for p in must_reflow {
        queue.push(p);
    }
    changes
}

/// The places to look at after a node changed: it and its neighbours that
/// are liquid or air (ServerMap::addNodeAndUpdate).
pub fn after_change(pos: NodePos, get: impl Fn(NodePos) -> u16, queue: &mut Queue) {
    // The node itself last, as Luanti does.
    for d in [(0, 0, 1), (0, 1, 0), (1, 0, 0), (0, 0, -1), (0, -1, 0), (-1, 0, 0), (0, 0, 0)] {
        let p = (pos.0 + d.0, pos.1 + d.1, pos.2 + d.2);
        let id = get(p);
        if id == CONTENT_AIR || type_of(id) != Type::None {
            queue.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A small map: stone below y = 0, air above, plus `nodes`.
    struct Map(HashMap<NodePos, (u16, u8)>);

    impl Nodes for Map {
        fn get(&self, p: NodePos) -> (u16, u8) {
            self.0.get(&p).copied().unwrap_or(if p.1 < 0 { (content::CONTENT_STONE, 0) } else { (CONTENT_AIR, 0) })
        }
        fn set(&mut self, p: NodePos, id: u16, param2: u8) {
            self.0.insert(p, (id, param2));
        }
    }

    fn settle(map: &mut Map, queue: &mut Queue) -> usize {
        let mut steps = 0;
        while queue.len() > 0 && steps < 200 {
            let n = queue.len();
            transform(map, queue, n);
            steps += 1;
        }
        steps
    }

    #[test]
    fn water_spreads_and_drains() {
        let mut map = Map(HashMap::new());
        let mut q = Queue::default();
        map.set((0, 0, 0), CONTENT_WATER, 0);
        after_change((0, 0, 0), |p| map.get(p).0, &mut q);
        settle(&mut map, &mut q);
        // On flat ground a source spreads 8 nodes each way (its range), a
        // level less each node, from 7 down to 0.
        for x in 1..=8 {
            assert_eq!(map.get((x, 0, 0)), (CONTENT_WATER_FLOWING, 8 - x as u8), "x = {x}");
        }
        assert_eq!(map.get((9, 0, 0)).0, CONTENT_AIR);
        assert_eq!(map.get((3, 0, 3)), (CONTENT_WATER_FLOWING, 2));
        // Into a hole: it falls, and the flow above the hole goes down.
        map.set((2, -1, 0), CONTENT_AIR, 0);
        after_change((2, -1, 0), |p| map.get(p).0, &mut q);
        settle(&mut map, &mut q);
        assert_eq!(map.get((2, -1, 0)).0, CONTENT_WATER_FLOWING);
        assert_eq!(map.get((2, 0, 0)).1 & FLOW_DOWN, FLOW_DOWN);
        // Take the source away: everything drains.
        map.set((0, 0, 0), CONTENT_AIR, 0);
        after_change((0, 0, 0), |p| map.get(p).0, &mut q);
        settle(&mut map, &mut q);
        assert!(map.0.values().all(|&(id, _)| id == CONTENT_AIR || id == content::CONTENT_STONE), "water left behind");
    }

    #[test]
    fn lava_creeps_and_stays() {
        // Lava spreads like water, but its levels change one step at a
        // time, and two lava sources make no third.
        let mut map = Map(HashMap::new());
        let mut q = Queue::default();
        map.set((0, 0, 0), CONTENT_LAVA, 0);
        map.set((2, 0, 0), CONTENT_LAVA, 0);
        after_change((0, 0, 0), |p| map.get(p).0, &mut q);
        after_change((2, 0, 0), |p| map.get(p).0, &mut q);
        let n = q.len();
        transform(&mut map, &mut q, n);
        let n = q.len();
        transform(&mut map, &mut q, n);
        // Beside the sources: flowing at 7 straight away (a source's
        // neighbour), but not a source.
        assert_eq!(map.get((1, 0, 0)), (CONTENT_LAVA_FLOWING, 7));
        settle(&mut map, &mut q);
        assert_eq!(map.get((1, 0, 0)).0, CONTENT_LAVA_FLOWING);
        assert_eq!(map.get((5, 0, 0)), (CONTENT_LAVA_FLOWING, 5));
        // Water and lava don't mix: each is a wall to the other.
        let mut map = Map(HashMap::new());
        let mut q = Queue::default();
        map.set((0, 0, 0), CONTENT_LAVA, 0);
        map.set((2, 0, 0), CONTENT_WATER, 0);
        after_change((0, 0, 0), |p| map.get(p).0, &mut q);
        after_change((2, 0, 0), |p| map.get(p).0, &mut q);
        settle(&mut map, &mut q);
        let between = map.get((1, 0, 0)).0;
        assert!(between == CONTENT_LAVA_FLOWING || between == CONTENT_WATER_FLOWING);
        assert_eq!(map.get((0, 0, 0)).0, CONTENT_LAVA);
        assert_eq!(map.get((2, 0, 0)).0, CONTENT_WATER);
    }

    #[test]
    fn sources_renew_and_pits_fill() {
        // A lake two wide with a gap: the gap becomes a source.
        let mut map = Map(HashMap::new());
        let mut q = Queue::default();
        for x in [0, 2] {
            map.set((x, 0, 0), CONTENT_WATER, 0);
        }
        // Walls round it so it stays put.
        for x in -1..=3 {
            for z in [-1, 1] {
                map.set((x, 0, z), content::CONTENT_STONE, 0);
            }
        }
        map.set((-1, 0, 0), content::CONTENT_STONE, 0);
        map.set((3, 0, 0), content::CONTENT_STONE, 0);
        after_change((1, 0, 0), |p| map.get(p).0, &mut q);
        settle(&mut map, &mut q);
        assert_eq!(map.get((1, 0, 0)), (CONTENT_WATER, 0));
        // Dig the lake's bed: the hole fills with full flowing water, and
        // the lake stays.
        map.set((1, -1, 0), CONTENT_AIR, 0);
        after_change((1, -1, 0), |p| map.get(p).0, &mut q);
        settle(&mut map, &mut q);
        assert_eq!(map.get((1, -1, 0)), (CONTENT_WATER_FLOWING, 7));
        assert_eq!(map.get((1, 0, 0)).0, CONTENT_WATER);
        // Water washes a torch away.
        let mut map = Map(HashMap::new());
        map.set((0, 0, 0), CONTENT_WATER, 0);
        map.set((1, 0, 0), content::CONTENT_TORCH, 1);
        let mut q = Queue::default();
        after_change((0, 0, 0), |p| map.get(p).0, &mut q);
        let mut flooded = Vec::new();
        while q.len() > 0 {
            let n = q.len();
            flooded.extend(transform(&mut map, &mut q, n).into_iter().filter(|c| c.was == content::CONTENT_TORCH));
        }
        assert_eq!(flooded.len(), 1);
        assert_eq!(map.get((1, 0, 0)), (CONTENT_WATER_FLOWING, 7));
    }
}
