// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Keir Finlow-Bates
// Based on minetest_game's mtg_craftguide mod (mods/mtg_craftguide/init.lua):
//   Copyright (C) 2015-2019 Jean-Patrick Guerrero and contributors
//   Copyright (C) 2020 pauloue
// Licensed under the MIT License: permission is hereby granted, free of
// charge, to any person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy,
// modify, merge, publish, distribute, sublicense, and/or sell copies of the
// Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions: the above copyright notice and
// this permission notice shall be included in all copies or substantial
// portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT
// WARRANTY OF ANY KIND, EXPRESS OR IMPLIED.

//! The crafting guide: the inventory window's "Recipes" tab, as
//! minetest_game's mtg_craftguide has it. Every item with a recipe or a use
//! in one, a page at a time, with a search box; clicking one shows how to
//! make it (each of its recipes in turn), and clicking it again what it is
//! used in.
//!
//! The recipes aren't copied into a table of their own (the board has
//! little memory to spare): each is found again in crafting.rs's tables
//! when it is shown. Only the sorted list of items is kept, made the first
//! time anyone opens the tab.

use std::sync::OnceLock;

use crate::content::{self, *};
use crate::crafting::{self, Ingredient, GRID_SIZE};
use crate::messages::formspec_escape as esc;

/// The inventory window's tabs (sfinv's tab header): "1" Crafting, "2"
/// Recipes.
pub const TABS: &str = "sfinv_nav_tabs";

/// Items on a page: 8 columns of 6 (mtg_craftguide shows 4 rows in its
/// smaller window).
const COLUMNS: usize = 8;
const ROWS: usize = 6;
const PER_PAGE: usize = COLUMNS * ROWS;

/// One player's view of the guide (mtg_craftguide's player_data).
#[derive(Default)]
pub struct Guide {
    /// Whether the Recipes tab is the one showing.
    pub open: bool,
    filter: String,
    /// From 0.
    page: usize,
    /// The item last clicked, and whether its usages are showing rather
    /// than its recipes.
    item: Option<u16>,
    usages: bool,
    /// Which of its recipes, from 0.
    rnum: usize,
}

/// Where a recipe is: an index into crafting::RECIPES, SHAPELESS or
/// stairs::recipes(), or a cooking recipe by what goes into the furnace.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    Shaped(u16),
    Shapeless(u16),
    Stair(u16),
    Cook(u16),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Method {
    /// A shaped recipe this many cells wide.
    Normal(usize),
    Shapeless,
    /// Cooking, taking this many seconds.
    Cooking(f32),
}

/// A recipe as shown: its cells (Empty past the end), how it is made, and
/// what it makes.
struct View {
    method: Method,
    cells: [Ingredient; GRID_SIZE],
    len: usize,
    output: u16,
    count: u16,
}

/// A recipe to show, and for a usage the group in it that stands for the
/// clicked item (shown as that item, as mtg_craftguide's cache_usages
/// does).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Shown {
    source: Source,
    standing_for: Option<(&'static str, u16)>,
}

/// Every item a player can hold: nodes, craftitems and tools.
fn all_items() -> impl Iterator<Item = u16> {
    NODES.iter().map(|n| n.id).chain(CRAFTITEMS.iter().map(|c| c.id)).chain(TOOLS.iter().map(|t| t.id))
}

/// mtg_craftguide's show_item: anything with a description.
fn shown(id: u16) -> bool {
    !content::description_of(id).is_empty() && content::group_of(id, "not_in_craft_guide") == 0
}

/// Every recipe, in the order the guide lists them: shaped, shapeless,
/// stairs, then cooking. Of the cooking recipes, minetest_game's one for
/// "group:sand" (glass) is one recipe here too.
fn sources() -> impl Iterator<Item = Source> {
    let shaped = (0..crafting::RECIPES.len()).map(|i| Source::Shaped(i as u16));
    let shapeless = (0..crafting::SHAPELESS.len()).map(|i| Source::Shapeless(i as u16));
    let stairs = (0..crate::stairs::recipes().count()).map(|i| Source::Stair(i as u16));
    let mut sand = false;
    let cooking = all_items().filter(move |&id| {
        if crafting::cook_result(id).is_none() {
            return false;
        }
        if content::group_of(id, "sand") > 0 {
            return !std::mem::replace(&mut sand, true);
        }
        true
    });
    shaped.chain(shapeless).chain(stairs).chain(cooking.map(Source::Cook))
}

fn view(source: Source) -> View {
    let mut cells = [Ingredient::Empty; GRID_SIZE];
    let (method, len, output, count) = match source {
        Source::Shaped(i) => {
            let r = &crafting::RECIPES[i as usize];
            cells[..r.pattern.len()].copy_from_slice(r.pattern);
            (Method::Normal(r.width), r.pattern.len(), r.output, r.count)
        }
        Source::Shapeless(i) => {
            let r = &crafting::SHAPELESS[i as usize];
            cells[..r.items.len()].copy_from_slice(r.items);
            (Method::Shapeless, r.items.len(), r.output, r.count)
        }
        Source::Stair(i) => {
            let (width, pattern, item, output, count) = crate::stairs::recipes().nth(i as usize).expect("a stair recipe");
            for (cell, &p) in cells.iter_mut().zip(pattern.iter()) {
                *cell = if p == 0 { Ingredient::Empty } else { Ingredient::Item(item) };
            }
            // Only the rows the pattern fills.
            let len = pattern.iter().rposition(|&p| p != 0).map_or(0, |last| (last / width + 1) * width);
            (Method::Normal(width), len, output, count)
        }
        Source::Cook(id) => {
            let (output, time) = crafting::cook_result(id).expect("a cooking recipe");
            cells[0] = if content::group_of(id, "sand") > 0 { Ingredient::Group("sand") } else { Ingredient::Item(id) };
            (Method::Cooking(time), 1, output, 1)
        }
    };
    View { method, cells, len, output, count }
}

/// Whether an item goes into a recipe cell.
fn fits(cell: Ingredient, id: u16) -> bool {
    match cell {
        Ingredient::Empty => false,
        Ingredient::Item(i) => i == id,
        Ingredient::Group(g) => content::group_of(id, g) > 0,
    }
}

/// The recipes making an item (mtg_craftguide's recipes_cache).
fn recipes_for(id: u16) -> Vec<Shown> {
    if !shown(id) {
        return Vec::new();
    }
    sources().filter(|&s| view(s).output == id).map(|source| Shown { source, standing_for: None }).collect()
}

/// The recipes an item goes into (usages_cache): those naming it, and
/// those taking any of a group it is in, with the group shown as it.
fn usages_for(id: u16) -> Vec<Shown> {
    if !shown(id) {
        return Vec::new();
    }
    sources()
        .filter_map(|source| {
            let v = view(source);
            if !shown(v.output) {
                return None;
            }
            let cells = &v.cells[..v.len];
            if cells.iter().any(|c| matches!(c, Ingredient::Item(i) if *i == id)) {
                return Some(Shown { source, standing_for: None });
            }
            cells.iter().find_map(|c| match c {
                Ingredient::Group(g) if fits(*c, id) => Some(Shown { source, standing_for: Some((*g, id)) }),
                _ => None,
            })
        })
        .collect()
}

/// Every item with a recipe or a usage, sorted by name (init_items).
fn listed() -> &'static [u16] {
    static LISTED: OnceLock<Vec<u16>> = OnceLock::new();
    LISTED.get_or_init(|| {
        let mut ids: Vec<u16> = Vec::new();
        let groups: Vec<&'static str> = {
            let mut g = Vec::new();
            for s in sources() {
                let v = view(s);
                if !shown(v.output) {
                    continue;
                }
                ids.push(v.output);
                for c in &v.cells[..v.len] {
                    match *c {
                        Ingredient::Item(i) => ids.push(i),
                        Ingredient::Group(name) if !g.contains(&name) => g.push(name),
                        _ => {}
                    }
                }
            }
            g
        };
        ids.extend(all_items().filter(|&id| groups.iter().any(|g| content::group_of(id, g) > 0)));
        ids.retain(|&id| shown(id));
        ids.sort_unstable_by_key(|&id| content::name_of(id));
        ids.dedup();
        ids.shrink_to_fit();
        ids
    })
}

/// The listed items matching the search (execute_search): by name or
/// description, ignoring case.
fn items(filter: &str) -> Vec<u16> {
    listed()
        .iter()
        .copied()
        .filter(|&id| {
            filter.is_empty()
                || content::name_of(id).to_lowercase().contains(filter)
                || content::description_of(id).to_lowercase().contains(filter)
        })
        .collect()
}

/// The item a group is shown as (groups_to_item): mtg_craftguide's
/// stereotypes, else the item named after the group, else the first item
/// in it.
fn stereotype(group: &str) -> u16 {
    let named = match group {
        "wood" => Some(CONTENT_APPLE_WOOD),
        "stone" => Some(CONTENT_STONE),
        "stick" => Some(ITEM_STICK),
        "sand" => Some(CONTENT_SAND),
        "coal" => Some(ITEM_COAL_LUMP),
        "vessel" => Some(VESSELS_GLASS_BOTTLE),
        "wool" => Some(crate::farming::WOOL_WHITE),
        _ => None,
    };
    named
        .or_else(|| content::id_of(&format!("esp:{group}")))
        .or_else(|| all_items().find(|&id| content::group_of(id, group) > 0))
        .unwrap_or(ITEM_STICK)
}

fn group_name(group: &str) -> String {
    match group {
        "coal" => "Any coal".into(),
        "sand" => "Any sand".into(),
        "wool" => "Any wool".into(),
        "stick" => "Any stick".into(),
        "vessel" => "Any vessel".into(),
        "wood" => "Any wood planks".into(),
        "stone" => "Any kind of stone block".into(),
        g => format!("Any item belonging to the group(s): {}", colorize("yellow", g)),
    }
}

/// A coordinate, to two places at most ("11.7", not "11.700001").
fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// minetest.colorize.
fn colorize(colour: &str, text: &str) -> String {
    format!("\x1b(c@{colour}){text}\x1b(c@#ffffff)")
}

/// What a click on the Recipes tab did (on_receive_fields): true when the
/// window must be sent again.
pub fn on_fields(g: &mut Guide, fields: &[(String, String)]) -> bool {
    let value = |name: &str| fields.iter().find(|(f, _)| f == name).map(|(_, v)| v.as_str());
    let pressed = |name: &str| value(name).is_some();
    if pressed("clear") {
        g.filter.clear();
        g.page = 0;
        g.item = None;
        return true;
    }
    if let (true, Some(text)) = (value("key_enter_field") == Some("filter") || pressed("search"), value("filter")) {
        let new: String = text
            .chars()
            .take(128)
            .filter(|&c| !c.is_control() || c == '\t' || c == '\n')
            .collect::<String>()
            .to_lowercase();
        if new == g.filter {
            return false;
        }
        g.filter = new;
        g.page = 0;
        return true;
    }
    if pressed("prev") || pressed("next") {
        let pages = pages(items(&g.filter).len());
        if pages == 1 {
            return false;
        }
        g.page = if pressed("next") { (g.page + 1) % pages } else { (g.page + pages - 1) % pages };
        return true;
    }
    if pressed("recipe_prev") || pressed("recipe_next") {
        let n = g.item.map_or(0, |id| shown_for(id, g.usages).len());
        if n == 0 {
            return false;
        }
        g.rnum = if pressed("recipe_next") { (g.rnum + 1) % n } else { (g.rnum + n - 1) % n };
        return true;
    }
    // An item: "esp:stone", or a group shown as an item,
    // "esp:apple_wood.wood".
    let Some(name) = fields.iter().find(|(f, _)| f.contains(':')).map(|(f, _)| f.split('.').next().unwrap_or("")) else {
        return false;
    };
    let Some(id) = content::id_of(name) else {
        return false;
    };
    g.usages = g.item == Some(id) && !g.usages;
    g.item = Some(id);
    g.rnum = 0;
    true
}

fn shown_for(id: u16, usages: bool) -> Vec<Shown> {
    if usages {
        usages_for(id)
    } else {
        recipes_for(id)
    }
}

fn pages(items: usize) -> usize {
    items.div_ceil(PER_PAGE).max(1)
}

/// An item's button, with mtg_craftguide's tooltips: which group it
/// stands for ("G" on the button), or that it is fuel.
fn item_button(fs: &mut String, x: f32, y: f32, size: f32, item: &str, element: &str, group: Option<&str>) {
    fs.push_str(&format!(
        "item_image_button[{x},{y};{size},{size};{item};{element};{label}]",
        x = num(x),
        y = num(y),
        item = esc(item),
        element = esc(element),
        label = if group.is_some() { "\nG" } else { "" },
    ));
    let id = content::id_of(item.split(' ').next().unwrap_or(""));
    let tooltip = match (group, id) {
        (Some(g), _) => Some(group_name(g)),
        (None, Some(id)) if crafting::burn_time(id) > 0.0 => {
            Some(format!("{}\n{}", content::description_of(id), colorize("orange", "Fuel")))
        }
        _ => None,
    };
    if let Some(t) = tooltip {
        fs.push_str(&format!("tooltip[{};{}]", esc(element), esc(&t)));
    }
}

/// Where the recipe goes: the window's right-hand column.
const RECIPE_X: f32 = 10.6;
const RECIPE_Y: f32 = 1.5;
const CELL: f32 = 1.1;

fn recipe_fs(fs: &mut String, shown: &Shown, rnum: usize, of: usize, usages: bool) {
    let v = view(shown.source);
    let (width, cooking, shapeless) = match v.method {
        Method::Normal(w) => (w, None, false),
        Method::Shapeless => (if v.len == 1 { 1 } else if v.len <= 4 { 2 } else { 3 }, None, true),
        Method::Cooking(t) => (1, Some(t), false),
    };
    let what = if usages { "Usage" } else { "Recipe" };
    fs.push_str(&format!("label[{RECIPE_X},0.5;{}]", esc(&format!("{what} {} of {of}", rnum + 1))));
    if of > 1 {
        fs.push_str(
            "image_button[11.35,7.25;0.8,0.8;craftguide_prev_icon.png;recipe_prev;]\
             image_button[12.25,7.25;0.8,0.8;craftguide_next_icon.png;recipe_next;]\
             tooltip[recipe_prev;Previous recipe]\
             tooltip[recipe_next;Next recipe]",
        );
    }
    let rows = v.len.div_ceil(width.max(1));
    let base_x = 3 - width;
    let base_y = if rows == 1 { 1 } else { 0 };
    for (i, cell) in v.cells[..v.len].iter().enumerate() {
        let (x, y) = (RECIPE_X + (base_x + i % width) as f32 * CELL, RECIPE_Y + (base_y + i / width) as f32 * CELL);
        match *cell {
            Ingredient::Empty => {}
            Ingredient::Item(id) => item_button(fs, x, y, 1.05, content::name_of(id), content::name_of(id), None),
            Ingredient::Group(g) => match shown.standing_for {
                Some((sg, id)) if sg == g => item_button(fs, x, y, 1.05, content::name_of(id), content::name_of(id), None),
                _ => {
                    let item = content::name_of(stereotype(g));
                    item_button(fs, x, y, 1.05, item, &format!("{item}.{g}"), Some(g));
                }
            },
        }
    }
    if shapeless || cooking.is_some() {
        let (icon, tip) = match cooking {
            Some(t) => ("furnace", format!("Cooking time: {}", colorize("yellow", &format!("{t}")))),
            None => ("shapeless", "Shapeless".to_string()),
        };
        fs.push_str(&format!("image[12.95,5.25;0.5,0.5;craftguide_{icon}.png]tooltip[12.95,5.25;0.5,0.5;{}]", esc(&tip)));
    }
    // The arrow, turned to point down at what it makes.
    fs.push_str("image[11.7,4.95;1,1;sfinv_crafting_arrow.png^[transformR270]");
    let name = content::name_of(v.output);
    let output = if v.count > 1 { format!("{name} {}", v.count) } else { name.to_string() };
    item_button(fs, 11.675, 6.0, 1.05, &output, name, None);
}

/// The Recipes tab (get_formspec), without the window's size and tabs.
pub fn formspec(g: &Guide) -> String {
    let items = items(&g.filter);
    let pagemax = pages(items.len());
    let page = g.page.min(pagemax - 1);
    let mut fs = format!(
        "style_type[item_image_button;padding=2]\
         field[0.5,8.75;3.4,0.8;filter;;{filter}]\
         label[8.05,9.15;{yellow} / {pagemax}]\
         image_button[4.0,8.75;0.8,0.8;craftguide_search_icon.png;search;]\
         image_button[4.9,8.75;0.8,0.8;craftguide_clear_icon.png;clear;]\
         image_button[7.0,8.75;0.8,0.8;craftguide_prev_icon.png;prev;]\
         image_button[9.2,8.75;0.8,0.8;craftguide_next_icon.png;next;]\
         tooltip[search;Search]\
         tooltip[clear;Reset]\
         tooltip[prev;Previous page]\
         tooltip[next;Next page]\
         field_enter_after_edit[filter;true]\
         field_close_on_enter[filter;false]",
        filter = esc(&g.filter),
        yellow = colorize("yellow", &(page + 1).to_string()),
    );
    if items.is_empty() {
        fs.push_str("label[3.5,3.5;No items to show.]");
    }
    for (i, &id) in items.iter().skip(page * PER_PAGE).take(PER_PAGE).enumerate() {
        let (x, y) = (0.5 + (i % COLUMNS) as f32 * 1.25, 0.5 + (i / COLUMNS) as f32 * 1.25);
        let name = content::name_of(id);
        item_button(&mut fs, x, y, 1.1, name, name, None);
    }
    if let Some(id) = g.item {
        let list = shown_for(id, g.usages);
        match list.get(g.rnum.min(list.len().saturating_sub(1))) {
            Some(s) => recipe_fs(&mut fs, s, g.rnum.min(list.len() - 1), list.len(), g.usages),
            None => {
                let text = if g.usages {
                    "No usages.\nClick again to show recipes."
                } else {
                    "No recipes.\nClick again to show usages."
                };
                fs.push_str(&format!("label[{RECIPE_X},3;{}]", esc(text)));
            }
        }
    }
    fs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(names: &[(&str, &str)]) -> Vec<(String, String)> {
        names.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn listed_items() {
        let l = listed();
        // Sorted by name, no repeats.
        assert!(l.windows(2).all(|w| content::name_of(w[0]) < content::name_of(w[1])));
        for id in [CONTENT_STONE, CONTENT_COBBLE, ITEM_STICK, CONTENT_APPLE_TREE, CONTENT_GLASS, CONTENT_DESERT_SAND, TOOL_PICK_STONE] {
            assert!(l.contains(&id), "{} not listed", content::name_of(id));
        }
        // Air and things with no recipe or usage aren't.
        assert!(!l.contains(&crate::world::CONTENT_AIR));
        assert!(!l.contains(&CONTENT_FURNACE_ACTIVE));
    }

    #[test]
    fn recipes_and_usages() {
        // Planks: one recipe per tree (and the bush stem), and the stone
        // pick's recipe is one way to use cobble, through group:stone.
        let r = recipes_for(CONTENT_APPLE_WOOD);
        assert!(r.len() >= 2);
        let u = usages_for(CONTENT_COBBLE);
        let pick = u.iter().find(|s| view(s.source).output == TOOL_PICK_STONE).expect("the stone pick");
        assert_eq!(pick.standing_for, Some(("stone", CONTENT_COBBLE)));
        // Cobble cooks into stone, and its stairs and slabs.
        assert!(recipes_for(CONTENT_STONE).iter().any(|s| s.source == Source::Cook(CONTENT_COBBLE)));
        assert!(u.iter().any(|s| matches!(s.source, Source::Stair(_))));
        assert!(u.iter().any(|s| s.source == Source::Cook(CONTENT_COBBLE)));
        // One glass recipe for all sand.
        let glass = recipes_for(CONTENT_GLASS);
        let cooked: Vec<_> = glass.iter().filter(|s| matches!(s.source, Source::Cook(_))).collect();
        assert_eq!(cooked.len(), 2, "sand, and glass fragments");
        assert!(usages_for(CONTENT_DESERT_SAND).iter().any(|s| view(s.source).output == CONTENT_GLASS));
    }

    #[test]
    fn stair_rows() {
        // The slab back into its block: two slabs in a column, so two rows.
        let i = crate::stairs::recipes().position(|(w, ..)| w == 1).unwrap();
        let v = view(Source::Stair(i as u16));
        assert_eq!((v.method, v.len), (Method::Normal(1), 2));
    }

    #[test]
    fn clicking() {
        let mut g = Guide { open: true, ..Default::default() };
        let fs = formspec(&g);
        assert!(fs.contains("item_image_button[0.5,0.5;1.1,1.1;"));
        assert!(fs.contains("1\x1b(c@#ffffff) / "));
        // Search for cobble, then click it: its recipes (from its stairs
        // and slabs), and again: what it makes.
        assert!(on_fields(&mut g, &fields(&[("filter", "Cobble"), ("key_enter_field", "filter")])));
        assert!(!on_fields(&mut g, &fields(&[("filter", "cobble"), ("search", "")])));
        assert!(items(&g.filter).iter().all(|&id| content::name_of(id).contains("cobble")
            || content::description_of(id).to_lowercase().contains("cobble")));
        assert!(on_fields(&mut g, &fields(&[("esp:cobble", ""), ("filter", "cobble")])));
        assert!(!g.usages);
        let fs = formspec(&g);
        assert!(fs.contains("Recipe 1 of 2"), "{fs}");
        assert!(fs.contains("item_image_button[11.7,1.5;1.05,1.05;esp:stair_cobble;"), "{fs}");
        assert!(on_fields(&mut g, &fields(&[("esp:cobble", "")])));
        assert!(g.usages);
        let fs = formspec(&g);
        assert!(fs.contains("Usage 1 of "), "{fs}");
        assert!(fs.contains("recipe_next"));
        assert!(on_fields(&mut g, &fields(&[("recipe_next", "")])));
        assert_eq!(g.rnum, 1);
        assert!(on_fields(&mut g, &fields(&[("recipe_prev", "")])));
        assert!(on_fields(&mut g, &fields(&[("recipe_prev", "")])));
        assert_eq!(g.rnum, usages_for(CONTENT_COBBLE).len() - 1);
        // A tree: no recipe.
        assert!(on_fields(&mut g, &fields(&[("esp:apple_tree", "")])));
        assert!(formspec(&g).contains("No recipes."));
        // A group cell clicked is its stereotype.
        assert!(on_fields(&mut g, &fields(&[("esp:apple_wood.wood", "")])));
        assert_eq!((g.item, g.usages), (Some(CONTENT_APPLE_WOOD), false));
        // Pages wrap round.
        assert!(on_fields(&mut g, &fields(&[("clear", "")])));
        let pages = pages(listed().len());
        assert!(pages > 1);
        assert!(on_fields(&mut g, &fields(&[("prev", "")])));
        assert_eq!(g.page, pages - 1);
        assert!(on_fields(&mut g, &fields(&[("next", "")])));
        assert_eq!(g.page, 0);
    }

    #[test]
    fn groups_shown() {
        // The wooden pick: planks shown as apple wood with a G, sticks as a
        // stick.
        let pick = recipes_for(TOOL_PICK_WOOD);
        let mut fs = String::new();
        recipe_fs(&mut fs, &pick[0], 0, 1, false);
        assert!(fs.contains("esp:apple_wood;esp:apple_wood.wood;\nG]"), "{fs}");
        assert!(fs.contains("tooltip[esp:apple_wood.wood;Any wood planks]"));
        assert!(fs.contains("esp:stick.stick"));
        // Cooking: a furnace icon and the time.
        let mut fs = String::new();
        recipe_fs(&mut fs, &Shown { source: Source::Cook(ITEM_FLOUR), standing_for: None }, 0, 1, false);
        assert!(fs.contains("craftguide_furnace.png") && fs.contains("15"), "{fs}");
        // Fuel says so.
        assert!(fs.contains("Fuel") == (crafting::burn_time(ITEM_FLOUR) > 0.0));
        // Several made: the count.
        let mut fs = String::new();
        recipe_fs(&mut fs, &recipes_for(CONTENT_APPLE_WOOD)[0], 0, 2, false);
        assert!(fs.contains(" 4;esp:apple_wood;]"), "{fs}");
    }
}
