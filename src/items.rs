//! Dropped items lying in the world, like builtin's "__builtin:item"
//! entity: thrown by a player (or knocked off a wall), they fall and come to
//! rest on the ground, merge with matching items nearby, and are picked up
//! by punching them. They disappear after 15 minutes, and aren't saved over
//! a restart.
//!
//! Clients move a falling item themselves (it is a physical object with
//! gravity); the server moves its own copy the same way so it knows where
//! the item ends up, and tells clients when it lands.

use crate::inventory::Stack;
use crate::messages::{item_size, GRAVITY};

/// builtin's default item_entity_ttl, in seconds.
pub const TIME_TO_LIVE: f32 = 900.0;
/// At most this many items at once; the oldest goes when another drops.
pub const MAX_ITEMS: usize = 128;
/// Object ids for items, well clear of the player ids (which are peer ids).
const FIRST_ID: u16 = 0x8000;
/// Physics time step, in seconds.
const STEP: f32 = 0.02;
/// Resting items this close (in nodes) with room for each other merge.
const MERGE_RADIUS: f32 = 1.0;
/// Items that fall this far are lost.
const LOWEST: f32 = -2000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    pub id: u16,
    pub item: u16,
    pub count: u16,
    /// A tool's wear.
    pub wear: u16,
    /// Centre, in nodes.
    pub pos: [f32; 3],
    /// Nodes per second.
    pub vel: [f32; 3],
    pub falling: bool,
    pub age: f32,
}

impl Dropped {
    pub fn stack(&self) -> Stack {
        Stack { id: self.item, count: self.count, wear: self.wear }
    }
    pub fn pos_bs(&self) -> [f32; 3] {
        self.pos.map(|v| v * 10.0)
    }
    pub fn vel_bs(&self) -> [f32; 3] {
        self.vel.map(|v| v * 10.0)
    }
}

/// What clients need to be told after a step.
#[derive(Debug, PartialEq)]
pub enum Change {
    /// New position and velocity (it landed, or started falling).
    Moved(u16),
    /// New count, so new properties.
    Resized(u16),
    Removed(u16),
}

pub struct Items {
    list: Vec<Dropped>,
    next_id: u16,
}

impl Items {
    pub fn new() -> Self {
        Self { list: Vec::new(), next_id: FIRST_ID }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Dropped> {
        self.list.iter()
    }

    pub fn get(&self, id: u16) -> Option<&Dropped> {
        self.list.iter().find(|d| d.id == id)
    }

    fn get_mut(&mut self, id: u16) -> Option<&mut Dropped> {
        self.list.iter_mut().find(|d| d.id == id)
    }

    /// Add an item stack at `pos` (nodes) moving at `vel`. If it starts
    /// inside something solid it is lifted out. Returns its id, and the id
    /// of the oldest item if that had to go to make room.
    pub fn add(&mut self, stack: Stack, motion: ([f32; 3], [f32; 3]), solid: &dyn Fn(i32, i32, i32) -> bool) -> (u16, Option<u16>) {
        let evicted = if self.list.len() >= MAX_ITEMS {
            Some(self.list.remove(0).id)
        } else {
            None
        };
        let id = self.next_id;
        self.next_id = if self.next_id == u16::MAX { FIRST_ID } else { self.next_id + 1 };
        let (mut pos, vel) = motion;
        let half = item_size(stack.count);
        for _ in 0..3 {
            if !overlaps(pos, half, solid) {
                break;
            }
            pos[1] = (pos[1] + 0.5).floor() + 0.5 + half + 1e-3;
        }
        let (item, count, wear) = (stack.id, stack.count, stack.wear);
        self.list.push(Dropped { id, item, count, wear, pos, vel, falling: true, age: 0.0 });
        (id, evicted)
    }

    pub fn remove(&mut self, id: u16) -> Option<Dropped> {
        let i = self.list.iter().position(|d| d.id == id)?;
        Some(self.list.remove(i))
    }

    pub fn set_count(&mut self, id: u16, count: u16) {
        if let Some(d) = self.get_mut(id) {
            d.count = count;
        }
    }

    /// The node at `node` changed: items resting on or near it start
    /// falling again (they stop at once if still supported).
    pub fn wake_near(&mut self, node: [i32; 3]) -> Vec<u16> {
        let mut woken = Vec::new();
        for d in &mut self.list {
            let near = (0..3).all(|i| (d.pos[i] - node[i] as f32).abs() < 1.5);
            if near && !d.falling {
                d.falling = true;
                woken.push(d.id);
            }
        }
        woken
    }

    /// Move falling items on by `dt` seconds, age everything, and merge
    /// items that came to rest next to matching ones.
    pub fn step(&mut self, dt: f32, solid: &dyn Fn(i32, i32, i32) -> bool) -> Vec<Change> {
        let mut changes = Vec::new();
        let dt = dt.min(0.5);
        let mut landed = Vec::new();
        for d in &mut self.list {
            d.age += dt;
            if !d.falling {
                continue;
            }
            let half = item_size(d.count);
            let mut t = 0.0;
            while t < dt {
                let h = STEP.min(dt - t);
                t += h;
                d.vel[1] -= GRAVITY * h;
                let mut on_ground = false;
                for axis in [1, 0, 2] {
                    let delta = d.vel[axis] * h;
                    if delta != 0.0 && move_axis(&mut d.pos, axis, delta, half, solid) {
                        on_ground |= axis == 1 && delta < 0.0;
                        d.vel[axis] = 0.0;
                    }
                }
                if on_ground {
                    // As builtin does on touching the ground: stop dead.
                    d.vel = [0.0; 3];
                    d.falling = false;
                    landed.push(d.id);
                    break;
                }
            }
        }
        let before = self.list.len();
        let mut gone = Vec::new();
        self.list.retain(|d| {
            let keep = d.age < TIME_TO_LIVE && d.pos[1] > LOWEST;
            if !keep {
                gone.push(d.id);
            }
            keep
        });
        if self.list.len() != before {
            landed.retain(|id| !gone.contains(id));
        }
        changes.extend(gone.into_iter().map(Change::Removed));
        for id in landed {
            changes.push(Change::Moved(id));
            changes.extend(self.merge_into(id));
        }
        changes
    }

    /// builtin's try_merge_with: absorb resting items of the same kind
    /// within MERGE_RADIUS, while the stack has room.
    fn merge_into(&mut self, id: u16) -> Vec<Change> {
        let mut changes = Vec::new();
        loop {
            let Some(me) = self.get(id).cloned() else {
                break;
            };
            let other = self.list.iter().find(|o| {
                o.id != id
                    && !o.falling
                    && o.item == me.item
                    && o.wear == me.wear
                    && o.count + me.count <= crate::content::stack_max(me.item)
                    && (0..3).map(|i| (o.pos[i] - me.pos[i]).powi(2)).sum::<f32>() <= MERGE_RADIUS * MERGE_RADIUS
            });
            let Some(other) = other.map(|o| (o.id, o.count, o.pos)) else {
                break;
            };
            self.remove(other.0);
            changes.push(Change::Removed(other.0));
            if let Some(d) = self.get_mut(id) {
                // builtin moves the merged item to where the other one was,
                // lifted a little as it grows.
                let old = item_size(d.count);
                d.count += other.1;
                d.pos = other.2;
                d.pos[1] += item_size(d.count) - old;
                d.age = 0.0;
            }
            changes.push(Change::Resized(id));
            changes.push(Change::Moved(id));
        }
        changes
    }
}

/// The range of node coordinates a box from `min` to `max` overlaps. Node
/// n spans n - 0.5 to n + 0.5.
fn node_range(min: f32, max: f32) -> (i32, i32) {
    ((min + 0.5).floor() as i32, (max + 0.5).ceil() as i32 - 1)
}

fn overlaps(pos: [f32; 3], half: f32, solid: &dyn Fn(i32, i32, i32) -> bool) -> bool {
    let r: [(i32, i32); 3] = std::array::from_fn(|i| node_range(pos[i] - half, pos[i] + half));
    (r[0].0..=r[0].1).any(|x| (r[1].0..=r[1].1).any(|y| (r[2].0..=r[2].1).any(|z| solid(x, y, z))))
}

/// Move a box of half size `half` at `pos` by `delta` along one axis,
/// stopping against solid nodes. Returns whether it hit one.
fn move_axis(pos: &mut [f32; 3], axis: usize, delta: f32, half: f32, solid: &dyn Fn(i32, i32, i32) -> bool) -> bool {
    let old = pos[axis];
    let mut new = *pos;
    new[axis] += delta;
    let r: [(i32, i32); 3] = std::array::from_fn(|i| node_range(new[i] - half, new[i] + half));
    let mut hit: Option<i32> = None;
    for x in r[0].0..=r[0].1 {
        for y in r[1].0..=r[1].1 {
            for z in r[2].0..=r[2].1 {
                if solid(x, y, z) {
                    let n = [x, y, z][axis];
                    hit = Some(match hit {
                        None => n,
                        Some(h) if delta < 0.0 => h.max(n),
                        Some(h) => h.min(n),
                    });
                }
            }
        }
    }
    match hit {
        None => {
            *pos = new;
            false
        }
        Some(n) => {
            // Up against the nearest solid node, but never backwards.
            let snap = if delta < 0.0 { n as f32 + 0.5 + half + 1e-4 } else { n as f32 - 0.5 - half - 1e-4 };
            pos[axis] = if delta < 0.0 { snap.clamp(new[axis], old) } else { snap.clamp(old, new[axis]) };
            true
        }
    }
}
