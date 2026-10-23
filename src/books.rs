// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from minetest_game's books (mods/default/craftitems.lua):
//   Copyright (C) 2011-2018 celeron55, Perttu Ahola <celeron55@gmail.com>
//   Copyright (C) 2011-2018 Various Minetest Game developers and contributors
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! Writing in books, as minetest_game's (mods/default/craftitems.lua):
//! using a book (left-click with it in hand) opens a window with a title
//! and a text to write; saving turns it into a "book with text", which
//! keeps what was written in its item metadata, with its author. Its
//! author can go on editing it (the Write tab) or read it (the Read
//! tab); anyone else gets only the Read view, 14 lines a page.
//!
//! A written book and an empty one, crafted together, make a copy, and
//! the original stays in the grid (inventory.rs).

use crate::content::{ITEM_BOOK, ITEM_BOOK_WRITTEN};
use crate::inventory::{Meta, Stack};
use crate::messages::formspec_escape as esc;
use crate::meta::Fields;

/// The form's name (minetest_game's).
pub const FORM: &str = "default:book";

const SIZE: &str = "size[8,8]";
/// Lines per page.
const LPP: usize = 14;
const MAX_TEXT: usize = 10000;
const MAX_TITLE: usize = 80;
const SHORT_TITLE: usize = 35;

pub fn is_book(id: u16) -> bool {
    matches!(id, ITEM_BOOK | ITEM_BOOK_WRITTEN)
}

fn core(tab: u8) -> String {
    format!("tabheader[0,0;book_header;Write,Read;{tab};false;false]")
}

fn write(title: &str, text: &str) -> String {
    format!(
        "field[0.5,1;7.5,0;title;Title:;{}]textarea[0.5,1.5;7.5,7;text;Contents:;{}]button_exit[2.5,7.5;3,1;save;Save]",
        esc(title),
        esc(text)
    )
}

fn read(owner: &str, title: &str, page_text: &str, text: &str, page: u32, page_max: u32) -> String {
    format!(
        "label[0.5,0.5;{}]tablecolumns[color;text]\
         tableoptions[background=#00000000;highlight=#00000000;border=false]\
         table[0.4,0;7,0.5;title;#FFFF00,{}]textarea[0.5,1.5;7.5,7;;{};]\
         button[2.4,7.6;0.8,0.8;book_prev;<]label[3.2,7.7;{}]button[4.9,7.6;0.8,0.8;book_next;>]",
        esc(&format!("by {owner}")),
        esc(title),
        esc(if page_text.is_empty() { text } else { page_text }),
        esc(&format!("Page {page} of {page_max}"))
    )
}

/// The text's lines, as Lua's gmatch over text .. "\n".
fn lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// One page's lines, each followed by a newline (formspec_string).
fn page_text(page: u32, lines: &[&str]) -> String {
    let first = (LPP * page.max(1) as usize) - LPP;
    lines.iter().skip(first).take(LPP).map(|l| format!("{l}\n")).collect()
}

fn number(fields: &Fields, name: &str) -> Option<u32> {
    fields.get(name).and_then(|v| v.trim().parse().ok())
}

/// The window a book opens for `player` (book_on_use), with the tab they
/// last chose.
pub fn use_form(stack: &Stack, player: &str, tab: u8) -> String {
    let data = stack.meta.fields();
    let (mut title, mut text, mut owner) = (String::new(), String::new(), player.to_string());
    let (mut page, mut page_max, mut page_string) = (1, 1, String::new());
    if let Some(o) = data.get("owner") {
        title = data.get("title").cloned().unwrap_or_default();
        text = data.get("text").cloned().unwrap_or_default();
        owner = o.clone();
        if let Some(p) = number(&data, "page") {
            page = p;
            page_max = number(&data, "page_max").unwrap_or(1);
            page_string = page_text(page, &lines(&text));
        }
    }
    let body = if title.is_empty() && text.is_empty() {
        write(&title, &text)
    } else if owner == player {
        if tab == 2 {
            core(2) + &read(&owner, &title, &page_string, &text, page, page_max)
        } else {
            core(1) + &write(&title, &text)
        }
    } else {
        read(&owner, &title, &page_string, &text, page, page_max)
    };
    format!("{SIZE}{body}")
}

/// What a book's window sent does (minetest_game's on_player_receive_fields
/// for "default:book").
#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    /// Said to the player.
    pub message: Option<&'static str>,
    /// What the book's slot now holds, if it changed.
    pub stack: Option<Option<Stack>>,
    /// A newly written book, split off a stack of empty ones, for the
    /// player's inventory (or the ground, if it's full).
    pub extra: Option<Stack>,
    /// A window to show next.
    pub show: Option<String>,
    /// The window closed: the player is done with the book.
    pub done: bool,
    /// The tab chosen, if one was.
    pub tab: Option<u8>,
}

/// The longest prefix of `s` of at most `n` bytes (Lua's sub(1, n), kept
/// whole characters).
fn cut(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// The book's window sent `fields`; `stack` is what is in the slot the
/// book was used from now, `tab` the tab the player last chose.
pub fn on_fields(stack: Option<Stack>, player: &str, fields: &[(String, String)], tab: u8) -> Reply {
    let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let mut reply = Reply::default();
    let Some(mut stack) = stack.filter(|s| is_book(s.id)) else {
        reply.message = Some("The book you were writing to mysteriously disappeared.");
        return reply;
    };
    let written = stack.id == ITEM_BOOK_WRITTEN;
    let mut data = stack.meta.fields();
    let title = data.get("title").cloned().unwrap_or_default();
    let text = data.get("text").cloned().unwrap_or_default();
    let is_owner = data.get("owner").is_some_and(|o| o == player);
    if let Some(header) = get("book_header").filter(|_| is_owner) {
        let contents = match header.trim().parse::<u8>() {
            Ok(1) => core(1) + &write(&title, &text),
            Ok(2) => {
                let (page, page_max) = (number(&data, "page").unwrap_or(1), number(&data, "page_max").unwrap_or(1));
                core(2) + &read(player, &title, &page_text(page, &lines(&text)), &text, page, page_max)
            }
            _ => return reply, // malicious data
        };
        reply.tab = header.trim().parse().ok();
        reply.show = Some(format!("{SIZE}{contents}"));
        return reply;
    }
    reply.done = get("quit").is_some();
    if let (Some(_), Some(new_title), Some(new_text)) = (get("save"), get("title"), get("text")) {
        let before = stack;
        let mut new_stack = None;
        if !written {
            if stack.count == 1 {
                stack = Stack::new(ITEM_BOOK_WRITTEN, 1);
            } else {
                stack.count -= 1;
                new_stack = Some(Stack::new(ITEM_BOOK_WRITTEN, 1));
            }
        }
        if !is_owner && !title.is_empty() && !text.is_empty() {
            return reply;
        }
        let new_title = cut(new_title, MAX_TITLE).to_string();
        let short = if new_title.len() > SHORT_TITLE + 3 { format!("{}...", cut(&new_title, SHORT_TITLE)) } else { new_title.clone() };
        let new_text: String = cut(new_text, MAX_TEXT)
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|&c| !(c < ' ' && c != '\t' && c != '\n' || c == '\x7f'))
            .collect();
        // Nothing written (or all of it rubbed out): an empty book again,
        // which stacks with the others. (minetest_game would keep a book
        // with text titled "".)
        if new_title.is_empty() && new_text.is_empty() {
            reply.stack = Some(Some(if written { Stack::new(ITEM_BOOK, 1) } else { before }));
            return reply;
        }
        let page_max = (new_text.matches('\n').count() + 1).div_ceil(LPP);
        data.insert("description".into(), format!("\"{short}\" by {player}"));
        data.insert("owner".into(), player.to_string());
        data.insert("title".into(), new_title);
        data.insert("text".into(), new_text);
        data.insert("page".into(), "1".into());
        data.insert("page_max".into(), page_max.to_string());
        match new_stack {
            Some(s) => reply.extra = Some(Stack { meta: Meta::new(data), ..s }),
            None => stack.meta = Meta::new(data),
        }
    } else if get("book_next").is_some() || get("book_prev").is_some() {
        let Some(page) = number(&data, "page") else {
            return reply;
        };
        let page_max = number(&data, "page_max").unwrap_or(1).max(1);
        let page = if get("book_next").is_some() {
            if page + 1 > page_max { 1 } else { page + 1 }
        } else if page <= 1 {
            page_max
        } else {
            page - 1
        };
        data.insert("page".into(), page.to_string());
        stack.meta = Meta::new(data);
        reply.show = Some(use_form(&stack, player, tab));
    }
    reply.stack = Some(Some(stack));
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn writing_a_book() {
        // A new book opens to write in.
        let book = Stack::new(ITEM_BOOK, 3);
        let form = use_form(&book, "Ann", 1);
        assert!(form.starts_with("size[8,8]field[0.5,1;7.5,0;title;Title:;]") && !form.contains("tabheader"));
        // Saved: one of the three becomes a book with text.
        let r = on_fields(Some(book), "Ann", &f(&[("title", "Notes; [1]"), ("text", "a\r\nb\x07"), ("save", "Save"), ("quit", "true")]), 1);
        assert!(r.done);
        assert_eq!(r.stack.unwrap().unwrap(), Stack::new(ITEM_BOOK, 2));
        let w = r.extra.unwrap();
        assert_eq!(w.id, ITEM_BOOK_WRITTEN);
        let m = w.meta.fields();
        assert_eq!((m["title"].as_str(), m["text"].as_str(), m["owner"].as_str()), ("Notes; [1]", "a\nb", "Ann"));
        assert_eq!((m["description"].as_str(), m["page"].as_str(), m["page_max"].as_str()), ("\"Notes; [1]\" by Ann", "1", "1"));
        // Its author gets the tabs, with the title escaped.
        let form = use_form(&w, "Ann", 1);
        assert!(form.contains("tabheader[0,0;book_header;Write,Read;1;false;false]") && form.contains("title;Title:;Notes\\; \\[1\\]]"));
        // Anyone else reads it.
        let form = use_form(&w, "Bob", 1);
        assert!(form.contains("label[0.5,0.5;by Ann]") && form.contains("Page 1 of 1") && !form.contains("tabheader"));
        // ... and can't change it.
        let r = on_fields(Some(w), "Bob", &f(&[("title", "Mine"), ("text", "now"), ("save", "Save")]), 1);
        assert_eq!(r.stack, None);
        // A single book becomes the written one in place.
        let r = on_fields(Some(Stack::new(ITEM_BOOK, 1)), "Ann", &f(&[("title", "T"), ("text", "x"), ("save", "")]), 1);
        assert_eq!(r.stack.unwrap().unwrap().id, ITEM_BOOK_WRITTEN);
        assert!(r.extra.is_none());
        // Saved with nothing in it: still (or again) an empty book.
        let r = on_fields(Some(Stack::new(ITEM_BOOK, 3)), "Ann", &f(&[("title", ""), ("text", ""), ("save", "")]), 1);
        assert_eq!((r.stack, r.extra), (Some(Some(Stack::new(ITEM_BOOK, 3))), None));
        let r = on_fields(Some(w), "Ann", &f(&[("title", ""), ("text", "\x07"), ("save", "")]), 1);
        assert_eq!(r.stack, Some(Some(Stack::new(ITEM_BOOK, 1))));
        // Gone from the hand: say so.
        let r = on_fields(None, "Ann", &f(&[("save", "")]), 1);
        assert_eq!(r.message, Some("The book you were writing to mysteriously disappeared."));
    }

    #[test]
    fn pages() {
        let text: String = (1..=30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let r = on_fields(Some(Stack::new(ITEM_BOOK, 1)), "Ann", &f(&[("title", "Long"), ("text", &text), ("save", "")]), 1);
        let mut book = r.stack.unwrap().unwrap();
        assert_eq!(book.meta.get("page_max").as_deref(), Some("3"));
        // Next page, then on past the end back to the first; back from the
        // first to the last.
        for want in ["2", "3", "1"] {
            let r = on_fields(Some(book), "Bob", &f(&[("book_next", ">")]), 1);
            book = r.stack.unwrap().unwrap();
            assert_eq!(book.meta.get("page").as_deref(), Some(want));
            assert!(r.show.unwrap().contains(&format!("Page {want} of 3")));
        }
        let r = on_fields(Some(book), "Bob", &f(&[("book_prev", "<")]), 1);
        let show = r.show.unwrap();
        assert!(show.contains("Page 3 of 3") && show.contains("line 29\nline 30\n;]") && !show.contains("line 28\n"));
        // The author switches tabs.
        let r = on_fields(Some(book), "Ann", &f(&[("book_header", "2")]), 1);
        assert_eq!(r.tab, Some(2));
        assert!(r.show.unwrap().contains("tabheader[0,0;book_header;Write,Read;2;false;false]"));
        // Over-long titles are cut, and shortened in the description.
        let long = "t".repeat(100);
        let r = on_fields(Some(Stack::new(ITEM_BOOK, 1)), "Ann", &f(&[("title", &long), ("text", "x"), ("save", "")]), 1);
        let m = r.stack.unwrap().unwrap().meta.fields();
        assert_eq!(m["title"].len(), 80);
        assert_eq!(m["description"], format!("\"{}...\" by Ann", "t".repeat(35)));
    }
}
