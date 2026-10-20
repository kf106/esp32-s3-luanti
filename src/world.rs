//! The world: generated terrain (see terrain.rs) plus the changes players
//! have made, and map block serialisation.
//!
//! A map block is 16x16x16 nodes. Each node has param0 (content id, u16),
//! param1 (light: day level in the low nibble, night level in the high
//! nibble) and param2 (unused here). Format: serialisation version 29, as
//! written by MapBlock::serialize in Luanti's src/mapblock.cpp for the
//! network (no name-id mapping, no timestamps, no static objects).
//!
//! Player changes are kept in memory and appended to a log file on the SD
//! card, one record per change, so they survive a restart:
//!   i32 x, i32 y, i32 z, u8 length, node name [, 0, param2]
//! Names rather than content ids, so the log stays valid if ids change.
//! param2 (which way a torch faces) follows a zero byte when non-zero;
//! records written before torches simply don't have it.
//!
//! Torches light the blocks we send: each has a precomputed light field,
//! spread outward through nodes that let light through, losing one level
//! per node, as Luanti does.
//! Later records override earlier ones. On startup the log is read and, if
//! it has many superseded records, rewritten.
//!
//! The changes only make sense on top of the terrain they were made in, so
//! the generator version is stored alongside (`generator`). A log from a
//! different generator is renamed out of the way and the world starts
//! fresh.

use std::cell::RefCell;
use std::collections::VecDeque;

use crate::fasthash::{FastMap, FastSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use log::{info, warn};

use crate::content::{self, TORCH_LIGHT};
use crate::messages::Writer;
use crate::storage;
use crate::terrain::{self, Column, GENERATOR_VERSION};
use crate::zstd;

pub const BLOCK_SIZE: i32 = 16;
const NODE_COUNT: usize = 16 * 16 * 16;
pub const CONTENT_AIR: u16 = 126;
const LIGHT_SUN: u8 = 15;

/// Cache limits (entries), after which a cache is simply emptied.
const COLUMN_CACHE_MAX: usize = 1024; // about 2 KB each
const CAVE_CACHE_MAX: usize = 60_000; // a few dozen bytes each

/// How far torch light reaches: it starts at TORCH_LIGHT and fades by one
/// per node, so nodes further than this get none.
const TORCH_REACH: i32 = TORCH_LIGHT as i32 - 1;
const FIELD_SIDE: i32 = 2 * TORCH_REACH + 1;

pub type BlockPos = (i16, i16, i16);
pub type NodePos = (i32, i32, i32);

/// Block coordinate containing node coordinate `n`.
pub fn block_of(n: i32) -> i16 {
    n.div_euclid(BLOCK_SIZE) as i16
}

pub fn block_of_node(p: NodePos) -> BlockPos {
    (block_of(p.0), block_of(p.1), block_of(p.2))
}

/// The edge of the world, as in Luanti (MAX_MAP_GENERATION_LIMIT): only map
/// blocks within this many nodes of the origin exist. Beyond them the
/// client has nothing, and it treats that as solid: an invisible wall.
pub const MAP_LIMIT: i32 = 31007;

/// Whether a map block is inside the world (blockpos_over_max_limit in
/// Luanti's mapblock.h, which makes the edge at -30992 and +31007).
pub fn block_in_world(p: BlockPos) -> bool {
    let limit = (MAP_LIMIT / BLOCK_SIZE) as i16;
    [p.0, p.1, p.2].iter().all(|v| (-limit..=limit).contains(v))
}

/// The 16x16 columns of a block column, their height range, and the trees
/// growing in or into it.
struct BlockColumn {
    columns: Vec<Column>, // index z * 16 + x
    min_height: i32,
    max_height: i32,
    trees: Vec<terrain::Tree>,
    /// Per column, the highest y a tree could reach there (i32::MIN for
    /// none): above it, and above the surface, there is only air.
    tree_top: Vec<i32>,
    /// The highest of those.
    max_tree_top: i32,
}

impl BlockColumn {
    /// The generated node above the surface: a tree's, or air.
    fn above_ground(&self, x: i32, y: i32, z: i32) -> u16 {
        if y > self.tree_top[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize] {
            return CONTENT_AIR;
        }
        terrain::tree_node_in(&self.trees, x, y, z).unwrap_or(CONTENT_AIR)
    }

    /// How high anything generated goes in this column (surface or tree).
    fn generated_top(&self, x: i32, z: i32) -> i32 {
        let i = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        self.columns[i].height.max(self.tree_top[i])
    }
}

/// How many recent changes are remembered for `changed_near_since`.
const RECENT_CHANGES: usize = 512;

/// What players have done to the world: everything that isn't generated.
/// Shared (copy on write) with the thread that builds map blocks, which
/// works from a snapshot while the server carries on changing the world.
#[derive(Clone, Default)]
pub struct State {
    changes: FastMap<NodePos, u16>,
    /// param2 of changed nodes, where non-zero.
    param2s: FastMap<NodePos, u8>,
    /// Light fields of torches, FIELD_SIDE^3 levels around each.
    torch_light: FastMap<NodePos, Arc<Vec<u8>>>,
    /// Highest y of any solid node players have placed in each column, so
    /// sunlight is blocked by towers and roofs.
    column_top: FastMap<(i32, i32), i32>,
    /// Map blocks containing any changes, so unchanged blocks skip the
    /// per-node lookups.
    changed_blocks: FastSet<BlockPos>,
    /// Counts changes; a block built from a snapshot is tagged with it.
    version: u64,
    /// The latest changes and their versions, oldest first.
    recent: VecDeque<(u64, NodePos)>,
}

impl State {
    pub fn version(&self) -> u64 {
        self.version
    }
}

pub struct World {
    state: Arc<State>,
    /// The change log on the SD card; None for a copy that only builds
    /// map blocks.
    log_path: Option<PathBuf>,
    /// Terrain caches (filled on demand; each World is used by one thread).
    block_columns: RefCell<FastMap<(i16, i16), Arc<BlockColumn>>>,
    cave_samples: RefCell<FastMap<(i32, i32, i32), (f32, f32)>>,
}

impl World {
    /// Load the world's changes from `dir` (created if missing).
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let log_path = dir.join("nodes.log");
        Self::check_generator(&dir, &log_path)?;
        let mut world = Self {
            state: Arc::new(State::default()),
            log_path: Some(log_path.clone()),
            block_columns: RefCell::new(FastMap::default()),
            cave_samples: RefCell::new(FastMap::default()),
        };

        let data = match storage::read(&log_path) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        let (records, unknown) = world.load_log(&data);
        info!(
            "World: {} changed nodes from {} log records{}",
            world.state.changes.len(),
            records,
            if unknown > 0 { format!(" ({unknown} with unknown node names skipped)") } else { String::new() }
        );
        // Light up the torches now that the world is complete.
        let torches: Vec<NodePos> = world.state.changes.iter().filter(|(_, &id)| content::is_torch(id)).map(|(&p, _)| p).collect();
        for p in torches {
            let field = Arc::new(world.torch_field(p));
            Arc::make_mut(&mut world.state).torch_light.insert(p, field);
        }
        if !world.state.torch_light.is_empty() {
            info!("World: {} torches lit", world.state.torch_light.len());
        }
        if records > 2 * world.state.changes.len() + 256 {
            if let Err(e) = world.compact(&log_path) {
                warn!("Compacting {}: {e}", log_path.display());
            }
        }
        Ok(world)
    }

    /// A world for building map blocks on another thread: no change log,
    /// its own terrain caches, and the changes from `snapshot` (replaced
    /// with `use_snapshot` before each block).
    pub fn builder() -> Self {
        Self {
            state: Arc::new(State::default()),
            log_path: None,
            block_columns: RefCell::new(FastMap::default()),
            cave_samples: RefCell::new(FastMap::default()),
        }
    }

    /// The current changes, cheaply shared.
    pub fn snapshot(&self) -> Arc<State> {
        self.state.clone()
    }

    pub fn use_snapshot(&mut self, state: Arc<State>) {
        self.state = state;
    }

    pub fn version(&self) -> u64 {
        self.state.version
    }

    /// Whether a change since `version` could make a copy of block `pos`
    /// built then out of date: a change in or near it (torch light reaches
    /// TORCH_REACH nodes) or above it (shade from a new roof). True if
    /// `version` is too old to tell.
    pub fn changed_near_since(&self, pos: BlockPos, version: u64) -> bool {
        if version == self.state.version {
            return false;
        }
        let oldest = self.state.recent.front().map_or(u64::MAX, |r| r.0);
        if version + 1 < oldest {
            return true; // the changes since then are no longer listed
        }
        let (bx, by, bz) = (pos.0 as i32 * 16, pos.1 as i32 * 16, pos.2 as i32 * 16);
        let near = |v: i32, b: i32| v >= b - TORCH_REACH && v < b + BLOCK_SIZE + TORCH_REACH;
        self.state
            .recent
            .iter()
            .rev()
            .take_while(|(v, _)| *v > version)
            .any(|(_, p)| near(p.0, bx) && near(p.2, bz) && p.1 >= by - TORCH_REACH)
    }

    /// If the change log was made with another terrain generator, rename it
    /// (it would make no sense on new terrain) and record the current one.
    fn check_generator(dir: &std::path::Path, log_path: &std::path::Path) -> io::Result<()> {
        let version_path = dir.join("generator");
        let stored = fs::read_to_string(&version_path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(1); // worlds from before versioning
        if stored == GENERATOR_VERSION {
            return Ok(());
        }
        if log_path.exists() {
            let old = dir.join(format!("nodes-v{stored}.log"));
            warn!(
                "World: the terrain generator changed (v{stored} to v{GENERATOR_VERSION}); \
                 keeping the old changes as {} and starting afresh",
                old.display()
            );
            let _ = fs::remove_file(&old);
            fs::rename(log_path, &old)?;
        }
        storage::write(&version_path, GENERATOR_VERSION.to_string().as_bytes())
    }

    fn load_log(&mut self, mut data: &[u8]) -> (usize, usize) {
        // Loading isn't "changing": no versions or recent list.
        let (mut records, mut unknown) = (0, 0);
        while data.len() >= 13 {
            let i = |o: usize| i32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            let pos = (i(0), i(4), i(8));
            let len = data[12] as usize;
            let Some(field) = data.get(13..13 + len) else {
                break; // truncated by a power cut mid-write
            };
            let (name, param2) = match field.iter().position(|&b| b == 0) {
                Some(i) => (&field[..i], field.get(i + 1).copied().unwrap_or(0)),
                None => (field, 0),
            };
            match content::id_of(&String::from_utf8_lossy(name)) {
                // Torches saved before there were wall and ceiling torch
                // nodes get the right one for the way they face.
                Some(id) => self.apply(pos, content::torch_variant(id, param2), param2),
                None => unknown += 1,
            }
            records += 1;
            data = &data[13 + len..];
        }
        if !data.is_empty() {
            warn!("World log ends with {} bytes of a partial record; ignoring them", data.len());
        }
        (records, unknown)
    }

    /// Rewrite the log with one record per changed node.
    fn compact(&self, log_path: &std::path::Path) -> io::Result<()> {
        let mut out = Vec::new();
        for (&pos, &id) in &self.state.changes {
            out.extend_from_slice(&record(pos, id, self.param2(pos)));
        }
        storage::write(log_path, &out)?;
        info!("World log compacted to {} records", self.state.changes.len());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Terrain with caches
    // -----------------------------------------------------------------------

    fn block_column(&self, bx: i16, bz: i16) -> Arc<BlockColumn> {
        if let Some(c) = self.block_columns.borrow().get(&(bx, bz)) {
            return c.clone();
        }
        let mut columns = Vec::with_capacity(256);
        let (x0, z0) = (bx as i32 * 16, bz as i32 * 16);
        for z in 0..16 {
            for x in 0..16 {
                columns.push(terrain::column(x0 + x, z0 + z));
            }
        }
        let min_height = columns.iter().map(|c| c.height).min().unwrap_or(0);
        let max_height = columns.iter().map(|c| c.height).max().unwrap_or(0);
        let trees = terrain::trees_in(x0, z0, x0 + 15, z0 + 15);
        let mut tree_top = vec![i32::MIN; 256];
        for t in &trees {
            for z in (t.z - terrain::TREE_REACH).max(z0)..=(t.z + terrain::TREE_REACH).min(z0 + 15) {
                for x in (t.x - terrain::TREE_REACH).max(x0)..=(t.x + terrain::TREE_REACH).min(x0 + 15) {
                    let top = &mut tree_top[((z - z0) * 16 + x - x0) as usize];
                    *top = (*top).max(t.top());
                }
            }
        }
        let max_tree_top = tree_top.iter().copied().max().unwrap_or(i32::MIN);
        let bc = Arc::new(BlockColumn { columns, min_height, max_height, trees, tree_top, max_tree_top });
        let mut cache = self.block_columns.borrow_mut();
        if cache.len() >= COLUMN_CACHE_MAX {
            cache.clear();
        }
        cache.insert((bx, bz), bc.clone());
        bc
    }

    fn column(&self, x: i32, z: i32) -> Column {
        let bc = self.block_column(block_of(x), block_of(z));
        bc.columns[(z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize]
    }

    /// Lowest generated surface and highest generated node (surface or
    /// tree) in a block column, for deciding which blocks to send.
    pub fn height_range(&self, bx: i16, bz: i16) -> (i32, i32) {
        let bc = self.block_column(bx, bz);
        (bc.min_height, bc.max_height.max(bc.max_tree_top))
    }

    fn generated(&self, x: i32, y: i32, z: i32, col: &Column) -> u16 {
        // Above the surface it's air or trees: no need to look at caves.
        if y > col.height {
            return self.block_column(block_of(x), block_of(z)).above_ground(x, y, z);
        }
        let mut cache = self.cave_samples.borrow_mut();
        if cache.len() >= CAVE_CACHE_MAX {
            cache.clear();
        }
        terrain::node(x, y, z, col, &mut |gx, gy, gz| {
            *cache.entry((gx, gy, gz)).or_insert_with(|| terrain::cave_sample(gx, gy, gz))
        })
    }

    // -----------------------------------------------------------------------
    // Nodes
    // -----------------------------------------------------------------------

    fn apply(&mut self, pos: NodePos, id: u16, param2: u8) {
        let col = self.column(pos.0, pos.2);
        let generated = self.generated(pos.0, pos.1, pos.2, &col);
        let st = Arc::make_mut(&mut self.state);
        // Kept even if the change is undone; it only costs some lookups.
        st.changed_blocks.insert(block_of_node(pos));
        if id == generated && param2 == 0 {
            st.changes.remove(&pos);
        } else {
            st.changes.insert(pos, id);
        }
        if param2 == 0 {
            st.param2s.remove(&pos);
        } else {
            st.param2s.insert(pos, param2);
        }
        if !content::passes_sunlight(id) {
            let top = st.column_top.entry((pos.0, pos.2)).or_insert(pos.1);
            *top = (*top).max(pos.1);
        }
    }

    pub fn param2(&self, pos: NodePos) -> u8 {
        self.state.param2s.get(&pos).copied().unwrap_or(0)
    }

    pub fn node(&self, pos: NodePos) -> u16 {
        let col = self.column(pos.0, pos.2);
        self.node_with(pos.0, pos.1, pos.2, &col)
    }

    fn node_with(&self, x: i32, y: i32, z: i32, col: &Column) -> u16 {
        if let Some(&id) = self.state.changes.get(&(x, y, z)) {
            return id;
        }
        self.generated(x, y, z, col)
    }

    /// Change a node and record it on the SD card.
    pub fn set_node(&mut self, pos: NodePos, id: u16, param2: u8) {
        let was_torch = content::is_torch(self.node(pos));
        self.apply(pos, id, param2);
        if let Some(log_path) = &self.log_path {
            let result = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .and_then(|mut f: File| f.write_all(&record(pos, id, param2)));
            if let Err(e) = result {
                warn!("Saving node change to {}: {e}", log_path.display());
            }
        }
        self.relight(pos, was_torch, content::is_torch(id));
        let st = Arc::make_mut(&mut self.state);
        st.version += 1;
        let version = st.version;
        st.recent.push_back((version, pos));
        if st.recent.len() > RECENT_CHANGES {
            st.recent.pop_front();
        }
    }

    /// Keep torch light up to date after a node changed: the torch there
    /// (if any) and any torch whose light could reach it.
    fn relight(&mut self, pos: NodePos, was_torch: bool, is_torch: bool) {
        if was_torch && !is_torch {
            Arc::make_mut(&mut self.state).torch_light.remove(&pos);
        }
        let near = |t: &NodePos| {
            (t.0 - pos.0).abs() <= TORCH_REACH && (t.1 - pos.1).abs() <= TORCH_REACH && (t.2 - pos.2).abs() <= TORCH_REACH
        };
        let mut affected: Vec<NodePos> = self.state.torch_light.keys().filter(|t| near(t)).copied().collect();
        if is_torch && !affected.contains(&pos) {
            affected.push(pos);
        }
        for t in affected {
            let field = Arc::new(self.torch_field(t));
            Arc::make_mut(&mut self.state).torch_light.insert(t, field);
        }
    }

    /// Light levels around a torch: TORCH_LIGHT at the torch, one less per
    /// node outward through nodes that let light through.
    fn torch_field(&self, t: NodePos) -> Vec<u8> {
        let side = FIELD_SIDE as usize;
        let mut field = vec![0u8; side * side * side];
        let index = |d: (i32, i32, i32)| {
            ((d.2 + TORCH_REACH) as usize * side + (d.1 + TORCH_REACH) as usize) * side + (d.0 + TORCH_REACH) as usize
        };
        field[index((0, 0, 0))] = TORCH_LIGHT;
        let mut queue: VecDeque<((i32, i32, i32), u8)> = VecDeque::from([((0, 0, 0), TORCH_LIGHT)]);
        while let Some((d, l)) = queue.pop_front() {
            if l <= 1 {
                continue;
            }
            for (sx, sy, sz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let n = (d.0 + sx, d.1 + sy, d.2 + sz);
                if n.0.abs() > TORCH_REACH || n.1.abs() > TORCH_REACH || n.2.abs() > TORCH_REACH {
                    continue;
                }
                let i = index(n);
                if field[i] >= l - 1 {
                    continue;
                }
                if !content::passes_light(self.node((t.0 + n.0, t.1 + n.1, t.2 + n.2))) {
                    continue;
                }
                field[i] = l - 1;
                queue.push_back((n, l - 1));
            }
        }
        field
    }

    /// The y of the highest solid node in a column (leaves included): below
    /// it is out of direct sunlight, and on it is where a player can stand.
    pub fn highest_solid(&self, x: i32, z: i32) -> i32 {
        let col = self.column(x, z);
        self.highest_solid_with(x, z, &col)
    }

    fn highest_solid_with(&self, x: i32, z: i32, col: &Column) -> i32 {
        let generated = self.block_column(block_of(x), block_of(z)).generated_top(x, z);
        let start = generated.max(self.state.column_top.get(&(x, z)).copied().unwrap_or(i32::MIN));
        let mut y = start;
        // Scan down past dug-out nodes and cave mouths. A very deep shaft is
        // rare; stop after a while rather than scan forever.
        while content::passes_sunlight(self.node_with(x, y, z, col)) && y > start - 256 {
            y -= 1;
        }
        y
    }

    /// The uncompressed version 29 block data.
    pub fn block_raw(&self, pos: BlockPos) -> Vec<u8> {
        let (bx, by, bz) = (pos.0 as i32 * 16, pos.1 as i32 * 16, pos.2 as i32 * 16);
        let idx = |x: i32, y: i32, z: i32| (z * 256 + y * 16 + x) as usize;
        let mut param0 = vec![0u16; NODE_COUNT];
        let mut light = vec![0u8; NODE_COUNT];
        let mut param2 = vec![0u8; NODE_COUNT];
        let mut any_air = false;
        let mut any_solid = false;
        let mut lit = VecDeque::new();
        let mut any_shaded = false;
        let bc = self.block_column(pos.0, pos.2);
        let changed = self.state.changed_blocks.contains(&pos);
        // Cave samples for the 5x5x5 grid points around this block, filled
        // on first use. Same values as the shared cache, just quicker to
        // reach than through a hash map.
        let (g0x, g0y, g0z) = (pos.0 as i32 * 4, pos.1 as i32 * 4, pos.2 as i32 * 4);
        let mut grid = [(0.0f32, 0.0f32); 125];
        let mut grid_done = [false; 125];
        let mut local = |gx: i32, gy: i32, gz: i32| {
            let i = ((gz - g0z) * 25 + (gy - g0y) * 5 + (gx - g0x)) as usize;
            if !grid_done[i] {
                grid[i] = terrain::cave_sample(gx, gy, gz);
                grid_done[i] = true;
            }
            grid[i]
        };
        // Corner samples per 4x4x4 cell of the block, gathered once.
        let mut cells: [Option<[(f32, f32); 8]>; 64] = [None; 64];

        for z in 0..BLOCK_SIZE {
            for x in 0..BLOCK_SIZE {
                let col = bc.columns[(z * 16 + x) as usize];
                // Where sunlight stops. A block wholly above every possible
                // solid node needs no scan.
                let top = bc
                    .generated_top(bx + x, bz + z)
                    .max(self.state.column_top.get(&(bx + x, bz + z)).copied().unwrap_or(i32::MIN));
                let floor = if top < by { top } else { self.highest_solid_with(bx + x, bz + z, &col) };
                for y in 0..BLOCK_SIZE {
                    let (wx, wy, wz) = (bx + x, by + y, bz + z);
                    // Look for a player's change only in blocks that have any.
                    let change = if changed { self.state.changes.get(&(wx, wy, wz)) } else { None };
                    let id = match change {
                        Some(&id) => id,
                        _ if wy > col.height => bc.above_ground(wx, wy, wz),
                        _ => {
                            let cell = ((z / 4) * 16 + (y / 4) * 4 + x / 4) as usize;
                            let corners = *cells[cell]
                                .get_or_insert_with(|| terrain::corners_at(wx, wy, wz, &mut local));
                            terrain::finish(wx, wy, wz, &col, || {
                                terrain::cave_from_corners(&corners, wx, wy, wz)
                            })
                        }
                    };
                    let i = idx(x, y, z);
                    param0[i] = id;
                    if changed {
                        param2[i] = self.param2((wx, wy, wz));
                    }
                    if content::passes_light(id) {
                        any_air |= id == CONTENT_AIR;
                        any_solid |= id != CONTENT_AIR; // flag 0x02 means "not all air"
                        if wy > floor {
                            light[i] = LIGHT_SUN;
                            lit.push_back((x, y, z));
                        } else {
                            any_shaded = true;
                        }
                    } else {
                        any_solid = true;
                    }
                }
            }
        }

        // Spread light sideways and down into shaded air, one level dimmer
        // per node, as Luanti does. Only within this block: light from
        // neighbouring blocks isn't considered.
        if !any_shaded {
            lit.clear(); // nothing in the shade, so nothing to spread into
        }
        while let Some((x, y, z)) = lit.pop_front() {
            let l = light[idx(x, y, z)];
            if l <= 1 {
                continue;
            }
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                if !(0..16).contains(&nx) || !(0..16).contains(&ny) || !(0..16).contains(&nz) {
                    continue;
                }
                let j = idx(nx, ny, nz);
                if content::passes_light(param0[j]) && light[j] < l - 1 {
                    light[j] = l - 1;
                    lit.push_back((nx, ny, nz));
                }
            }
        }

        // Torch light: in the day bank where it beats sunlight, and alone in
        // the night bank (sunlight has none at night).
        let mut night = vec![0u8; NODE_COUNT];
        let in_block = |t: &&NodePos| {
            let reach = |t: i32, b: i32| t + TORCH_REACH >= b && t - TORCH_REACH < b + BLOCK_SIZE;
            reach(t.0, bx) && reach(t.1, by) && reach(t.2, bz)
        };
        for (t, field) in self.state.torch_light.iter().filter(|(t, _)| in_block(t)) {
            let side = FIELD_SIDE;
            for z in 0..BLOCK_SIZE {
                let dz = bz + z - t.2;
                if dz.abs() > TORCH_REACH {
                    continue;
                }
                for y in 0..BLOCK_SIZE {
                    let dy = by + y - t.1;
                    if dy.abs() > TORCH_REACH {
                        continue;
                    }
                    for x in 0..BLOCK_SIZE {
                        let dx = bx + x - t.0;
                        if dx.abs() > TORCH_REACH {
                            continue;
                        }
                        let l = field[(((dz + TORCH_REACH) * side + dy + TORCH_REACH) * side + dx + TORCH_REACH) as usize];
                        let i = idx(x, y, z);
                        light[i] = light[i].max(l);
                        night[i] = night[i].max(l);
                    }
                }
            }
        }
        for (l, n) in light.iter_mut().zip(&night) {
            *l |= n << 4;
        }

        let mut flags = 0u8;
        if !any_air {
            flags |= 0x01; // is_underground
        }
        if any_solid {
            flags |= 0x02; // not entirely air
        }
        // 0x08 would mean "not generated"; ours are.

        let mut w = Writer::with_capacity(5 + NODE_COUNT * 4 + 1);
        w.u8(flags)
            .u16(0xFFFF) // lighting complete in all directions, day and night
            .u8(2) // content width
            .u8(2); // params width
        for id in &param0 {
            w.u16(*id);
        }
        // param1: day light in the low nibble, night light in the high
        w.raw(&light);
        w.raw(&param2);
        w.u8(0); // node metadata: none
        w.into_inner()
    }

    /// The payload after the command id of TOCLIENT_BLOCKDATA: position,
    /// zstd-compressed block, then the network-specific part (version 2).
    pub fn block_payload(&self, pos: BlockPos) -> Vec<u8> {
        let compressed = zstd::compress(&self.block_raw(pos));
        let mut w = Writer::plain();
        w.i16(pos.0).i16(pos.1).i16(pos.2);
        w.raw(&compressed);
        w.u8(2);
        w.into_inner()
    }
}

fn record(pos: NodePos, id: u16, param2: u8) -> Vec<u8> {
    let name = content::name_of(id);
    let mut field = name.as_bytes().to_vec();
    if param2 != 0 {
        field.extend_from_slice(&[0, param2]);
    }
    let mut r = Vec::with_capacity(13 + field.len());
    r.extend_from_slice(&pos.0.to_be_bytes());
    r.extend_from_slice(&pos.1.to_be_bytes());
    r.extend_from_slice(&pos.2.to_be_bytes());
    r.push(field.len() as u8);
    r.extend_from_slice(&field);
    r
}
