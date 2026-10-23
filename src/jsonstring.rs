// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from Luanti's src/util/serialize.cpp (serializeJsonString,
// serializeJsonStringIfNeeded, deSerializeJsonStringIfNeeded):
//   Copyright (C) 2010-2013 celeron55, Perttu Ahola <celeron55@gmail.com>
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! Luanti's JSON strings in item strings: an item's metadata is quoted
//! and escaped when it holds spaces or the like (inventory.rs).

/// Luanti's serializeJsonStringIfNeeded: as it is, or quoted and escaped
/// if it has spaces, quotes, control characters or anything not ASCII.
pub fn json_if_needed(s: &str) -> String {
    if !s.bytes().any(|b| b <= 0x1f || b >= 0x7f || b == b' ' || b == b'"') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 || c as u32 == 127 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The next word of an item string, or a JSON string
/// (deSerializeJsonStringIfNeeded), taken off the front of `rest`.
pub fn next_token(rest: &mut &str) -> Option<String> {
    let s = rest.trim_start_matches(' ');
    if s.is_empty() {
        *rest = s;
        return None;
    }
    if let Some(body) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = body.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => {
                    *rest = &body[i + 1..];
                    return Some(out);
                }
                '\\' => match chars.next().map(|(_, c)| c) {
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('u') => {
                        let hex: String = (0..4).filter_map(|_| chars.next().map(|(_, c)| c)).collect();
                        if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            out.push(c);
                        }
                    }
                    Some(c) => out.push(c),
                    None => break,
                },
                c => out.push(c),
            }
        }
        *rest = "";
        return Some(out); // unterminated: take what there is
    }
    let end = s.find(' ').unwrap_or(s.len());
    *rest = &s[end..];
    Some(s[..end].to_string())
}
