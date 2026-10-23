//! The server side of SRP-6a login, as Luanti does it (src/util/srp.cpp):
//! SHA-256 and the 2048-bit group from RFC 5054 appendix A.
//!
//! The client sends A. We reply with the salt and B, and work out the proof
//! M the client must send back if it knows the password. The client never
//! checks our own proof (H_AMK), so we don't compute it.
//!
//! The arithmetic uses fixed-size 2048-bit numbers in Montgomery form, which
//! needs only multiplications and additions. A login takes three modular
//! exponentiations with 256-bit exponents.

use crate::sha256::{sha256, Sha256};

const LIMBS: usize = 64; // 64 x 32 bits = 2048 bits
const BYTES: usize = LIMBS * 4;
type Num = [u32; LIMBS];

/// RFC 5054 2048-bit prime, generator 2.
const N_HEX: &str = "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050A37329CBB4A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B855F97993EC975EEAA80D740ADBF4FF747359D041D5C33EA71D281E446B14773BCA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DBFBB694B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73";
const G: u32 = 2;

/// What the server keeps between SRP_BYTES_A and SRP_BYTES_M.
pub struct Challenge {
    /// B, to send to the client with the salt.
    pub bytes_b: Vec<u8>,
    /// The proof a client that knows the password will send.
    expected_m: [u8; 32],
}

impl Challenge {
    pub fn check(&self, m: &[u8]) -> bool {
        // Not timing-sensitive in practice (one try per connection), but cheap
        // to do properly.
        m.len() == 32 && m.iter().zip(&self.expected_m).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }
}

/// Start a login for `username` (exactly as the client sent it in
/// TOSERVER_INIT) with the stored salt and verifier and the client's A.
/// `None` means A failed the SRP-6a safety check or a value was too large.
pub fn start(login: &Login) -> Option<Challenge> {
    let mut b = [0u8; 32];
    random_fill(&mut b);
    start_with_b(login, &b)
}

/// What a login brings: grouped into one struct to keep the argument count
/// low (see server::Setup).
pub struct Login<'a> {
    /// Exactly as the client sent it in TOSERVER_INIT.
    pub username: &'a str,
    pub salt: &'a [u8],
    pub verifier: &'a [u8],
    /// The client's A.
    pub bytes_a: &'a [u8],
}

/// `start` with a given secret b, for testing against known values.
pub fn start_with_b(login: &Login, secret_b: &[u8]) -> Option<Challenge> {
    let Login { username, salt, verifier, bytes_a } = *login;
    let g = Group::new();
    let v = g.reduce(from_be(verifier)?);
    let a_raw = from_be(bytes_a)?;
    let a = g.reduce(a_raw);
    if is_zero(&a) {
        return None; // SRP-6a safety check: A mod N must not be 0
    }

    // k = H(N | PAD(g))
    let k = from_be(&h_nn(&g.n, &small(G)))?;
    // B = k*v + g^b
    let gb = g.pow(&small(G), secret_b);
    let big_b = g.add(&g.mul(&k, &v), &gb);
    // u = H(PAD(A) | PAD(B))
    let u = h_nn(&a_raw, &big_b);
    // S = (A * v^u)^b
    let s = g.pow(&g.mul(&a, &g.pow(&v, &u)), secret_b);
    let key = sha256(&to_be_min(&s));

    // M = H(H(N) xor H(g), H(I), s, A, B, K)
    let hn = sha256(&to_be_min(&g.n));
    let hg = sha256(&to_be_min(&small(G)));
    let mut hxor = [0u8; 32];
    for i in 0..32 {
        hxor[i] = hn[i] ^ hg[i];
    }
    let mut h = Sha256::new();
    h.update(&hxor)
        .update(&sha256(username.as_bytes()))
        .update(salt)
        .update(&to_be_min(&a_raw))
        .update(&to_be_min(&big_b))
        .update(&key);
    Some(Challenge {
        bytes_b: to_be_min(&big_b),
        expected_m: h.finish(),
    })
}

/// A new salt and verifier for a password, as Luanti's
/// generate_srp_verifier_and_salt: x = H(salt | H(lowercase name ":"
/// password)), v = g^x mod N, with a 16-byte random salt. Takes one
/// modular exponentiation (about a third of a second on the ESP32).
pub fn new_verifier(name: &str, password: &str) -> (Vec<u8>, Vec<u8>) {
    let mut salt = vec![0u8; 16];
    random_fill(&mut salt);
    let v = verifier_with_salt(name, password, &salt);
    (salt, v)
}

fn verifier_with_salt(name: &str, password: &str, salt: &[u8]) -> Vec<u8> {
    let mut inner = Sha256::new();
    inner.update(name.to_ascii_lowercase().as_bytes()).update(b":").update(password.as_bytes());
    let mut outer = Sha256::new();
    outer.update(salt).update(&inner.finish());
    let x = outer.finish();
    let g = Group::new();
    to_be_min(&g.pow(&small(G), &x))
}

/// H(PAD(x) | PAD(y)), each padded to the length of N.
fn h_nn(x: &Num, y: &Num) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(&to_be_padded(x)).update(&to_be_padded(y));
    h.finish()
}

// ---------------------------------------------------------------------------
// Random numbers
// ---------------------------------------------------------------------------

#[cfg(target_os = "espidf")]
fn random_fill(buf: &mut [u8]) {
    // The hardware RNG is a true random source while Wi-Fi is running.
    unsafe { esp_idf_svc::sys::esp_fill_random(buf.as_mut_ptr().cast(), buf.len()) }
}

#[cfg(not(target_os = "espidf"))]
fn random_fill(buf: &mut [u8]) {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .expect("reading /dev/urandom");
}

// ---------------------------------------------------------------------------
// 2048-bit arithmetic modulo N
// ---------------------------------------------------------------------------

fn small(v: u32) -> Num {
    let mut n = [0; LIMBS];
    n[0] = v;
    n
}

fn is_zero(x: &Num) -> bool {
    x.iter().all(|&l| l == 0)
}

/// Big-endian bytes to a number; `None` if it doesn't fit in 2048 bits.
fn from_be(bytes: &[u8]) -> Option<Num> {
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    let bytes = &bytes[start..];
    if bytes.len() > BYTES {
        return None;
    }
    let mut n = [0u32; LIMBS];
    for (i, &byte) in bytes.iter().rev().enumerate() {
        n[i / 4] |= (byte as u32) << (8 * (i % 4));
    }
    Some(n)
}

fn to_be_padded(x: &Num) -> [u8; BYTES] {
    let mut out = [0u8; BYTES];
    for i in 0..LIMBS {
        out[BYTES - 4 * (i + 1)..BYTES - 4 * i].copy_from_slice(&x[i].to_be_bytes());
    }
    out
}

/// Big-endian bytes without leading zeros (GMP's mpz_export).
fn to_be_min(x: &Num) -> Vec<u8> {
    let b = to_be_padded(x);
    let start = b.iter().position(|&v| v != 0).unwrap_or(BYTES);
    b[start..].to_vec()
}

fn geq(a: &Num, b: &Num) -> bool {
    for i in (0..LIMBS).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    true
}

/// a -= b, wrapping (the caller knows the true result fits).
fn sub_in_place(a: &mut Num, b: &Num) {
    let mut borrow = 0u64;
    for i in 0..LIMBS {
        let d = (a[i] as u64).wrapping_sub(b[i] as u64).wrapping_sub(borrow);
        a[i] = d as u32;
        borrow = (d >> 63) & 1;
    }
}

struct Group {
    n: Num,
    /// -N^-1 mod 2^32
    n0inv: u32,
    /// R^2 mod N, with R = 2^2048
    r2: Num,
}

impl Group {
    fn new() -> Self {
        let mut n_bytes = [0u8; BYTES];
        for (i, byte) in n_bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&N_HEX[2 * i..2 * i + 2], 16).unwrap();
        }
        let n = from_be(&n_bytes).unwrap();

        // Newton iteration for N^-1 mod 2^32 (N is odd)
        let mut inv: u32 = 1;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u32.wrapping_sub(n[0].wrapping_mul(inv)));
        }
        let n0inv = inv.wrapping_neg();

        // R^2 mod N by doubling 1 a total of 4096 times
        let mut r2 = small(1);
        for _ in 0..2 * 2048 {
            let carry = r2[LIMBS - 1] >> 31;
            for i in (1..LIMBS).rev() {
                r2[i] = (r2[i] << 1) | (r2[i - 1] >> 31);
            }
            r2[0] <<= 1;
            if carry != 0 || geq(&r2, &n) {
                sub_in_place(&mut r2, &n);
            }
        }
        Self { n, n0inv, r2 }
    }

    /// x mod N for any x below 2^2048. N's top bit is set, so x < 2N and one
    /// subtraction is enough.
    fn reduce(&self, mut x: Num) -> Num {
        if geq(&x, &self.n) {
            sub_in_place(&mut x, &self.n);
        }
        x
    }

    /// Montgomery product a*b/R mod N (CIOS), for a, b < N.
    fn mont(&self, a: &Num, b: &Num) -> Num {
        let n = &self.n;
        let mut t = [0u32; LIMBS + 2];
        for i in 0..LIMBS {
            let mut c: u64 = 0;
            let bi = b[i] as u64;
            for j in 0..LIMBS {
                let s = t[j] as u64 + a[j] as u64 * bi + c;
                t[j] = s as u32;
                c = s >> 32;
            }
            let s = t[LIMBS] as u64 + c;
            t[LIMBS] = s as u32;
            t[LIMBS + 1] = (s >> 32) as u32;

            let m = t[0].wrapping_mul(self.n0inv) as u64;
            let s = t[0] as u64 + m * n[0] as u64;
            let mut c = s >> 32;
            for j in 1..LIMBS {
                let s = t[j] as u64 + m * n[j] as u64 + c;
                t[j - 1] = s as u32;
                c = s >> 32;
            }
            let s = t[LIMBS] as u64 + c;
            t[LIMBS - 1] = s as u32;
            t[LIMBS] = t[LIMBS + 1] + (s >> 32) as u32;
        }
        let mut r = [0u32; LIMBS];
        r.copy_from_slice(&t[..LIMBS]);
        if t[LIMBS] != 0 || geq(&r, n) {
            sub_in_place(&mut r, n);
        }
        r
    }

    /// a*b mod N
    fn mul(&self, a: &Num, b: &Num) -> Num {
        self.mont(&self.mont(a, b), &self.r2)
    }

    /// a+b mod N, for a, b < N
    fn add(&self, a: &Num, b: &Num) -> Num {
        let mut r = [0u32; LIMBS];
        let mut c = 0u64;
        for i in 0..LIMBS {
            let s = a[i] as u64 + b[i] as u64 + c;
            r[i] = s as u32;
            c = s >> 32;
        }
        if c != 0 || geq(&r, &self.n) {
            sub_in_place(&mut r, &self.n);
        }
        r
    }

    /// base^exp mod N, exp given as big-endian bytes.
    fn pow(&self, base: &Num, exp: &[u8]) -> Num {
        let base_m = self.mont(base, &self.r2);
        let mut acc = self.mont(&small(1), &self.r2); // 1 in Montgomery form
        for &byte in exp {
            for bit in (0..8).rev() {
                acc = self.mont(&acc, &acc);
                if byte >> bit & 1 == 1 {
                    acc = self.mont(&acc, &base_m);
                }
            }
        }
        self.mont(&acc, &small(1))
    }
}
