//! What is kept about a player between visits, apart from the inventory:
//! where they were, which way they faced, and their health.
//!
//! File format (`<dir>/<lowercase name>.player`), plain text:
//!   pos <x> <y> <z>      position of the feet, in nodes
//!   look <yaw> <pitch>   degrees
//!   hp <n>
//!   skin <key>           (see skins.rs)

use std::io;
use std::path::PathBuf;

use log::warn;

use crate::messages::PLAYER_MAX_HP;
use crate::storage;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerData {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub hp: u16,
    pub skin: String,
}

impl PlayerData {
    fn to_text(&self) -> String {
        format!(
            "pos {:.2} {:.2} {:.2}\nlook {:.1} {:.1}\nhp {}\nskin {}\n",
            self.pos[0], self.pos[1], self.pos[2], self.yaw, self.pitch, self.hp, self.skin
        )
    }

    fn from_text(text: &str) -> Option<Self> {
        let (mut pos, mut look, mut hp) = (None, (0.0, 0.0), PLAYER_MAX_HP);
        let mut skin = crate::skins::DEFAULT_SKIN.to_string();
        for line in text.lines() {
            let mut f = line.split_whitespace();
            let num = |s: Option<&str>| s.and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
            match f.next() {
                Some("pos") => pos = Some([num(f.next())?, num(f.next())?, num(f.next())?]),
                Some("look") => look = (num(f.next())?, num(f.next())?),
                Some("hp") => hp = f.next()?.parse::<u16>().ok()?.min(PLAYER_MAX_HP),
                Some("skin") => skin = f.next()?.to_string(),
                _ => {}
            }
        }
        Some(Self { pos: pos?, yaw: look.0, pitch: look.1, hp, skin })
    }
}

pub struct PlayerStore {
    dir: PathBuf,
}

impl PlayerStore {
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.player", name.to_ascii_lowercase()))
    }

    /// The saved state, or None for a player who has never been saved.
    pub fn load(&self, name: &str) -> Option<PlayerData> {
        match storage::read(&self.path(name)) {
            Ok(data) => {
                let parsed = PlayerData::from_text(&String::from_utf8_lossy(&data));
                if parsed.is_none() {
                    warn!("Ignoring unreadable player file for {name}");
                }
                parsed
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => {
                warn!("Reading player file for {name}: {e}");
                None
            }
        }
    }

    pub fn save(&self, name: &str, data: &PlayerData) {
        if let Err(e) = storage::write(&self.path(name), data.to_text().as_bytes()) {
            warn!("Saving player file for {name}: {e}");
            crate::status::problem();
        }
    }
}
