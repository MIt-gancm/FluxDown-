//! 握手专用的 SHA-256 与 HMAC-SHA256。
//!
//! 协议 crate 不依赖任何加密库；daemon ↔ agent 握手只需要 HMAC-SHA256 这一个原语，
//! 实现遵循 FIPS 180-4 与 RFC 2104，并由标准向量测试钉住。

const BLOCK_LEN: usize = 64;
const DIGEST_LEN: usize = 32;

const INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const ROUND_CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// 增量 SHA-256；只在本模块内使用。
struct Sha256 {
    state: [u32; 8],
    block: [u8; BLOCK_LEN],
    filled: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            block: [0; BLOCK_LEN],
            filled: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        if self.filled > 0 {
            let take = (BLOCK_LEN - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled < BLOCK_LEN {
                return;
            }
            compress(&mut self.state, &self.block);
            self.filled = 0;
        }
        let (blocks, rest) = data.as_chunks::<BLOCK_LEN>();
        for block in blocks {
            compress(&mut self.state, block);
        }
        self.block[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    fn finish(mut self) -> [u8; DIGEST_LEN] {
        let bit_len = self.total_len.wrapping_mul(8);
        let mut padding = [0_u8; BLOCK_LEN];
        padding[0] = 0x80;
        // 补到 56 mod 64，再追加 8 字节大端比特长度。
        let padding_len = if self.filled < 56 {
            56 - self.filled
        } else {
            120 - self.filled
        };
        self.update(&padding[..padding_len]);
        self.update(&bit_len.to_be_bytes());
        let mut digest = [0_u8; DIGEST_LEN];
        for (chunk, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            *chunk = word.to_be_bytes();
        }
        digest
    }
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut schedule = [0_u32; 64];
    for (slot, bytes) in schedule.iter_mut().zip(block.as_chunks::<4>().0) {
        *slot = u32::from_be_bytes(*bytes);
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (constant, word) in ROUND_CONSTANTS.iter().zip(schedule) {
        let big_s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(big_s1)
            .wrapping_add(choose)
            .wrapping_add(*constant)
            .wrapping_add(word);
        let big_s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = big_s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

/// `HMAC-SHA256(key, parts[0] ‖ parts[1] ‖ …)`。
pub(crate) fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> [u8; DIGEST_LEN] {
    let mut block_key = [0_u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        let mut hasher = Sha256::new();
        hasher.update(key);
        block_key[..DIGEST_LEN].copy_from_slice(&hasher.finish());
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(&block_key.map(|byte| byte ^ 0x36));
    for part in parts {
        inner.update(part);
    }
    let inner_digest = inner.finish();
    let mut outer = Sha256::new();
    outer.update(&block_key.map(|byte| byte ^ 0x5c));
    outer.update(&inner_digest);
    outer.finish()
}

/// 小写十六进制。
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// 常量时间比较；长度不同直接判否（长度本身不是秘密）。
pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::{Sha256, constant_time_eq, hmac_sha256, to_hex};

    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        to_hex(&hasher.finish())
    }

    #[test]
    fn sha256_matches_fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_incremental_updates_match_one_shot_across_block_boundaries() {
        let data: Vec<u8> = (0..1_000_u32).map(|value| (value % 251) as u8).collect();
        let expected = sha256_hex(&data);
        for chunk_len in [1, 3, 55, 56, 57, 63, 64, 65, 127, 500] {
            let mut hasher = Sha256::new();
            for chunk in data.chunks(chunk_len) {
                hasher.update(chunk);
            }
            assert_eq!(to_hex(&hasher.finish()), expected, "chunk {chunk_len}");
        }
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&million),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_vectors() {
        assert_eq!(
            to_hex(&hmac_sha256(&[0x0b; 20], &[b"Hi There"])),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            to_hex(&hmac_sha256(
                b"Jefe",
                &[b"what do ya want ", b"for nothing?"]
            )),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // 密钥长于分组长度时先哈希。
        assert_eq!(
            to_hex(&hmac_sha256(
                &[0xaa; 131],
                &[b"Test Using Larger Than Block-Size Key - Hash Key First"]
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn constant_time_eq_requires_same_length_and_bytes() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
