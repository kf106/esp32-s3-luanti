//! A fast hash for the world's maps, whose keys are small integer tuples
//! (node and block positions). Rust's default hasher (SipHash) resists
//! deliberately colliding keys, which these maps don't need, and costs a
//! lot on the ESP32: profiling showed block building spending much of its
//! time in it. This is the "Fx" hash used inside the Rust compiler, with
//! a final mix so the hash table also gets well-spread high bits.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u32,
}

const SEED: u32 = 0x9E37_79B9;

impl FxHasher {
    #[inline]
    fn add(&mut self, v: u32) {
        self.hash = (self.hash.rotate_left(5) ^ v).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(4) {
            let mut v = 0u32;
            for (i, b) in chunk.iter().enumerate() {
                v |= (*b as u32) << (8 * i);
            }
            self.add(v);
        }
    }
    #[inline]
    fn write_u8(&mut self, v: u8) {
        self.add(v as u32);
    }
    #[inline]
    fn write_u16(&mut self, v: u16) {
        self.add(v as u32);
    }
    #[inline]
    fn write_u32(&mut self, v: u32) {
        self.add(v);
    }
    #[inline]
    fn write_u64(&mut self, v: u64) {
        self.add(v as u32);
        self.add((v >> 32) as u32);
    }
    #[inline]
    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }
    #[inline]
    fn write_i16(&mut self, v: i16) {
        self.add(v as u16 as u32);
    }
    #[inline]
    fn write_i32(&mut self, v: i32) {
        self.add(v as u32);
    }
    #[inline]
    fn finish(&self) -> u64 {
        // Spread the 32 bits over 64: hashbrown takes its tag from the top.
        (self.hash as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
}

pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FastSet<K> = HashSet<K, BuildHasherDefault<FxHasher>>;
