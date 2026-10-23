// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from Luanti's NodeMetadata serialisation (src/nodemetadata.cpp) and
// minetest_game's signs (mods/default/nodes.lua):
//   Copyright (C) 2010-2013 celeron55, Perttu Ahola <celeron55@gmail.com>
//   Copyright (C) 2011-2018 Various Minetest Game developers and contributors
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! Node metadata, as Luanti's (src/nodemetadata.cpp): named text values
//! kept with a node, such as a sign's text. The client has its own copy,
//! sent in the map blocks and, when it changes, in
//! TOCLIENT_NODEMETA_CHANGED; it shows a node's "infotext" when the node
//! is pointed at, and opens its "formspec" when it is right-clicked.
//!
//! The server keeps them in the world (world.rs), saved in
//! `world/meta.txt`:
//!
//! ```text
//! node <x> <y> <z>
//! <name> <value, with \ as \\, newlines as \n and returns as \r>
//! ```
//!
//! Chests, furnaces and bones keep their inventories their own way, so
//! only signs use this for now.

use crate::messages::Writer;
use crate::world::NodePos;
use std::collections::BTreeMap;

/// A node's values, by name.
pub type Fields = BTreeMap<String, String>;

/// Luanti's NodeMetadata::serialize, version 2: the values (none private)
/// and an empty inventory.
fn write_fields(w: &mut Writer, fields: &Fields) {
    w.u32(fields.len() as u32);
    for (name, value) in fields {
        w.string16(name);
        w.bytes32(value.as_bytes());
        w.u8(0); // not private
    }
    w.raw(b"EndInventory\n");
}

/// NodeMetadataList::serialize for a map block: positions within the
/// block. Version 0 says there are none.
pub fn write_block_list(w: &mut Writer, entries: &[(NodePos, &Fields)]) {
    if entries.is_empty() {
        w.u8(0);
        return;
    }
    w.u8(2).u16(entries.len() as u16);
    for (p, fields) in entries {
        let (x, y, z) = (p.0.rem_euclid(16), p.1.rem_euclid(16), p.2.rem_euclid(16));
        w.u16(((z * 16 + y) * 16 + x) as u16);
        write_fields(w, fields);
    }
}

/// TOCLIENT_NODEMETA_CHANGED: the metadata of some nodes, by absolute
/// position, zlib-compressed. A node with no values (empty `Fields`) has
/// its metadata removed on the client.
pub fn build_changed(entries: &[(NodePos, &Fields)]) -> Vec<u8> {
    let mut list = Writer::plain();
    list.u8(2).u16(entries.len() as u16);
    for (p, fields) in entries {
        list.i16(p.0 as i16).i16(p.1 as i16).i16(p.2 as i16);
        write_fields(&mut list, fields);
    }
    let mut w = Writer::new(crate::messages::TOCLIENT_NODEMETA_CHANGED);
    w.bytes32(&zlib_stored(&list.into_inner()));
    w.into_inner()
}

/// A zlib stream of `data` in stored (uncompressed) blocks: the client
/// inflates it like any other, and metadata is small.
pub fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = data.chunks(0xFFFF).peekable();
    if chunks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(chunk) = chunks.next() {
        out.push(chunks.peek().is_none() as u8); // BFINAL, BTYPE 00
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The text of `world/meta.txt`.
pub fn format(metas: &[(NodePos, &Fields)]) -> String {
    let mut t = String::new();
    for (p, fields) in metas {
        t.push_str(&format!("node {} {} {}\n", p.0, p.1, p.2));
        for (name, value) in fields.iter() {
            t.push_str(&format!("{name} {}\n", escape(value)));
        }
    }
    t
}

pub fn parse(text: &str) -> Vec<(NodePos, Fields)> {
    let mut out: Vec<(NodePos, Fields)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("node ") {
            let v: Vec<i32> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            if let [x, y, z] = v[..] {
                out.push(((x, y, z), Fields::new()));
            }
            continue;
        }
        let Some((_, fields)) = out.last_mut() else {
            continue;
        };
        let (name, value) = line.split_once(' ').unwrap_or((line, ""));
        if !name.is_empty() {
            fields.insert(name.to_string(), unescape(value));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Signs
// ---------------------------------------------------------------------------

/// The longest text a sign takes, in bytes (minetest_game's 512).
pub const SIGN_MAX: usize = 512;

/// A new sign's metadata: the text box its right-click opens
/// (minetest_game's on_construct).
pub fn new_sign() -> Fields {
    Fields::from([("formspec".to_string(), "field[text;;${text}]".to_string())])
}

/// What a sign says once `text` is written on it (minetest_game's
/// on_receive_fields), or None if it is too long. Control characters go,
/// except tabs and newlines; what it says shows, in quotes, when it is
/// pointed at.
pub fn write_sign(fields: &Fields, text: &str) -> Option<Fields> {
    if text.len() > SIGN_MAX {
        return None;
    }
    let text: String = text.chars().filter(|&c| !(c < ' ' && c != '\t' && c != '\n' || c == '\x7f')).collect();
    let mut out = fields.clone();
    out.insert("infotext".to_string(), if text.is_empty() { String::new() } else { format!("\"{text}\"") });
    out.insert("text".to_string(), text);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inflate a stored-blocks zlib stream (enough to check ours).
    fn inflate_stored(z: &[u8]) -> Vec<u8> {
        assert_eq!(&z[..2], &[0x78, 0x01]);
        let mut i = 2;
        let mut out = Vec::new();
        loop {
            let last = z[i] & 1;
            assert_eq!(z[i] & 6, 0);
            let len = u16::from_le_bytes([z[i + 1], z[i + 2]]) as usize;
            assert_eq!(u16::from_le_bytes([z[i + 3], z[i + 4]]), !(len as u16));
            out.extend_from_slice(&z[i + 5..i + 5 + len]);
            i += 5 + len;
            if last == 1 {
                break;
            }
        }
        assert_eq!(i + 4, z.len());
        out
    }

    #[test]
    fn zlib() {
        for n in [0, 1, 100, 65535, 65536, 200_000] {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 % 251) as u8).collect();
            assert_eq!(inflate_stored(&zlib_stored(&data)), data);
        }
        // Adler-32 of "Wikipedia" is 0x11E60398.
        let z = zlib_stored(b"Wikipedia");
        assert_eq!(&z[z.len() - 4..], &[0x11, 0xE6, 0x03, 0x98]);
    }

    #[test]
    fn file_round_trip() {
        let a = write_sign(&new_sign(), "Hello\nworld \\ back\\n").unwrap();
        let b = Fields::from([("x".to_string(), String::new())]);
        let text = format(&[((1, -2, 3), &a), ((-40, 5, 6), &b)]);
        let back = parse(&text);
        assert_eq!(back, vec![((1, -2, 3), a), ((-40, 5, 6), b)]);
    }

    #[test]
    fn signs() {
        let s = write_sign(&new_sign(), "Bell\x07 tower\n\tnorth\x7f").unwrap();
        assert_eq!(s["text"], "Bell tower\n\tnorth");
        assert_eq!(s["infotext"], "\"Bell tower\n\tnorth\"");
        assert_eq!(s["formspec"], "field[text;;${text}]");
        assert_eq!(write_sign(&s, "").unwrap().get("infotext").map(String::as_str), Some(""));
        assert!(write_sign(&s, &"x".repeat(513)).is_none());
        assert!(write_sign(&s, &"x".repeat(512)).is_some());
    }

    #[test]
    fn block_list() {
        let s = new_sign();
        let mut w = Writer::plain();
        write_block_list(&mut w, &[((17, -1, 33), &s)]);
        let b = w.into_inner();
        // version 2, one entry, at (1, 15, 1) in its block
        assert_eq!(&b[..5], &[2, 0, 1, ((1 * 16 + 15) * 16 + 1u16).to_be_bytes()[0], ((1 * 16 + 15) * 16 + 1u16).to_be_bytes()[1]]);
        assert!(b.ends_with(b"\x00EndInventory\n"));
        let mut w = Writer::plain();
        write_block_list(&mut w, &[]);
        assert_eq!(w.into_inner(), vec![0]);
    }
}
