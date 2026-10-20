//! Player inventories: the 32-slot "main" list (the first 8 slots are the
//! hotbar), saved on the SD card, and the inventory moves clients send.
//!
//! Only node items exist so far, so a stack is a content id and a count.
//!
//! File format (`<dir>/<lowercase name>.inv`), plain text so it can be
//! read and edited on a PC: one line per non-empty slot,
//!   <slot> <item name> <count>
//! plus a line `kit <n>` recording which starter kit the player has had.

use std::fs;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::content;
use crate::storage;

pub const MAIN_SIZE: usize = 32;
pub const STACK_MAX: u16 = 99;

/// The starter kit's version. Players whose saved kit number is lower get
/// it (once) when their inventory is loaded, so raising this hands a new
/// kit to existing players too.
const STARTER_KIT: u8 = 1;
const STARTER_TORCHES: u16 = 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    pub id: u16,
    pub count: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub main: [Option<Stack>; MAIN_SIZE],
    /// The last starter kit given (0: none yet).
    kit: u8,
}

impl Inventory {
    pub fn new() -> Self {
        Self { main: [None; MAIN_SIZE], kit: 0 }
    }

    /// Add items as picking up does in Luanti: top up existing stacks of
    /// the same item first, then use empty slots. Returns what didn't fit.
    pub fn add(&mut self, id: u16, mut count: u16) -> u16 {
        for slot in self.main.iter_mut().flatten() {
            if slot.id == id && slot.count < STACK_MAX {
                let n = count.min(STACK_MAX - slot.count);
                slot.count += n;
                count -= n;
            }
        }
        for slot in self.main.iter_mut() {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let n = count.min(STACK_MAX);
                *slot = Some(Stack { id, count: n });
                count -= n;
            }
        }
        count
    }

    /// Take one item from a slot, for placing. Returns its id.
    pub fn take_one(&mut self, slot: usize) -> Option<u16> {
        let s = self.main.get_mut(slot)?;
        let stack = s.as_mut()?;
        let id = stack.id;
        stack.count -= 1;
        if stack.count == 0 {
            *s = None;
        }
        Some(id)
    }

    /// Take up to `count` items from a slot (0: the whole stack), as
    /// IDropAction does.
    pub fn take(&mut self, slot: usize, count: u16) -> Option<Stack> {
        let s = self.main.get_mut(slot)?;
        let stack = s.as_mut()?;
        let n = if count == 0 { stack.count } else { count.min(stack.count) };
        let taken = Stack { id: stack.id, count: n };
        stack.count -= n;
        if stack.count == 0 {
            *s = None;
        }
        Some(taken)
    }

    pub fn get(&self, slot: usize) -> Option<Stack> {
        self.main.get(slot).copied().flatten()
    }

    /// IMoveAction within the main list: move `count` items (0 = the whole
    /// stack) from one slot to another. Into an empty slot or onto the same
    /// item, as many as fit move; onto a different item, the two stacks
    /// swap places. Returns false if the move is impossible.
    pub fn move_items(&mut self, from: usize, to: usize, count: u16) -> bool {
        if from >= MAIN_SIZE || to >= MAIN_SIZE || from == to {
            return false;
        }
        let Some(src) = self.main[from] else {
            return false;
        };
        let n = if count == 0 || count > src.count { src.count } else { count };
        match self.main[to] {
            None => {
                self.main[to] = Some(Stack { id: src.id, count: n });
            }
            Some(dst) if dst.id == src.id => {
                let n = n.min(STACK_MAX - dst.count);
                if n == 0 {
                    return false;
                }
                self.main[to] = Some(Stack { id: dst.id, count: dst.count + n });
                return self.reduce(from, n);
            }
            Some(dst) => {
                // Luanti swaps only when the whole moved amount can go; we
                // swap whole stacks, and only when the whole stack is moved.
                if n != src.count {
                    return false;
                }
                self.main[to] = Some(src);
                self.main[from] = Some(dst);
                return true;
            }
        }
        self.reduce(from, n)
    }

    fn reduce(&mut self, slot: usize, n: u16) -> bool {
        if let Some(s) = &mut self.main[slot] {
            s.count -= n;
            if s.count == 0 {
                self.main[slot] = None;
            }
        }
        true
    }

    /// The main list as (item name, count) per slot, for TOCLIENT_INVENTORY.
    pub fn slots(&self) -> Vec<Option<(&'static str, u16)>> {
        self.main
            .iter()
            .map(|s| s.map(|s| (content::name_of(s.id), s.count)))
            .collect()
    }

    /// Give the starter kit if this inventory hasn't had it. Returns true
    /// if anything changed.
    fn give_starter_kit(&mut self) -> bool {
        if self.kit >= STARTER_KIT {
            return false;
        }
        self.kit = STARTER_KIT;
        let lost = self.add(content::CONTENT_TORCH, STARTER_TORCHES);
        if lost > 0 {
            warn!("Starter kit: no room for {lost} torches");
        }
        true
    }

    fn to_text(&self) -> String {
        let mut t = format!("kit {}\n", self.kit);
        for (i, s) in self.main.iter().enumerate() {
            if let Some(s) = s {
                t.push_str(&format!("{i} {} {}\n", content::name_of(s.id), s.count));
            }
        }
        t
    }

    fn from_text(text: &str) -> Self {
        let mut inv = Self::new();
        for line in text.lines() {
            let mut f = line.split_whitespace();
            if let (Some("kit"), Some(Ok(k)), None) = (f.next(), f.next().map(str::parse::<u8>), f.next()) {
                inv.kit = k;
                continue;
            }
            let mut f = line.split_whitespace();
            let (Some(slot), Some(name), Some(count)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            let (Ok(slot), Ok(count)) = (slot.parse::<usize>(), count.parse::<u16>()) else {
                continue;
            };
            match content::id_of(name) {
                Some(id) if slot < MAIN_SIZE && count > 0 && content::is_item(id) => {
                    inv.main[slot] = Some(Stack { id, count: count.min(STACK_MAX) });
                }
                _ => warn!("Skipping inventory line \"{line}\""),
            }
        }
        inv
    }
}

/// An inventory move or other action from TOSERVER_INVENTORY_ACTION, in
/// Luanti's text form (InventoryAction::deSerialize).
#[derive(Debug, PartialEq)]
pub enum Action {
    /// "Move <count> <from_inv> <from_list> <from_i> <to_inv> <to_list> <to_i>"
    Move {
        count: u16,
        from: (String, String, usize),
        to: (String, String, usize),
    },
    /// "Drop <count> <from_inv> <from_list> <from_i>" (count 0: all)
    Drop { count: u16, from: (String, String, usize) },
    /// MoveSomewhere or Craft, which we don't support yet.
    Other(String),
}

pub fn parse_action(text: &str) -> Option<Action> {
    let mut f = text.split(' ');
    let kind = f.next()?;
    if kind != "Move" && kind != "Drop" {
        return Some(Action::Other(kind.to_string()));
    }
    let count = f.next()?.parse().ok()?;
    let from = (f.next()?.to_string(), f.next()?.to_string(), f.next()?.parse().ok()?);
    if kind == "Drop" {
        return Some(Action::Drop { count, from });
    }
    let to = (f.next()?.to_string(), f.next()?.to_string(), f.next()?.parse().ok()?);
    Some(Action::Move { count, from, to })
}

/// Whether an inventory location in an action means this player's own.
pub fn is_own(location: &str, player: &str) -> bool {
    location == "current_player"
        || location.strip_prefix("player:").is_some_and(|n| n.eq_ignore_ascii_case(player))
}

pub struct InventoryStore {
    dir: PathBuf,
}

impl InventoryStore {
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.inv", name.to_ascii_lowercase()))
    }

    /// A player's saved inventory, or an empty one for a new player, with
    /// the starter kit added (and saved) if they haven't had it yet.
    pub fn load(&self, name: &str) -> Inventory {
        let mut inv = self.read(name);
        if inv.give_starter_kit() {
            log::info!("Gave {name} the starter kit");
            self.save(name, &inv);
        }
        inv
    }

    fn read(&self, name: &str) -> Inventory {
        match storage::read(&self.path(name)) {
            Ok(t) => Inventory::from_text(&String::from_utf8_lossy(&t)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Inventory::new(),
            Err(e) => {
                warn!("Reading inventory of {name}: {e}");
                Inventory::new()
            }
        }
    }

    pub fn save(&self, name: &str, inv: &Inventory) {
        if let Err(e) = storage::write(&self.path(name), inv.to_text().as_bytes()) {
            warn!("Saving inventory of {name}: {e}");
        }
    }
}
