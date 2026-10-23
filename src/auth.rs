//! Player accounts, stored on the SD card as one small file per player.
//!
//! Only the SRP salt and verifier are stored. The password itself never
//! reaches the server.
//!
//! File format (`<dir>/<lowercase name>.srp`):
//!   4 bytes  magic "SRP1"
//!   u16      salt length, then the salt
//!   u16      verifier length, then the verifier

use std::fs;
use std::io;
use std::path::PathBuf;

const MAGIC: &[u8; 4] = b"SRP1";
pub const PLAYERNAME_SIZE: usize = 20;

pub struct Account {
    pub salt: Vec<u8>,
    pub verifier: Vec<u8>,
}

pub enum NameError {
    Empty,
    TooLong,
    BadChars,
}

/// Luanti's player name rules: 1 to 20 characters from [A-Za-z0-9_-].
pub fn check_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > PLAYERNAME_SIZE {
        return Err(NameError::TooLong);
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(NameError::BadChars);
    }
    Ok(())
}

pub struct AuthStore {
    dir: PathBuf,
}

impl AuthStore {
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    // FAT is case-insensitive anyway; lowercasing makes that explicit, so
    // "Thinklair" and "thinklair" are the same account.
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.srp", name.to_ascii_lowercase()))
    }

    /// Load an account. `Ok(None)` means the name is not registered.
    /// Callers must have validated the name with `check_name` first.
    pub fn load(&self, name: &str) -> io::Result<Option<Account>> {
        let data = match fs::read(self.path(name)) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        decode(&data)
            .map(Some)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "corrupt account file"))
    }

    /// Create a new account. Returns `Ok(false)` if the name is already taken.
    pub fn create(&self, name: &str, account: &Account) -> io::Result<bool> {
        let path = self.path(name);
        if path.exists() {
            return Ok(false);
        }
        // Write to a temporary file and rename it into place, so a power cut
        // can't leave a half-written account behind.
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, encode(account))?;
        fs::rename(&tmp, &path)?;
        Ok(true)
    }
}

impl AuthStore {
    /// Replace an account's password (a new salt and verifier), or create
    /// the account. An empty verifier is a cleared password: the next
    /// login sets a new one, as Luanti's empty password.
    pub fn set(&self, name: &str, account: &Account) -> io::Result<()> {
        crate::storage::write(&self.path(name), &encode(account))
    }

    /// The folder the accounts are kept in.
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Whether the account exists.
    pub fn exists(&self, name: &str) -> bool {
        self.path(name).exists()
    }
}

fn encode(a: &Account) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + a.salt.len() + a.verifier.len());
    v.extend_from_slice(MAGIC);
    v.extend_from_slice(&(a.salt.len() as u16).to_be_bytes());
    v.extend_from_slice(&a.salt);
    v.extend_from_slice(&(a.verifier.len() as u16).to_be_bytes());
    v.extend_from_slice(&a.verifier);
    v
}

fn decode(data: &[u8]) -> Option<Account> {
    let rest = data.strip_prefix(MAGIC)?;
    let (salt, rest) = take16(rest)?;
    let (verifier, rest) = take16(rest)?;
    if !rest.is_empty() {
        return None;
    }
    Some(Account {
        salt: salt.to_vec(),
        verifier: verifier.to_vec(),
    })
}

fn take16(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let len = u16::from_be_bytes([*b.first()?, *b.get(1)?]) as usize;
    let body = b.get(2..2 + len)?;
    Some((body, &b[2 + len..]))
}
