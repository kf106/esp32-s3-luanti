// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Keir Finlow-Bates
// Translated from minetest_game's keys mod (mods/keys) and
// default.can_interact_with_node (mods/default/functions.lua):
//   Copyright (C) 2011-2018 celeron55, Perttu Ahola <celeron55@gmail.com>
//   Copyright (C) 2011-2018 Various Minetest Game developers and contributors
// Licensed under the GNU Lesser General Public License, version 2.1 or (at
// your option) any later version: see COPYING.LESSER.

//! Keys, as minetest_game's keys mod: the owner of a locked chest, steel
//! door or steel trapdoor uses (left-clicks) a skeleton key on it, and
//! gets a key to it. Whoever wields that key may then open it, take from
//! it and dig it, as its owner can (default.can_interact_with_node).
//!
//! A lock's secret is 16 hex digits, made the first time a key is cut for
//! it and kept from then on, so every key cut for it fits. The chest
//! keeps it with its contents (chest.rs), the door with its owner
//! (doors.rs). The key keeps it in its item metadata, "secret", with a
//! description saying whose lock it opens.

use crate::content;
use crate::inventory::{Meta, Stack};
use crate::meta::Fields;

/// Whether a key with secret `key` opens a lock with secret `lock`: both
/// set, and the same.
pub fn fits(lock: Option<&str>, key: Option<&str>) -> bool {
    matches!((lock, key), (Some(l), Some(k)) if !l.is_empty() && l == k)
}

/// The secret a wielded stack carries, if it is a key (group "key").
pub fn secret_of(stack: Option<Stack>) -> Option<String> {
    stack.filter(|s| content::group_of(s.id, "key") == 1).and_then(|s| s.meta.get("secret")).filter(|s| !s.is_empty())
}

/// A new lock's secret from four random 16-bit numbers
/// (string.format("%04x%04x%04x%04x", ...)).
pub fn new_secret(mut random16: impl FnMut() -> u16) -> String {
    (0..4).map(|_| format!("{:04x}", random16())).collect()
}

/// The key cut for a lock: "Key to <owner>'s <node>".
pub fn key_for(secret: &str, owner: &str, node: u16) -> Stack {
    let fields = Fields::from([
        ("secret".to_string(), secret.to_string()),
        ("description".to_string(), format!("Key to {owner}'s {}", content::description_of(node))),
    ]);
    Stack { meta: Meta::new(fields), ..Stack::new(content::ITEM_KEY, 1) }
}

/// What a skeleton key says to a player who doesn't own the lock.
pub fn not_yours(node: u16) -> &'static str {
    if crate::doors::is_trapdoor(node) {
        "You do not own this trapdoor."
    } else if crate::doors::is_door(node) {
        "You do not own this locked door."
    } else {
        "You do not own this chest."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets() {
        let mut n = [0x0001u16, 0xabcd, 0xffff, 0x0a0b].into_iter();
        assert_eq!(new_secret(|| n.next().unwrap()), "0001abcdffff0a0b");
        assert!(fits(Some("0001abcdffff0a0b"), Some("0001abcdffff0a0b")));
        assert!(!fits(Some("0001abcdffff0a0b"), Some("0001abcdffff0a0c")));
        assert!(!fits(None, Some("x")) && !fits(Some("x"), None) && !fits(Some(""), Some("")));
    }

    #[test]
    fn cut_keys() {
        let key = key_for("0123456789abcdef", "Gandalf", content::CONTENT_CHEST_LOCKED);
        assert_eq!(key.meta.get("description").as_deref(), Some("Key to Gandalf's Locked Chest"));
        assert_eq!(secret_of(Some(key)).as_deref(), Some("0123456789abcdef"));
        // Through the item string and back, as it is saved.
        assert_eq!(secret_of(Stack::parse(&key.item_string())).as_deref(), Some("0123456789abcdef"));
        // A skeleton key, or a key with no secret, opens nothing.
        assert_eq!(secret_of(Some(Stack::new(content::ITEM_SKELETON_KEY, 1))), None);
        assert_eq!(secret_of(Some(Stack::new(content::ITEM_KEY, 1))), None);
        assert_eq!(not_yours(content::CONTENT_CHEST_LOCKED), "You do not own this chest.");
    }
}
