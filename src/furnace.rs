//! Furnaces, as minetest_game's (mods/default/furnace.lua): a block with
//! its own inventory of three lists, "src" (what to smelt, 1 slot), "fuel"
//! (1 slot) and "dst" (the results, 2x2).
//!
//! minetest_game keeps these in the node's metadata. Here they are kept by
//! the server and shown to the player who opens the furnace as a detached
//! inventory ("detached:esp_furnace_<x>_<y>_<z>"), which looks the same to
//! the player and keeps node metadata out of the map blocks.
//!
//! Saved in one text file, rewritten when a furnace's contents change:
//!   furnace <x> <y> <z>
//!   time <fuel burnt> <fuel total> <cooked>   (while it burns)
//!   <list> <slot> <item string>        (one line per filled slot)

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::crafting;
use crate::inventory::{self, Inventory, List, Stack};
use crate::storage;
use crate::world::NodePos;

/// The detached inventory name prefix; the furnace's position follows.
const DETACHED_PREFIX: &str = "esp_furnace_";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FurnaceList {
    Src,
    Fuel,
    Dst,
}

impl FurnaceList {
    pub const ALL: [FurnaceList; 3] = [FurnaceList::Src, FurnaceList::Fuel, FurnaceList::Dst];

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "src" => Some(Self::Src),
            "fuel" => Some(Self::Fuel),
            "dst" => Some(Self::Dst),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Src => "src",
            Self::Fuel => "fuel",
            Self::Dst => "dst",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Furnace {
    pub src: [Option<Stack>; 1],
    pub fuel: [Option<Stack>; 1],
    pub dst: [Option<Stack>; 4],
    /// How long the current fuel has burnt, out of how long it lasts, and
    /// how long the item being smelted has cooked (seconds), as
    /// minetest_game keeps them in the node's metadata.
    pub fuel_time: f32,
    pub fuel_totaltime: f32,
    pub src_time: f32,
    /// Its timer is running: it is burning, or something was just put in
    /// or taken out and it needs to see whether it can burn.
    pub running: bool,
    /// Timer steps since it lit, for the fire sound every 5 seconds.
    timer_elapsed: u32,
    /// What the window shows: None unlit, or the fuel left and the cooking
    /// progress in percent.
    pub display: Option<(u8, u8)>,
    /// The fire sounds playing, to fade out when it goes out.
    pub sounds: Vec<i32>,
}

/// What a step of a furnace's timer did.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// It is burning (after the step).
    pub active: bool,
    /// Items smelted (or ready but with no room in the output), for the
    /// cooling sound.
    pub smelted: u32,
    /// Its contents changed (fuel taken, an item smelted).
    pub changed: bool,
    /// Time to play the fire sound again.
    pub fire_sound: bool,
}

impl Furnace {
    pub fn list(&self, list: FurnaceList) -> &[Option<Stack>] {
        match list {
            FurnaceList::Src => &self.src,
            FurnaceList::Fuel => &self.fuel,
            FurnaceList::Dst => &self.dst,
        }
    }

    pub fn list_mut(&mut self, list: FurnaceList) -> &mut [Option<Stack>] {
        match list {
            FurnaceList::Src => &mut self.src,
            FurnaceList::Fuel => &mut self.fuel,
            FurnaceList::Dst => &mut self.dst,
        }
    }

    /// minetest_game's can_dig: only an empty furnace can be dug.
    pub fn is_empty(&self) -> bool {
        FurnaceList::ALL.iter().all(|&l| self.list(l).iter().all(Option::is_none))
    }

    /// The lists as Luanti sends them: (name, width, slots).
    pub fn lists(&self) -> Vec<(&'static str, u32, Vec<Option<Stack>>)> {
        FurnaceList::ALL.iter().map(|&l| (l.name(), 0, self.list(l).to_vec())).collect()
    }

    /// minetest_game's furnace_node_timer, for `elapsed` seconds: burn
    /// fuel while there is something to smelt, smelt one item at a time
    /// into the output, take new fuel when the old is used up, and go out
    /// when there is nothing left to smelt or burn.
    pub fn step(&mut self, mut elapsed: f32) -> Step {
        let timer_elapsed = self.timer_elapsed;
        self.timer_elapsed += 1;
        let mut out = Step::default();
        let mut cooked: Option<(u16, f32)> = None;
        let mut fuel_taken: Option<f32> = None;
        let mut update = true;
        while elapsed > 0.0 && update {
            update = false;
            cooked = self.src[0].and_then(|s| crafting::cook_result(s.id));
            let mut el = elapsed.min(self.fuel_totaltime - self.fuel_time);
            if let Some((_, time)) = cooked {
                // Fuel lasts long enough: step to when the item is done.
                el = el.min(time - self.src_time);
            }
            if self.fuel_time < self.fuel_totaltime {
                // Burning, with fuel left.
                self.fuel_time += el;
                if let Some((item, time)) = cooked {
                    self.src_time += el;
                    if self.src_time >= time {
                        let made = Stack::new(item, 1);
                        if room_for(&self.dst, made) {
                            inventory::add_to(&mut self.dst, made);
                            take_one(&mut self.src[0]);
                            self.src_time -= time;
                            out.changed = true;
                            update = true;
                        }
                        // (With no room in the output it waits, done.)
                        out.smelted += 1;
                    } else {
                        // Not done yet: probably the fuel ran out first.
                        update = true;
                    }
                }
            } else {
                // The fuel is used up.
                if cooked.is_some() {
                    let burn = self.fuel[0].map_or(0.0, |f| crafting::burn_time(f.id));
                    if burn == 0.0 {
                        // No fuel left.
                        self.fuel_totaltime = 0.0;
                        self.src_time = 0.0;
                    } else {
                        take_one(&mut self.fuel[0]);
                        out.changed = true;
                        update = true;
                        self.fuel_totaltime = burn + (self.fuel_totaltime - self.fuel_time);
                        fuel_taken = Some(burn);
                    }
                } else {
                    // Nothing to smelt, so no new fuel.
                    self.fuel_totaltime = 0.0;
                    self.src_time = 0.0;
                }
                self.fuel_time = 0.0;
            }
            elapsed -= el;
        }
        if let Some(burn) = fuel_taken {
            self.fuel_totaltime = self.fuel_totaltime.min(burn);
        }
        if self.src[0].is_none() {
            self.src_time = 0.0;
        }
        let item_percent = cooked.map_or(0.0, |(_, time)| (self.src_time / time * 100.0).floor());
        out.active = self.fuel_totaltime != 0.0;
        if out.active {
            let fuel_percent = 100.0 - (self.fuel_time / self.fuel_totaltime * 100.0).floor();
            self.display = Some((fuel_percent.clamp(0.0, 100.0) as u8, item_percent.clamp(0.0, 100.0) as u8));
            out.fire_sound = timer_elapsed == 0 || (timer_elapsed + 1) % 5 == 0;
        } else {
            self.display = None;
            self.running = false;
            self.timer_elapsed = 0;
        }
        out
    }
}

/// Whether a stack fits into a list (InventoryList::roomForItem).
fn room_for(list: &[Option<Stack>], stack: Stack) -> bool {
    let mut copy = list.to_vec();
    inventory::add_to(&mut copy, stack) == 0
}

fn take_one(slot: &mut Option<Stack>) {
    if let Some(s) = slot {
        s.count -= 1;
        if s.count == 0 {
            *slot = None;
        }
    }
}

/// minetest_game's allow_metadata_inventory_put: anything can go in to be
/// smelted, only fuel into the fuel slot, and nothing into the output.
pub fn may_put(list: FurnaceList, stack: &Stack) -> bool {
    match list {
        FurnaceList::Src => true,
        FurnaceList::Fuel => crafting::burn_time(stack.id) > 0.0,
        FurnaceList::Dst => false,
    }
}

/// The detached inventory name for the furnace at `pos`.
pub fn detached_name(pos: NodePos) -> String {
    format!("{DETACHED_PREFIX}{}_{}_{}", pos.0, pos.1, pos.2)
}

/// The furnace position in an inventory location such as
/// "detached:esp_furnace_3_-2_10".
pub fn pos_of_location(location: &str) -> Option<NodePos> {
    let rest = location.strip_prefix("detached:")?.strip_prefix(DETACHED_PREFIX)?;
    let mut f = rest.split('_').map(|v| v.parse::<i32>().ok());
    let pos = (f.next()??, f.next()??, f.next()??);
    f.next().is_none().then_some(pos)
}

/// All the furnaces in the world, by position.
pub struct Furnaces {
    path: PathBuf,
    map: BTreeMap<NodePos, Furnace>,
}

impl Furnaces {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let map = match storage::read(&path) {
            Ok(text) => parse(&String::from_utf8_lossy(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => {
                warn!("Reading {}: {e}", path.display());
                BTreeMap::new()
            }
        };
        if !map.is_empty() {
            log::info!("{} furnaces", map.len());
        }
        let mut map = map;
        // Let each furnace's timer see whether it should be burning.
        for f in map.values_mut() {
            f.running = true;
        }
        Self { path, map }
    }

    pub fn get(&self, pos: NodePos) -> Option<&Furnace> {
        self.map.get(&pos)
    }

    pub fn get_mut(&mut self, pos: NodePos) -> Option<&mut Furnace> {
        self.map.get_mut(&pos)
    }

    /// The furnace at `pos`, made empty if there wasn't one on record (a
    /// furnace placed, or one whose record was lost).
    pub fn get_or_add(&mut self, pos: NodePos) -> &mut Furnace {
        self.map.entry(pos).or_default()
    }

    /// Take a furnace's record away (it was dug), returning it.
    pub fn remove(&mut self, pos: NodePos) -> Option<Furnace> {
        let gone = self.map.remove(&pos);
        if gone.is_some() {
            self.save();
        }
        gone
    }

    /// Positions of the furnaces whose timers are running.
    pub fn running(&self) -> Vec<NodePos> {
        self.map.iter().filter(|(_, f)| f.running).map(|(p, _)| *p).collect()
    }

    pub fn save(&self) {
        if let Err(e) = storage::write(&self.path, format(&self.map).as_bytes()) {
            warn!("Saving {}: {e}", self.path.display());
            crate::status::problem();
        }
    }
}

fn format(map: &BTreeMap<NodePos, Furnace>) -> String {
    let mut t = String::new();
    for (pos, f) in map {
        t.push_str(&format!("furnace {} {} {}\n", pos.0, pos.1, pos.2));
        if f.fuel_totaltime != 0.0 || f.src_time != 0.0 {
            t.push_str(&format!("time {} {} {}\n", f.fuel_time, f.fuel_totaltime, f.src_time));
        }
        for list in FurnaceList::ALL {
            for (i, s) in f.list(list).iter().enumerate() {
                if let Some(s) = s {
                    t.push_str(&format!("{} {i} {}\n", list.name(), s.item_string()));
                }
            }
        }
    }
    t
}

fn parse(text: &str) -> BTreeMap<NodePos, Furnace> {
    let mut map = BTreeMap::new();
    let mut current: Option<NodePos> = None;
    for line in text.lines() {
        let mut f = line.split_whitespace();
        match f.next() {
            Some("furnace") => {
                let v: Vec<i32> = f.filter_map(|v| v.parse().ok()).collect();
                current = (v.len() == 3).then(|| (v[0], v[1], v[2]));
                if let Some(pos) = current {
                    map.insert(pos, Furnace::default());
                }
            }
            Some("time") => {
                let v: Vec<f32> = f.filter_map(|v| v.parse().ok()).collect();
                match (current.and_then(|p| map.get_mut(&p)), v.as_slice()) {
                    (Some(fu), &[fuel_time, fuel_totaltime, src_time]) => {
                        (fu.fuel_time, fu.fuel_totaltime, fu.src_time) = (fuel_time, fuel_totaltime, src_time);
                    }
                    _ => warn!("Skipping furnace line \"{line}\""),
                }
            }
            Some(name) => {
                let list = FurnaceList::from_name(name);
                let slot = f.next().and_then(|s| s.parse::<usize>().ok());
                let stack = Stack::parse(&f.collect::<Vec<_>>().join(" "));
                let furnace = current.and_then(|p| map.get_mut(&p));
                match (furnace, list, slot, stack) {
                    (Some(fu), Some(list), Some(slot), Some(stack)) if slot < fu.list(list).len() => {
                        fu.list_mut(list)[slot] = Some(stack);
                    }
                    _ => warn!("Skipping furnace line \"{line}\""),
                }
            }
            None => {}
        }
    }
    map
}

/// Where a list named in an inventory action is: one of the player's own,
/// or one of a furnace's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Own(List),
    Furnace(NodePos, FurnaceList),
}

/// A player's inventory and the furnaces, for inventory actions that may
/// move items between them (IMoveAction, with minetest_game's rules).
pub struct Lists<'a> {
    pub player: &'a mut Inventory,
    pub furnaces: &'a mut Furnaces,
}

impl Lists<'_> {
    fn slots(&mut self, place: Place) -> Option<&mut [Option<Stack>]> {
        match place {
            Place::Own(list) => Some(self.player.list_mut(list)),
            Place::Furnace(pos, list) => self.furnaces.get_mut(pos).map(|f| f.list_mut(list)),
        }
    }

    fn get(&mut self, (place, i): (Place, usize)) -> Option<Option<Stack>> {
        self.slots(place)?.get(i).copied()
    }

    fn set(&mut self, (place, i): (Place, usize), stack: Option<Stack>) {
        if let Some(slot) = self.slots(place).and_then(|s| s.get_mut(i)) {
            *slot = stack;
        }
    }

    /// Whether a stack may be put into a list: nothing into the craft
    /// result, and the furnace's own rules.
    fn may_put(place: Place, stack: &Stack) -> bool {
        match place {
            Place::Own(list) => list != List::CraftResult,
            Place::Furnace(_, list) => may_put(list, stack),
        }
    }

    /// Move `count` items (0: all) between two slots, which may be in
    /// different inventories. Returns false if the move isn't possible or
    /// allowed.
    pub fn move_items(&mut self, from: (Place, usize), to: (Place, usize), count: u16) -> bool {
        if from == to {
            return false;
        }
        let (Some(src), Some(dst)) = (self.get(from), self.get(to)) else {
            return false;
        };
        let Some((new_src, new_dst)) = inventory::plan_move(src, dst, count) else {
            return false;
        };
        // Whatever ends up in each slot must be allowed there: after a
        // swap, the source slot gets the destination's item.
        let swapped = new_src.is_some_and(|n| src.is_some_and(|s| n.id != s.id || n.wear != s.wear));
        if new_dst.is_some_and(|d| !Self::may_put(to.0, &d))
            || (swapped && new_src.is_some_and(|n| !Self::may_put(from.0, &n)))
        {
            return false;
        }
        self.set(from, new_src);
        self.set(to, new_dst);
        true
    }

    /// Shift-click: as many of `count` items (0: all) as fit into the list
    /// `to`. Returns false if nothing moved.
    pub fn move_somewhere(&mut self, from: (Place, usize), to: Place, count: u16) -> bool {
        let Some(Some(src)) = self.get(from) else {
            return false;
        };
        if from.0 == to || !Self::may_put(to, &src) {
            return false;
        }
        let Some(moving) = self.take(from, count) else {
            return false;
        };
        let left = match self.slots(to) {
            Some(list) => inventory::add_to(list, moving),
            None => moving.count,
        };
        if left > 0 {
            let back = match self.get(from).flatten() {
                Some(s) => Stack { count: s.count + left, ..s },
                None => Stack { count: left, ..moving },
            };
            self.set(from, Some(back));
        }
        left < moving.count
    }

    /// Take up to `count` items (0: all) out of a slot, as dropping does.
    pub fn take(&mut self, from: (Place, usize), count: u16) -> Option<Stack> {
        let stack = self.get(from)??;
        let n = if count == 0 { stack.count } else { count.min(stack.count) };
        let rest = (stack.count > n).then(|| Stack { count: stack.count - n, ..stack });
        self.set(from, rest);
        Some(Stack { count: n, ..stack })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::content::*;

    fn run(f: &mut Furnace, secs: u32) -> Vec<Step> {
        (0..secs).map(|_| f.step(1.0)).collect()
    }

    #[test]
    fn smelts_iron_with_coal() {
        let mut f = Furnace { running: true, ..Default::default() };
        f.src[0] = Some(Stack::new(ITEM_IRON_LUMP, 3));
        f.fuel[0] = Some(Stack::new(ITEM_COAL_LUMP, 1));
        let steps = run(&mut f, 10);
        // Lights at once, one ingot every 3 seconds.
        assert!(steps[0].active && steps[0].fire_sound && steps[0].changed);
        assert_eq!(f.fuel[0], None);
        assert_eq!(f.src[0], None);
        assert_eq!(f.dst[0], Some(Stack::new(ITEM_STEEL_INGOT, 3)));
        assert_eq!(steps.iter().map(|s| s.smelted).sum::<u32>(), 3);
        // Nothing left to smelt, but as in minetest_game the lump that is
        // alight burns on to the end (40 s), then the fire goes out.
        assert!(f.running && f.display.is_some());
        let rest = run(&mut f, 31);
        assert!(rest[29].active && !rest[30].active, "{:?}", rest.iter().map(|s| s.active).collect::<Vec<_>>());
        assert!(!f.running && f.display.is_none());
        assert_eq!(f.fuel_totaltime, 0.0);
    }

    #[test]
    fn runs_out_of_fuel() {
        let mut f = Furnace { running: true, ..Default::default() };
        f.src[0] = Some(Stack::new(CONTENT_COBBLE, 5));
        f.fuel[0] = Some(Stack::new(ITEM_STICK, 2)); // 1 s each
        let steps = run(&mut f, 5);
        assert!(steps[0].active);
        assert_eq!(f.fuel[0], None);
        assert_eq!(f.dst[0], None, "2 s of fuel can't make stone");
        assert!(!f.running);
        assert_eq!(f.src[0], Some(Stack::new(CONTENT_COBBLE, 5)));
    }

    #[test]
    fn progress_and_saving() {
        let mut f = Furnace { running: true, ..Default::default() };
        f.src[0] = Some(Stack::new(ITEM_IRON_LUMP, 1));
        f.fuel[0] = Some(Stack::new(CONTENT_BIRCH_WOOD, 1)); // 5 s
        f.step(1.0);
        assert_eq!(f.display, Some((80, 33)));
        let mut map = BTreeMap::new();
        map.insert((1, 2, 3), f.clone());
        let back = parse(&format(&map));
        let g = &back[&(1, 2, 3)];
        assert_eq!((g.fuel_time, g.fuel_totaltime, g.src_time, g.src), (f.fuel_time, f.fuel_totaltime, f.src_time, f.src));
    }

    #[test]
    fn locations() {
        assert_eq!(pos_of_location(&format!("detached:{}", detached_name((3, -2, 10)))), Some((3, -2, 10)));
        assert_eq!(pos_of_location("detached:esp_furnace_1_2"), None);
        assert_eq!(pos_of_location("current_player"), None);
    }
}
