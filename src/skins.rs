//! Player skins: textures for the player model (Minetest Game's
//! character.b3d), which players pick in their inventory window.
//!
//! The model uses the old 64x32 Minecraft skin layout. Besides the
//! built-in skins, any file in the SD card's media folder called
//! `skin_<name>.png` is offered as "<Name>". Modern 64x64 Minecraft skins
//! work too: the client is told to use only their top half, which holds
//! the 64x32 layout (Luanti's [combine texture modifier), so the ESP32
//! never has to decode an image.

use log::{info, warn};

use crate::content::Media;

/// The skin a player has until they choose another.
pub const DEFAULT_SKIN: &str = "default";

/// Built-in skins: (key, label, texture file).
const BUILT_IN: &[(&str, &str, &str)] = &[
    (DEFAULT_SKIN, "Default", "character.png"),
    ("wizard", "Wizard", "esp_skin_wizard.png"),
    ("elf", "Elf", "esp_skin_elf.png"),
    ("borgo", "Borgo", "esp_skin_borgo.png"),
    ("strider", "Strider", "esp_skin_strider.png"),
];

pub struct Skin {
    /// What the player file records.
    pub key: String,
    pub label: String,
    /// The texture string for the model: a file name, or a [combine
    /// that crops a 64x64 skin.
    pub texture: String,
}

pub struct Skins {
    list: Vec<Skin>,
}

impl Skins {
    /// The built-in skins, then any `skin_*.png` media files, in name order.
    pub fn from_media(media: &Media) -> Self {
        let mut list: Vec<Skin> = BUILT_IN
            .iter()
            .filter(|(_, _, file)| media.get(file).is_some())
            .map(|(key, label, file)| Skin {
                key: key.to_string(),
                label: label.to_string(),
                texture: file.to_string(),
            })
            .collect();
        let mut extra: Vec<Skin> = Vec::new();
        for f in media.files() {
            let Some(key) = f.name.strip_prefix("skin_").and_then(|n| n.strip_suffix(".png")) else {
                continue;
            };
            let key = key.to_ascii_lowercase();
            if key.is_empty() || list.iter().chain(&extra).any(|s| s.key == key) {
                continue;
            }
            let texture = match png_size(&f.data) {
                Some((64, 32)) => f.name.clone(),
                Some((64, 64)) => format!("[combine:64x32:0,0={}", f.name),
                other => {
                    warn!("Skin {}: expected a 64x32 or 64x64 PNG, found {other:?}; skipping", f.name);
                    continue;
                }
            };
            extra.push(Skin { label: label_for(&key), key, texture });
        }
        extra.sort_by(|a, b| a.key.cmp(&b.key));
        list.extend(extra);
        info!("Skins: {}", list.iter().map(|s| s.label.as_str()).collect::<Vec<_>>().join(", "));
        Self { list }
    }

    pub fn get(&self, i: usize) -> &Skin {
        &self.list[i.min(self.list.len() - 1)]
    }

    /// The index of a skin by key; the default for an unknown key (say, a
    /// skin file that has since been removed).
    pub fn index_of(&self, key: &str) -> usize {
        self.list.iter().position(|s| s.key == key).unwrap_or(0)
    }

    /// The next (or previous) skin, wrapping round.
    pub fn step(&self, i: usize, forward: bool) -> usize {
        let n = self.list.len();
        if forward { (i + 1) % n } else { (i + n - 1) % n }
    }
}

/// "red_knight" -> "Red knight"
fn label_for(key: &str) -> String {
    let words = key.replace(['_', '-'], " ");
    let mut c = words.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Width and height from a PNG's IHDR chunk.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    Some((be(16), be(20)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(label_for("red_knight"), "Red knight");
        assert_eq!(label_for("x"), "X");
    }
}
