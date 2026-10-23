//! Chests, as minetest_game's (mods/default/chests.lua): 32 slots in one
//! list, "main". A locked chest belongs to whoever placed it, and only they
//! can open or dig it.
//!
//! As with furnaces, the contents are kept by the server rather than in
//! node metadata, and shown to the player who opens the chest as a detached
//! inventory ("detached:esp_chest_<x>_<y>_<z>").
//!
//! Bones (bones.rs) are kept here too, as chests nobody can put into, and
//! bookshelves and vessels shelves, as chests of 16 slots that take only
//! books, or only vessels.
//!
//! Saved in one text file, rewritten when a chest's contents change:
//!   chest <x> <y> <z> [<owner>]         (or "bones ...", "shelf ...",
//!                                        "vessels ...")
//!   share <game time>                  (bones: when anyone may take)
//!   secret <hex>                       (a locked chest's key, keys.rs)
//!   <slot> <item string>               (one line per filled slot)

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::inventory::Stack;
use crate::storage;
use crate::world::NodePos;

pub const SLOTS: usize = 32;
/// A shelf's slots (minetest_game's 8 x 2 "books" or "vessels" list), the
/// first of its record's.
pub const SHELF_SLOTS: usize = 16;

/// The two kinds of shelf: minetest_game's bookshelf, and vessels' shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelf {
    Books,
    Vessels,
}

impl Shelf {
    /// The item group it takes.
    pub fn group(self) -> &'static str {
        match self {
            Shelf::Books => "book",
            Shelf::Vessels => "vessel",
        }
    }
}
const DETACHED_PREFIX: &str = "esp_chest_";

#[derive(Clone, Debug, PartialEq)]
pub struct Chest {
    pub main: [Option<Stack>; SLOTS],
    /// The owner of a locked chest, or of fresh bones.
    pub owner: Option<String>,
    /// Bones (bones.rs) rather than a chest.
    pub bones: bool,
    /// Fresh bones: the game time at which they become everyone's.
    pub share_at: Option<u64>,
    /// A shelf rather than a chest.
    pub shelf: Option<Shelf>,
    /// The secret of the keys to a locked chest (keys.rs), once its owner
    /// has made one.
    pub secret: Option<String>,
}

impl Default for Chest {
    fn default() -> Self {
        Self { main: [None; SLOTS], owner: None, bones: false, share_at: None, shelf: None, secret: None }
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

    /// The same, for a player wielding a key with `key`'s secret (or
    /// nothing): the right key opens a locked chest too.
    pub fn allows_with(&self, player: &str, key: Option<&str>) -> bool {
        self.allows(player) || crate::keys::fits(self.secret.as_deref(), key)
    }

    /// Its slots: a shelf's 16, or all 32.
    pub fn slots(&self) -> &[Option<Stack>] {
        &self.main[..if self.shelf.is_some() { SHELF_SLOTS } else { SLOTS }]
    }

    pub fn slots_mut(&mut self) -> &mut [Option<Stack>] {
        &mut self.main[..if self.shelf.is_some() { SHELF_SLOTS } else { SLOTS }]
    }

    /// Whether a stack may be put in (allow_metadata_inventory_put):
    /// nothing into bones, only books onto a bookshelf, only vessels onto
    /// a vessels shelf.
    pub fn may_put(&self, stack: &Stack) -> bool {
        !self.bones && self.shelf.map_or(true, |s| crate::content::group_of(stack.id, s.group()) > 0)
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

    /// Bones that have lain long enough become everyone's (bones' node
    /// timer). Returns the owners whose bones did.
    pub fn share_old_bones(&mut self, now: u64) -> Vec<String> {
        let mut shared = Vec::new();
        for c in self.map.values_mut().filter(|c| c.bones && c.share_at.is_some_and(|t| t <= now)) {
            c.share_at = None;
            shared.extend(c.owner.take());
        }
        if !shared.is_empty() {
            self.save();
        }
        shared
    }

    /// Put a record in (new bones).
    pub fn insert(&mut self, pos: NodePos, chest: Chest) {
        self.map.insert(pos, chest);
        self.save();
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
        let kind = if c.bones {
            "bones"
        } else if c.shelf == Some(Shelf::Books) {
            "shelf"
        } else if c.shelf == Some(Shelf::Vessels) {
            "vessels"
        } else {
            "chest"
        };
        t.push_str(&format!("{kind} {} {} {}", pos.0, pos.1, pos.2));
        if let Some(owner) = &c.owner {
            t.push_str(&format!(" {owner}"));
        }
        t.push('\n');
        if let Some(at) = c.share_at {
            t.push_str(&format!("share {at}\n"));
        }
        if let Some(secret) = &c.secret {
            t.push_str(&format!("secret {secret}\n"));
        }
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
            Some(kind @ ("chest" | "bones" | "shelf" | "vessels")) => {
                let v: Vec<i32> = f.by_ref().take(3).filter_map(|v| v.parse().ok()).collect();
                current = (v.len() == 3).then(|| (v[0], v[1], v[2]));
                if let Some(pos) = current {
                    let bones = kind == "bones";
                    let shelf = match kind {
                        "shelf" => Some(Shelf::Books),
                        "vessels" => Some(Shelf::Vessels),
                        _ => None,
                    };
                    map.insert(pos, Chest { owner: f.next().map(str::to_string), bones, shelf, ..Default::default() });
                }
            }
            Some("secret") => match (current.and_then(|p| map.get_mut(&p)), f.next()) {
                (Some(c), Some(secret)) => c.secret = Some(secret.to_string()),
                _ => warn!("Skipping chest line \"{line}\""),
            },
            Some("share") => match (current.and_then(|p| map.get_mut(&p)), f.next().and_then(|t| t.parse().ok())) {
                (Some(c), Some(t)) => c.share_at = Some(t),
                _ => warn!("Skipping chest line \"{line}\""),
            },
            Some(slot) => {
                let slot = slot.parse::<usize>().ok();
                // The item string as written (its metadata may hold spaces).
                let stack = line.trim_start().split_once(' ').and_then(|(_, item)| Stack::parse(item));
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
        let mut c = Chest { owner: Some("Steve".into()), secret: Some("0123456789abcdef".into()), ..Default::default() };
        c.main[31] = Some(Stack::new(CONTENT_COBBLE, 99));
        map.insert((1, -2, 3), c.clone());
        map.insert((4, 5, 6), Chest::default());
        let mut b = Chest { owner: Some("Jim".into()), bones: true, share_at: Some(1500), ..Default::default() };
        b.main[0] = Some(Stack::new(CONTENT_DIRT, 3));
        map.insert((7, 8, 9), b);
        let mut v = Chest { shelf: Some(Shelf::Vessels), ..Default::default() };
        v.main[3] = Some(Stack::new(VESSELS_GLASS_BOTTLE, 2));
        map.insert((0, 1, 0), v.clone());
        map.insert((0, 2, 0), Chest { shelf: Some(Shelf::Books), ..Default::default() });
        assert_eq!(parse(&format(&map)), map);
        assert!(c.allows("steve") && !c.allows("Jim"));
        assert!(c.allows_with("Jim", Some("0123456789abcdef")) && !c.allows_with("Jim", Some("0123456789abcdee")) && !c.allows_with("Jim", None));
        assert!(!Chest::default().allows_with("Jim", None) || Chest::default().allows("Jim"));
        assert!(v.may_put(&Stack::new(VESSELS_STEEL_BOTTLE, 1)) && !v.may_put(&Stack::new(ITEM_BOOK, 1)) && v.slots().len() == SHELF_SLOTS);
        assert!(Chest::default().allows("Jim"));
        assert_eq!(pos_of_location(&format!("detached:{}", detached_name((1, -2, 3)))), Some((1, -2, 3)));
    }

    #[test]
    fn bones_become_everyones() {
        let path = std::env::temp_dir().join(format!("esp-bones-test-{}.txt", std::process::id()));
        let mut chests = Chests::open(&path);
        chests.insert((0, 0, 0), Chest { owner: Some("Jim".into()), bones: true, share_at: Some(1200), ..Default::default() });
        chests.insert((1, 0, 0), Chest { owner: Some("Ann".into()), ..Default::default() });
        assert!(chests.share_old_bones(1199).is_empty());
        assert_eq!(chests.share_old_bones(1200), vec!["Jim".to_string()]);
        let reread = Chests::open(&path);
        assert!(reread.get((0, 0, 0)).is_some_and(|c| c.bones && c.owner.is_none() && c.allows("anyone")));
        assert_eq!(reread.get((1, 0, 0)).and_then(|c| c.owner.clone()), Some("Ann".to_string()), "a locked chest stays locked");
        let _ = std::fs::remove_file(&path);
    }
}
