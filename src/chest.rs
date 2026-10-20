//! Chests, as minetest_game's (mods/default/chests.lua): 32 slots in one
//! list, "main". A locked chest belongs to whoever placed it, and only they
//! can open or dig it.
//!
//! As with furnaces, the contents are kept by the server rather than in
//! node metadata, and shown to the player who opens the chest as a detached
//! inventory ("detached:esp_chest_<x>_<y>_<z>").
//!
//! Saved in one text file, rewritten when a chest's contents change:
//!   chest <x> <y> <z> [<owner>]
//!   <slot> <item string>               (one line per filled slot)

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::inventory::Stack;
use crate::storage;
use crate::world::NodePos;

pub const SLOTS: usize = 32;
const DETACHED_PREFIX: &str = "esp_chest_";

#[derive(Clone, Debug, PartialEq)]
pub struct Chest {
    pub main: [Option<Stack>; SLOTS],
    /// The owner of a locked chest.
    pub owner: Option<String>,
}

impl Default for Chest {
    fn default() -> Self {
        Self { main: [None; SLOTS], owner: None }
    }
}

impl Chest {
    /// minetest_game's can_dig: only an empty chest can be dug.
    pub fn is_empty(&self) -> bool {
        self.main.iter().all(Option::is_none)
    }

    /// Whether a player may open it, put in, take out or dig it
    /// (default.can_interact_with_node, without keys).
    pub fn allows(&self, player: &str) -> bool {
        self.owner.as_deref().map_or(true, |o| o.eq_ignore_ascii_case(player))
    }
}

/// The detached inventory name for the chest at `pos`.
pub fn detached_name(pos: NodePos) -> String {
    format!("{DETACHED_PREFIX}{}_{}_{}", pos.0, pos.1, pos.2)
}

/// The chest position in an inventory location such as
/// "detached:esp_chest_3_-2_10".
pub fn pos_of_location(location: &str) -> Option<NodePos> {
    let rest = location.strip_prefix("detached:")?.strip_prefix(DETACHED_PREFIX)?;
    let mut f = rest.split('_').map(|v| v.parse::<i32>().ok());
    let pos = (f.next()??, f.next()??, f.next()??);
    f.next().is_none().then_some(pos)
}

/// All the chests in the world, by position.
pub struct Chests {
    path: PathBuf,
    map: BTreeMap<NodePos, Chest>,
}

impl Chests {
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
            log::info!("{} chests", map.len());
        }
        Self { path, map }
    }

    pub fn get(&self, pos: NodePos) -> Option<&Chest> {
        self.map.get(&pos)
    }

    pub fn get_mut(&mut self, pos: NodePos) -> Option<&mut Chest> {
        self.map.get_mut(&pos)
    }

    /// The chest at `pos`, made empty if there wasn't one on record.
    pub fn get_or_add(&mut self, pos: NodePos) -> &mut Chest {
        self.map.entry(pos).or_default()
    }

    pub fn remove(&mut self, pos: NodePos) {
        if self.map.remove(&pos).is_some() {
            self.save();
        }
    }

    pub fn save(&self) {
        if let Err(e) = storage::write(&self.path, format(&self.map).as_bytes()) {
            warn!("Saving {}: {e}", self.path.display());
            crate::status::problem();
        }
    }
}

fn format(map: &BTreeMap<NodePos, Chest>) -> String {
    let mut t = String::new();
    for (pos, c) in map {
        t.push_str(&format!("chest {} {} {}", pos.0, pos.1, pos.2));
        if let Some(owner) = &c.owner {
            t.push_str(&format!(" {owner}"));
        }
        t.push('\n');
        for (i, s) in c.main.iter().enumerate() {
            if let Some(s) = s {
                t.push_str(&format!("{i} {}\n", s.item_string()));
            }
        }
    }
    t
}

fn parse(text: &str) -> BTreeMap<NodePos, Chest> {
    let mut map = BTreeMap::new();
    let mut current: Option<NodePos> = None;
    for line in text.lines() {
        let mut f = line.split_whitespace();
        match f.next() {
            Some("chest") => {
                let v: Vec<i32> = f.by_ref().take(3).filter_map(|v| v.parse().ok()).collect();
                current = (v.len() == 3).then(|| (v[0], v[1], v[2]));
                if let Some(pos) = current {
                    map.insert(pos, Chest { owner: f.next().map(str::to_string), ..Default::default() });
                }
            }
            Some(slot) => {
                let slot = slot.parse::<usize>().ok();
                let stack = Stack::parse(&f.collect::<Vec<_>>().join(" "));
                match (current.and_then(|p| map.get_mut(&p)), slot, stack) {
                    (Some(c), Some(slot), Some(stack)) if slot < SLOTS => c.main[slot] = Some(stack),
                    _ => warn!("Skipping chest line \"{line}\""),
                }
            }
            None => {}
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::*;

    #[test]
    fn saving_and_owners() {
        let mut map = BTreeMap::new();
        let mut c = Chest { owner: Some("Steve".into()), ..Default::default() };
        c.main[31] = Some(Stack::new(CONTENT_COBBLE, 99));
        map.insert((1, -2, 3), c.clone());
        map.insert((4, 5, 6), Chest::default());
        assert_eq!(parse(&format(&map)), map);
        assert!(c.allows("steve") && !c.allows("Jim"));
        assert!(Chest::default().allows("Jim"));
        assert_eq!(pos_of_location(&format!("detached:{}", detached_name((1, -2, 3)))), Some((1, -2, 3)));
    }
}
