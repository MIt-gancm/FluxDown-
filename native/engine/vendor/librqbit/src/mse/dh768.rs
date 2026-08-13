//! 768-bit Diffie-Hellman key exchange for BitTorrent MSE.
//!
//! Self-contained big-integer arithmetic (no `crypto-bigint` / `num-bigint`
//! dependency): limbs are `[u64; 12]` little-endian, with a naive
//! square-and-multiply modular exponentiation. Performance is irrelevant here
//! — one key exchange runs per peer connection, not per piece.
//!
//! The 768-bit prime is the fixed MSE prime (Azureus protocol-encryption
//! spec). Note this is **not** RFC 2409 group 1 — it has a modified tail.

use rand::RngCore;

/// 768-bit unsigned integer, little-endian limbs (`limbs[0]` is least
/// significant).
type U768 = [u64; 12];

/// 1536-bit unsigned integer (product of two 768-bit values).
type U1536 = [u64; 24];

/// The 768-bit MSE prime, little-endian limbs. Big-endian hex equivalent:
/// `FFFFFFFFFFFFFFFF C90FDAA22168C234 C4C6628B80DC1CD1 29024E088A67CC74`
/// `020BBEA63B139B22 514A08798E3404DD EF9519B3CD3A431B 302B0A6DF25F1437`
/// `4FE1356D6D51C245 E485B576625E7EC6 F44C42E9A63A3621 0000000000090563`
const DH_PRIME: U768 = [
    0x0000000000090563,
    0xF44C42E9A63A3621,
    0xE485B576625E7EC6,
    0x4FE1356D6D51C245,
    0x302B0A6DF25F1437,
    0xEF9519B3CD3A431B,
    0x514A08798E3404DD,
    0x020BBEA63B139B22,
    0x29024E088A67CC74,
    0xC4C6628B80DC1CD1,
    0xC90FDAA22168C234,
    0xFFFFFFFFFFFFFFFF,
];

const ONE: U768 = {
    let mut one = [0u64; 12];
    one[0] = 1;
    one
};

const TWO: U768 = {
    let mut two = [0u64; 12];
    two[0] = 2;
    two
};

/// Convert a 96-byte big-endian buffer into little-endian limbs.
fn bytes_be_to_limbs(bytes: &[u8; 96]) -> U768 {
    let mut limbs = [0u64; 12];
    for (i, limb) in limbs.iter_mut().enumerate() {
        let start = 96 - 8 * (i + 1);
        let mut be = [0u8; 8];
        be.copy_from_slice(&bytes[start..start + 8]);
        *limb = u64::from_be_bytes(be);
    }
    limbs
}

/// Convert little-endian limbs into a 96-byte big-endian buffer.
fn limbs_to_bytes_be(limbs: &U768) -> [u8; 96] {
    let mut bytes = [0u8; 96];
    for (i, limb) in limbs.iter().enumerate() {
        let be = limb.to_be_bytes();
        let start = 96 - 8 * (i + 1);
        bytes[start..start + 8].copy_from_slice(&be);
    }
    bytes
}

/// `a >= b`.
fn ge(a: &U768, b: &U768) -> bool {
    for i in (0..12).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    true
}

/// `a - b`, returning `(difference, borrow)`. Caller must guarantee `a >= b`.
fn sub(a: &U768, b: &U768) -> (U768, bool) {
    let mut diff = [0u64; 12];
    let mut borrow = 0u64;
    for i in 0..12 {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        diff[i] = d2;
        borrow = (b1 as u64) + (b2 as u64);
    }
    (diff, borrow != 0)
}

/// Schoolbook multiplication `a * b` into a 1536-bit result.
fn mul(a: &U768, b: &U768) -> U1536 {
    let mut out = [0u64; 24];
    for i in 0..12 {
        let mut carry = 0u128;
        for j in 0..12 {
            let idx = i + j;
            let cur = out[idx] as u128 + (a[i] as u128) * (b[j] as u128) + carry;
            out[idx] = cur as u64;
            carry = cur >> 64;
        }
        // Propagate the remaining carry into higher limbs (accumulating, since
        // `out[i+12]` may already hold a value from an earlier `i`).
        let mut k = i + 12;
        while carry > 0 && k < 24 {
            let cur = out[k] as u128 + carry;
            out[k] = cur as u64;
            carry = cur >> 64;
            k += 1;
        }
    }
    out
}

/// Binary long division: reduce a 1536-bit value modulo a 768-bit modulus.
fn mod_reduce(x: &U1536, m: &U768) -> U768 {
    let mut rem = [0u64; 12];
    // `hi` holds the (at most one) carry-out bit when `rem << 1` overflows
    // 768 bits. The invariant `rem < m` after each step guarantees `hi` is
    // either 0 or 1.
    let mut hi: u64 = 0;

    for bit in (0..1536).rev() {
        // rem = rem << 1, preserving the overflow bit.
        let new_hi = rem[11] >> 63;
        let mut carry = 0u64;
        for limb in rem.iter_mut() {
            let nc = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = nc;
        }
        hi = new_hi;

        // rem |= bit `bit` of x.
        if (x[bit / 64] >> (bit % 64)) & 1 == 1 {
            rem[0] |= 1;
        }

        // if (hi:rem) >= m, subtract m. `hi != 0` implies (hi:rem) >= 2^768 > m.
        if hi != 0 || ge(&rem, m) {
            let (d, borrow) = sub(&rem, m);
            debug_assert!(!borrow);
            rem = d;
            hi = 0;
        }
    }

    debug_assert_eq!(hi, 0);
    rem
}

/// Modular exponentiation `base^exp mod m`, exponent given as 20 big-endian
/// bytes (160-bit, matching libtorrent's DH private key size).
fn powm(base: &U768, exp: &[u8; 20], m: &U768) -> U768 {
    let mut result = ONE;
    for byte in exp.iter() {
        for bit in (0..8).rev() {
            result = mod_reduce(&mul(&result, &result), m);
            if (byte >> bit) & 1 == 1 {
                result = mod_reduce(&mul(&result, base), m);
            }
        }
    }
    result
}

/// A 768-bit Diffie-Hellman key pair for MSE.
pub struct Dh768 {
    /// 160-bit private exponent, big-endian (libtorrent uses 20 random bytes).
    secret: [u8; 20],
    /// `2^secret mod prime`.
    public: U768,
}

impl Dh768 {
    /// Generate a fresh key pair. `rng` is injectable for deterministic tests.
    pub fn generate(rng: &mut impl RngCore) -> Self {
        let mut secret = [0u8; 20];
        rng.fill_bytes(&mut secret);
        let public = powm(&TWO, &secret, &DH_PRIME);
        Dh768 { secret, public }
    }

    /// The 96-byte big-endian public key as sent on the wire.
    pub fn public_key_bytes(&self) -> [u8; 96] {
        limbs_to_bytes_be(&self.public)
    }

    /// Compute the shared secret from the peer's 96-byte public key.
    ///
    /// Returns `None` for a degenerate remote key (outside `[2, p-2]`), which
    /// would otherwise produce a small-subgroup shared secret and defeat the
    /// key exchange.
    pub fn shared_secret(&self, remote: &[u8; 96]) -> Option<[u8; 96]> {
        let remote_limbs = bytes_be_to_limbs(remote);
        if !ge(&remote_limbs, &TWO) {
            return None;
        }
        let prime_minus_one = sub(&DH_PRIME, &ONE).0;
        if ge(&remote_limbs, &prime_minus_one) {
            return None;
        }
        let shared = powm(&remote_limbs, &self.secret, &DH_PRIME);
        Some(limbs_to_bytes_be(&shared))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    #[test]
    fn shared_secret_agrees_between_parties() {
        let mut rng = SmallRng::seed_from_u64(0x5eed);
        let a = Dh768::generate(&mut rng);
        let b = Dh768::generate(&mut rng);

        let sa = a.shared_secret(&b.public_key_bytes()).unwrap();
        let sb = b.shared_secret(&a.public_key_bytes()).unwrap();
        assert_eq!(sa, sb);
    }

    #[test]
    fn public_key_is_96_bytes_and_nonzero() {
        let mut rng = SmallRng::seed_from_u64(7);
        let dh = Dh768::generate(&mut rng);
        let pk = dh.public_key_bytes();
        assert_eq!(pk.len(), 96);
        assert!(pk.iter().any(|&b| b != 0));
    }

    #[test]
    fn rejects_degenerate_remote_keys() {
        let mut rng = SmallRng::seed_from_u64(11);
        let dh = Dh768::generate(&mut rng);

        // All-zero and all-0xff public keys are outside [2, p-2].
        assert!(dh.shared_secret(&[0u8; 96]).is_none());
        assert!(dh.shared_secret(&[0xffu8; 96]).is_none());
        // g^0 = 1, also degenerate.
        let mut one = [0u8; 96];
        one[95] = 1;
        assert!(dh.shared_secret(&one).is_none());
    }

    #[test]
    fn mod_reduce_matches_known_value() {
        // (p + 1) mod p == 1.
        let mut y = [0u64; 24];
        y[..12].copy_from_slice(&DH_PRIME);
        y[0] = y[0].wrapping_add(1);
        assert_eq!(mod_reduce(&y, &DH_PRIME), ONE);

        // (2p) mod p == 0.
        let mut z = [0u64; 24];
        z[..12].copy_from_slice(&DH_PRIME);
        // z = p << 1.
        let mut carry = 0u64;
        for i in 0..12 {
            let nc = z[i] >> 63;
            z[i] = (z[i] << 1) | carry;
            carry = nc;
        }
        z[12] = carry;
        assert_eq!(mod_reduce(&z, &DH_PRIME), [0u64; 12]);
    }
}
