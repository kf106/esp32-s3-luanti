//! A small Zstandard compressor (RFC 8878) for the data Luanti expects
//! zstd-compressed: definitions, media and, above all, map blocks.
//!
//! It finds repeats with a 4-byte hash (greedy LZ77) and emits each match as
//! its own compressed block holding the preceding literals plus exactly one
//! sequence. With a single sequence per block, every symbol table can use
//! "RLE mode" (a single symbol, no FSE tables or state bits), which keeps the
//! encoder simple. The cost is about 10 bytes per match, which is nothing
//! for map blocks: 16 KB of air or layered terrain shrinks to a few hundred
//! bytes. Data without repeats goes out in raw (stored) blocks.

const MAGIC: u32 = 0xFD2F_B528;
/// Largest amount of data one block may regenerate.
const MAX_BLOCK: usize = 128 * 1024;
/// Shorter matches aren't worth a block of their own.
const MIN_MATCH: usize = 12;
const HASH_BITS: u32 = 13;
/// How many positions at the end of each match are added to the hash table.
const INDEX_TAIL: usize = 32;
/// Fixed distances also tried at every position. A map block stores 4096
/// u16 content ids, then 4096 param1 and 4096 param2 bytes, indexed
/// z*256 + y*16 + x, so its data repeats at these strides.
const STRIDES: [usize; 8] = [1, 2, 16, 32, 256, 512, 4096, 8192];

const BLOCK_RAW: u32 = 0;
const BLOCK_COMPRESSED: u32 = 2;

enum Block {
    /// data[start..end] stored as is.
    Raw { start: usize, end: usize },
    /// Literals data[lit_start..lit_end], then copy `len` bytes from
    /// `offset` bytes back.
    Match { lit_start: usize, lit_end: usize, offset: usize, len: usize },
}

pub fn compress(data: &[u8]) -> Vec<u8> {
    let blocks = find_blocks(data);

    let mut out = Vec::with_capacity(data.len() / 4 + 32);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    // Frame_Content_Size_flag = 2 (4 bytes), Single_Segment_flag = 1:
    // the window is the whole content, so matches may reach back anywhere.
    out.push(0b1010_0000);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());

    if blocks.is_empty() {
        block_header(&mut out, true, BLOCK_RAW, 0);
        return out;
    }
    let last_index = blocks.len() - 1;
    for (i, block) in blocks.iter().enumerate() {
        let last = i == last_index;
        match *block {
            Block::Raw { start, end } => {
                block_header(&mut out, last, BLOCK_RAW, end - start);
                out.extend_from_slice(&data[start..end]);
            }
            Block::Match { lit_start, lit_end, offset, len } => {
                let body = match_block(&data[lit_start..lit_end], offset, len);
                block_header(&mut out, last, BLOCK_COMPRESSED, body.len());
                out.extend_from_slice(&body);
            }
        }
    }
    out
}

fn hash(b: &[u8]) -> usize {
    let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    (v.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
}

/// Greedy LZ77 parse into blocks.
fn find_blocks(data: &[u8]) -> Vec<Block> {
    let n = data.len();
    let mut blocks = Vec::new();
    let mut table = vec![usize::MAX; 1 << HASH_BITS];
    let mut lit_start = 0;
    let mut i = 0;

    while i + 4 <= n {
        // Keep pending literals within one block's limit.
        if i - lit_start >= MAX_BLOCK / 2 {
            blocks.push(Block::Raw { start: lit_start, end: i });
            lit_start = i;
        }

        let h = hash(&data[i..]);
        let hashed = table[h];
        table[h] = i;

        // Try the hash candidate, plus the strides at which map block data
        // repeats (node rows, layers, whole parameter arrays), and keep the
        // longest match.
        let max_len = (n - i).min(MAX_BLOCK - (i - lit_start));
        let mut best = (0, 0); // (len, candidate)
        let strides = STRIDES.iter().filter(|&&s| s <= i).map(|&s| i - s);
        for candidate in std::iter::once(hashed).filter(|&c| c != usize::MAX).chain(strides) {
            if data[candidate..candidate + 4] != data[i..i + 4] {
                continue;
            }
            let mut len = 4;
            while len < max_len && data[candidate + len] == data[i + len] {
                len += 1;
            }
            if len > best.0 {
                best = (len, candidate);
                if len == max_len {
                    break; // can't do better
                }
            }
        }

        let (len, candidate) = best;
        {
            if len >= MIN_MATCH {
                blocks.push(Block::Match {
                    lit_start,
                    lit_end: i,
                    offset: i - candidate,
                    len,
                });
                // Index the end of the matched region so later data can
                // refer to it. Indexing all of it costs a hash per byte,
                // and the fixed strides already find the long repeats.
                let end = (i + len).min(n.saturating_sub(3));
                for j in end.saturating_sub(INDEX_TAIL).max(i + 1)..end {
                    table[hash(&data[j..])] = j;
                }
                i += len;
                lit_start = i;
                continue;
            }
        }
        i += 1;
    }
    if lit_start < n {
        blocks.push(Block::Raw { start: lit_start, end: n });
    }
    blocks
}

fn block_header(out: &mut Vec<u8>, last: bool, block_type: u32, size: usize) {
    let header = last as u32 | (block_type << 1) | ((size as u32) << 3);
    out.extend_from_slice(&header.to_le_bytes()[..3]);
}

/// A compressed block: raw literals, then one sequence.
fn match_block(literals: &[u8], offset: usize, len: usize) -> Vec<u8> {
    let mut b = Vec::with_capacity(literals.len() + 16);

    // Literals section, Raw_Literals_Block (type 0)
    let n = literals.len();
    if n < 32 {
        b.push((n << 3) as u8);
    } else if n < 4096 {
        b.push(((n & 0x0F) << 4) as u8 | 0b0100);
        b.push((n >> 4) as u8);
    } else {
        b.push(((n & 0x0F) << 4) as u8 | 0b1100);
        b.push((n >> 4) as u8);
        b.push((n >> 12) as u8);
    }
    b.extend_from_slice(literals);

    // Sequences section: one sequence, all three tables in RLE mode.
    let (ll_code, ll_bits, ll_extra) = literal_length_code(n as u32);
    let (ml_code, ml_bits, ml_extra) = match_length_code(len as u32);
    // Offset_Value = offset + 3 avoids the repeat-offset codes 1..3.
    let of_value = offset as u32 + 3;
    let of_code = 31 - of_value.leading_zeros();
    let of_extra = of_value - (1 << of_code);

    b.push(1); // Number_of_Sequences
    b.push(0b01_01_01_00); // LL, OF, ML: RLE_Mode
    b.push(ll_code);
    b.push(of_code as u8);
    b.push(ml_code);

    // Backwards bitstream: the decoder reads offset, match length and
    // literal length extra bits, in that order, from the end; so they are
    // written in reverse. RLE tables have no state bits.
    let mut bits = BitWriter::default();
    bits.add(ll_extra, ll_bits);
    bits.add(ml_extra, ml_bits);
    bits.add(of_extra, of_code);
    b.extend_from_slice(&bits.finish());
    b
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    acc: u64,
    count: u32,
}

impl BitWriter {
    fn add(&mut self, value: u32, nbits: u32) {
        if nbits == 0 {
            return;
        }
        self.acc |= (value as u64 & ((1u64 << nbits) - 1)) << self.count;
        self.count += nbits;
        while self.count >= 8 {
            self.bytes.push(self.acc as u8);
            self.acc >>= 8;
            self.count -= 8;
        }
    }

    /// Append the end marker (a single 1 bit) and pad to a whole byte.
    fn finish(mut self) -> Vec<u8> {
        self.add(1, 1);
        if self.count > 0 {
            self.bytes.push(self.acc as u8);
        }
        self.bytes
    }
}

/// Literals_Length codes 16..=35: (baseline, extra bits).
const LL_TABLE: [(u32, u32); 20] = [
    (16, 1), (18, 1), (20, 1), (22, 1), (24, 2), (28, 2), (32, 3), (40, 3),
    (48, 4), (64, 6), (128, 7), (256, 8), (512, 9), (1024, 10), (2048, 11),
    (4096, 12), (8192, 13), (16384, 14), (32768, 15), (65536, 16),
];

/// Match_Length codes 32..=52: (baseline, extra bits).
const ML_TABLE: [(u32, u32); 21] = [
    (35, 1), (37, 1), (39, 1), (41, 1), (43, 2), (47, 2), (51, 3), (59, 3),
    (67, 4), (83, 4), (99, 5), (131, 7), (259, 8), (515, 9), (1027, 10),
    (2051, 11), (4099, 12), (8195, 13), (16387, 14), (32771, 15), (65539, 16),
];

fn literal_length_code(v: u32) -> (u8, u32, u32) {
    if v < 16 {
        return (v as u8, 0, 0);
    }
    let i = LL_TABLE.iter().rposition(|&(base, _)| base <= v).unwrap();
    let (base, bits) = LL_TABLE[i];
    (16 + i as u8, bits, v - base)
}

fn match_length_code(v: u32) -> (u8, u32, u32) {
    debug_assert!(v >= 3);
    if v < 35 {
        return ((v - 3) as u8, 0, 0);
    }
    let i = ML_TABLE.iter().rposition(|&(base, _)| base <= v).unwrap();
    let (base, bits) = ML_TABLE[i];
    (32 + i as u8, bits, v - base)
}
