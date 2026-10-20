//! Player inventories: the 32-slot "main" list (the first 8 slots are the
//! hotbar), the 3x3 craft grid and the craft result slot, saved on the SD
//! card, and the inventory actions clients send.
//!
//! File format (`<dir>/<lowercase name>.inv`), plain text so it can be
//! read and edited on a PC: one line per non-empty slot,
//!   <slot> <item name> <count>              (main)
//!   craft <slot> <item name> <count>
//!   craftresult 0 <item name> <count>
//! plus a line `kit <n>` recording which starter kit the player has had.

use std::fs;
use std::io;
use std::path::PathBuf;

use log::warn;

use crate::content;
use crate::crafting::{self, GRID_SIZE, GRID_WIDTH};
use crate::storage;

pub const MAIN_SIZE: usize = 32;

/// The starter kit's version. Players whose saved kit number is lower get
/// it (once) when their inventory is loaded, so raising this hands a new
/// kit to existing players too.
const STARTER_KIT: u8 = 1;
const STARTER_TORCHES: u16 = 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    pub id: u16,
    pub count: u16,
    /// A tool's wear, 0 (new) up to 65535; it breaks at 65536.
    pub wear: u16,
}

impl Stack {
    pub fn new(id: u16, count: u16) -> Self {
        Self { id, count, wear: 0 }
    }

    /// Luanti's item string: "<name> <count>", with the wear after it for
    /// a worn tool.
    pub fn item_string(&self) -> String {
        match self.wear {
            0 => format!("{} {}", content::name_of(self.id), self.count),
            w => format!("{} {} {w}", content::name_of(self.id), self.count),
        }
    }

    /// Parse "<name> [<count> [<wear>]]" (ItemStack::deSerialize, without
    /// metadata).
    pub fn parse(text: &str) -> Option<Self> {
        let mut f = text.split_whitespace();
        let id = content::id_of(f.next()?).filter(|&id| content::is_item(id))?;
        let count = f.next().map_or(Some(1), |c| c.parse::<u16>().ok())?;
        let wear = f.next().map_or(Some(0), |w| w.parse::<u16>().ok())?;
        (count > 0).then(|| Self { id, count: count.min(content::stack_max(id)), wear })
    }

    /// Whether another stack can be put onto this one (same item, same
    /// wear: ItemStack::stacksWith).
    fn stacks_with(&self, other: &Stack) -> bool {
        self.id == other.id && self.wear == other.wear
    }

    /// Wear a tool by `amount`. Returns false if that breaks it.
    pub fn add_wear(&mut self, amount: u32) -> bool {
        let wear = self.wear as u32 + amount;
        if wear >= 65536 {
            return false;
        }
        self.wear = wear as u16;
        true
    }
}

/// The lists every Luanti player has. "craftpreview" isn't stored: it
/// shows what the craft grid makes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum List {
    Main,
    Craft,
    /// Where crafted items go; the client picks them up from here.
    CraftResult,
}

impl List {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "main" => Some(Self::Main),
            "craft" => Some(Self::Craft),
            "craftresult" => Some(Self::CraftResult),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub main: [Option<Stack>; MAIN_SIZE],
    pub craft: [Option<Stack>; GRID_SIZE],
    pub craft_result: [Option<Stack>; 1],
    /// The last starter kit given (0: none yet).
    kit: u8,
}

impl Inventory {
    pub fn new() -> Self {
        Self { main: [None; MAIN_SIZE], craft: [None; GRID_SIZE], craft_result: [None], kit: 0 }
    }

    pub fn list(&self, list: List) -> &[Option<Stack>] {
        match list {
            List::Main => &self.main,
            List::Craft => &self.craft,
            List::CraftResult => &self.craft_result,
        }
    }

    pub fn list_mut(&mut self, list: List) -> &mut [Option<Stack>] {
        match list {
            List::Main => &mut self.main,
            List::Craft => &mut self.craft,
            List::CraftResult => &mut self.craft_result,
        }
    }

    /// Add items as picking up does in Luanti: top up existing stacks of
    /// the same item first, then use empty slots. Returns what didn't fit.
    pub fn add(&mut self, stack: Stack) -> u16 {
        add_to(&mut self.main, stack)
    }

    /// Replace (or empty) a main slot, as wearing out a tool does.
    pub fn set(&mut self, slot: usize, stack: Option<Stack>) {
        if let Some(s) = self.main.get_mut(slot) {
            *s = stack;
        }
    }

    /// Take one item from a slot, for placing. Returns its id.
    pub fn take_one(&mut self, slot: usize) -> Option<u16> {
        self.take(List::Main, slot, 1).map(|s| s.id)
    }

    /// Take up to `count` items from a slot (0: the whole stack), as
    /// IDropAction does.
    pub fn take(&mut self, list: List, slot: usize, count: u16) -> Option<Stack> {
        let s = self.list_mut(list).get_mut(slot)?;
        let stack = s.as_mut()?;
        let n = if count == 0 { stack.count } else { count.min(stack.count) };
        let taken = Stack { count: n, ..*stack };
        stack.count -= n;
        if stack.count == 0 {
            *s = None;
        }
        Some(taken)
    }

    pub fn get(&self, slot: usize) -> Option<Stack> {
        self.main.get(slot).copied().flatten()
    }

    /// What the craft grid makes, for the craftpreview slot.
    pub fn craft_preview(&self) -> Option<Stack> {
        crafting::result(&self.craft).map(|(id, count)| Stack::new(id, count))
    }

    /// ICraftAction: craft up to `times` times (0: as often as possible)
    /// into the craft result slot, while the result fits there. Returns
    /// what was made in total.
    pub fn craft(&mut self, times: u16) -> Option<Stack> {
        let mut made: Option<Stack> = None;
        let mut left = times;
        while let Some(next) = self.craft_preview() {
            let room = match self.craft_result[0] {
                None => true,
                Some(r) => r.stacks_with(&next) && r.count + next.count <= content::stack_max(r.id),
            };
            if !room {
                break;
            }
            crafting::craft(&mut self.craft);
            let slot = &mut self.craft_result[0];
            match slot {
                Some(r) => r.count += next.count,
                None => *slot = Some(next),
            }
            match &mut made {
                Some(m) => m.count += next.count,
                None => made = Some(next),
            }
            if left == 1 {
                break;
            }
            left = left.saturating_sub(1);
        }
        made
    }

    /// The lists as Luanti sends them: (name, width, slots).
    pub fn lists(&self) -> Vec<(&'static str, u32, Vec<Option<Stack>>)> {
        vec![
            ("main", 0, self.main.to_vec()),
            ("craft", GRID_WIDTH as u32, self.craft.to_vec()),
            ("craftpreview", 0, vec![self.craft_preview()]),
            ("craftresult", 0, self.craft_result.to_vec()),
        ]
    }

    /// Give the starter kit if this inventory hasn't had it. Returns true
    /// if anything changed.
    fn give_starter_kit(&mut self) -> bool {
        if self.kit >= STARTER_KIT {
            return false;
        }
        self.kit = STARTER_KIT;
        let lost = self.add(Stack::new(content::CONTENT_TORCH, STARTER_TORCHES));
        if lost > 0 {
            warn!("Starter kit: no room for {lost} torches");
        }
        true
    }

    fn to_text(&self) -> String {
        let mut t = format!("kit {}\n", self.kit);
        for (i, s) in self.main.iter().enumerate() {
            if let Some(s) = s {
                t.push_str(&format!("{i} {}\n", s.item_string()));
            }
        }
        for (i, s) in self.craft.iter().enumerate() {
            if let Some(s) = s {
                t.push_str(&format!("craft {i} {}\n", s.item_string()));
            }
        }
        if let Some(s) = self.craft_result[0] {
            t.push_str(&format!("craftresult 0 {}\n", s.item_string()));
        }
        t
    }

    fn from_text(text: &str) -> Self {
        let mut inv = Self::new();
        for line in text.lines() {
            let mut f = line.split_whitespace().peekable();
            let list = match f.peek() {
                Some(&"kit") => {
                    f.next();
                    if let (Some(Ok(k)), None) = (f.next().map(str::parse::<u8>), f.next()) {
                        inv.kit = k;
                    }
                    continue;
                }
                Some(&"craft") => List::Craft,
                Some(&"craftresult") => List::CraftResult,
                _ => List::Main,
            };
            if list != List::Main {
                f.next();
            }
            let slot = f.next().and_then(|s| s.parse::<usize>().ok());
            let stack = Stack::parse(&f.collect::<Vec<_>>().join(" "));
            match (slot, stack) {
                (Some(slot), Some(stack)) if slot < inv.list(list).len() => inv.list_mut(list)[slot] = Some(stack),
                _ => warn!("Skipping inventory line \"{line}\""),
            }
        }
        inv
    }
}

/// IMoveAction on the contents of two slots (in any lists): what they hold
/// afterwards, or None if nothing can move. `count` items (0: all) move
/// into an empty slot or onto the same item, as many as fit; onto a
/// different item the two stacks swap places, but only when the whole
/// stack is moved (Luanti swaps when the whole moved amount can go).
pub fn plan_move(src: Option<Stack>, dst: Option<Stack>, count: u16) -> Option<(Option<Stack>, Option<Stack>)> {
    let src = src?;
    let n = if count == 0 || count > src.count { src.count } else { count };
    let left = |moved: u16| (src.count > moved).then(|| Stack { count: src.count - moved, ..src });
    match dst {
        None => Some((left(n), Some(Stack { count: n, ..src }))),
        Some(d) if d.stacks_with(&src) => {
            let n = n.min(content::stack_max(d.id).saturating_sub(d.count));
            (n > 0).then(|| (left(n), Some(Stack { count: d.count + n, ..d })))
        }
        Some(d) => (n == src.count).then_some((Some(d), Some(src))),
    }
}

/// Add a stack to a list: onto stacks of the same item first, then into
/// empty slots. Returns how many didn't fit.
pub fn add_to(list: &mut [Option<Stack>], stack: Stack) -> u16 {
    let max = content::stack_max(stack.id);
    let mut count = stack.count;
    for slot in list.iter_mut().flatten() {
        if slot.stacks_with(&stack) && slot.count < max {
            let n = count.min(max - slot.count);
            slot.count += n;
            count -= n;
        }
    }
    for slot in list.iter_mut() {
        if count == 0 {
            break;
        }
        if slot.is_none() {
            let n = count.min(max);
            *slot = Some(Stack { count: n, ..stack });
            count -= n;
        }
    }
    count
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
    /// "MoveSomewhere <count> <from_inv> <from_list> <from_i> <to_inv> <to_list>"
    /// (shift-click: into whichever slots of the list it fits)
    MoveSomewhere {
        count: u16,
        from: (String, String, usize),
        to: (String, String),
    },
    /// "Drop <count> <from_inv> <from_list> <from_i>" (count 0: all)
    Drop { count: u16, from: (String, String, usize) },
    /// "Craft <count> <inv>": clicking the craft preview (count 0: as
    /// many as possible).
    Craft { count: u16, inv: String },
    Other(String),
}

pub fn parse_action(text: &str) -> Option<Action> {
    let mut f = text.split(' ');
    let kind = f.next()?;
    if !matches!(kind, "Move" | "MoveSomewhere" | "Drop" | "Craft") {
        return Some(Action::Other(kind.to_string()));
    }
    let count = f.next()?.parse().ok()?;
    if kind == "Craft" {
        return Some(Action::Craft { count, inv: f.next()?.to_string() });
    }
    let from = (f.next()?.to_string(), f.next()?.to_string(), f.next()?.parse().ok()?);
    if kind == "Drop" {
        return Some(Action::Drop { count, from });
    }
    let to_inv = f.next()?.to_string();
    let to_list = f.next()?.to_string();
    if kind == "MoveSomewhere" {
        return Some(Action::MoveSomewhere { count, from, to: (to_inv, to_list) });
    }
    let to = (to_inv, to_list, f.next()?.parse().ok()?);
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
            crate::status::problem();
        }
    }
}
