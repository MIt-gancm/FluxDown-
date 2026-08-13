//! Message Stream Encryption (MSE / Azureus protocol encryption) handshake.
//!
//! Implements the outgoing (initiator) and incoming (acceptor) handshake so
//! librqbit can connect to peers that require RC4 encryption (Thunder,
//! BitComet, libtorrent with `prefer_rc4`). Plaintext peers are detected by the
//! first handshake byte (`0x13` == the BT protocol-string length) and handled
//! unchanged.
//!
//! The wire format and crypto parameters follow libtorrent's `pe_crypto.cpp`
//! exactly: 768-bit DH with the fixed MSE prime, RC4 with the 1024-byte key
//! discard, and SHA-1 key derivation (`hash("keyA"/"keyB", S, SKEY)`).

pub mod dh768;
pub mod rc4;
pub mod stream;

use anyhow::{bail, Context, Result};
use rand::{Rng, RngCore};
use sha1w::{ISha1, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use dh768::Dh768;
use rc4::Rc4;
use stream::{Rc4Reader, Rc4Writer};

/// Length of the BitTorrent handshake (1 pstrlen + 19 pstr + 8 reserved + 20
/// info_hash + 20 peer_id), i.e. the "IA" (initial application data) length.
const BT_HANDSHAKE_LEN: usize = 68;

/// Maximum PadA/PadB padding length (MSE spec: 0..=512).
const MAX_PAD: usize = 512;

/// Length of the verification constant (all-zero 8 bytes).
const VC_LEN: usize = 8;

/// `crypto_provide` / `crypto_select` value for RC4 full-stream encryption.
const CRYPTO_RC4: u32 = 2;

/// Boxed async stream types, chosen so the encrypted and plaintext branches of
/// an MSE handshake share one type at the call site.
pub type BoxedRead = Box<dyn AsyncRead + Send + Unpin>;
pub type BoxedWrite = Box<dyn AsyncWrite + Send + Unpin>;

/// Result of an outgoing handshake.
pub enum Outcome {
    /// MSE succeeded; the returned streams transparently encrypt/decrypt.
    Encrypted {
        read: BoxedRead,
        write: BoxedWrite,
    },
    /// The peer answered in plaintext (does not speak MSE). `prefix` holds the
    /// already-read bytes of the plaintext BT handshake and must be replayed
    /// into the read buffer.
    Plaintext {
        read: BoxedRead,
        write: BoxedWrite,
        prefix: Vec<u8>,
    },
}

fn sha1(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finish()
}

fn xor20(a: &[u8; 20], b: &[u8; 20]) -> [u8; 20] {
    let mut out = [0u8; 20];
    for i in 0..20 {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// Derive the two RC4 stream states (encrypt, decrypt) from the shared secret.
///
/// Matches libtorrent `init_pe_rc4_handler`:
/// * outgoing: encrypt = `hash("keyA", S, SKEY)`, decrypt = `hash("keyB", S, SKEY)`
/// * incoming: encrypt = `hash("keyB", S, SKEY)`, decrypt = `hash("keyA", S, SKEY)`
///
/// Both states discard the first 1024 keystream bytes.
fn derive_keys(secret: &[u8], skey: &[u8], outgoing: bool) -> (Rc4, Rc4) {
    let (enc_key, dec_key) = if outgoing {
        (
            sha1(&[&b"keyA"[..], secret, skey]),
            sha1(&[&b"keyB"[..], secret, skey]),
        )
    } else {
        (
            sha1(&[&b"keyB"[..], secret, skey]),
            sha1(&[&b"keyA"[..], secret, skey]),
        )
    };

    let mut encrypt = Rc4::new(&enc_key);
    encrypt.discard(1024);
    let mut decrypt = Rc4::new(&dec_key);
    decrypt.discard(1024);

    (encrypt, decrypt)
}

/// Read bytes one at a time until the trailing `needle.len()` bytes equal
/// `needle`. Returns the number of bytes read *before* `needle` (i.e. the pad
/// length). The bytes of `needle` itself are consumed.
async fn read_scan_for_needle<R: AsyncRead + Unpin>(
    read: &mut R,
    needle: &[u8],
    max_pad: usize,
) -> Result<usize> {
    let mut buf: Vec<u8> = Vec::with_capacity(max_pad + needle.len());
    let mut byte = [0u8; 1];
    loop {
        read.read_exact(&mut byte)
            .await
            .context("disconnected while scanning MSE handshake")?;
        buf.push(byte[0]);
        if buf.len() >= needle.len() && &buf[buf.len() - needle.len()..] == needle {
            return Ok(buf.len() - needle.len());
        }
        if buf.len() >= max_pad + needle.len() {
            bail!("MSE handshake: pattern not found within {} pad bytes", max_pad);
        }
    }
}

/// Outgoing (initiator) MSE handshake. Returns an encrypted stream pair, or a
/// plaintext fallback when the peer does not speak MSE.
pub async fn outgoing(
    mut read: BoxedRead,
    mut write: BoxedWrite,
    info_hash: &[u8; 20],
) -> Result<Outcome> {
    // 1. Send our 96-byte DH public key (plaintext), then PadA.
    let dh = Dh768::generate(&mut rand::rng());
    write.write_all(&dh.public_key_bytes()).await?;
    let pad_a_len = rand::rng().random_range(0..=MAX_PAD);
    let mut pad_a = vec![0u8; pad_a_len];
    rand::rng().fill_bytes(&mut pad_a);
    write.write_all(&pad_a).await?;

    // 2. Read the first byte to distinguish a plaintext BT handshake (pstrlen
    //    0x13) from an MSE SYNACK (top byte of the peer's DH public key).
    let mut first = [0u8; 1];
    read.read_exact(&mut first)
        .await
        .context("disconnected waiting for MSE SYNACK")?;
    if first[0] == 19 {
        let mut prefix = Vec::with_capacity(BT_HANDSHAKE_LEN);
        prefix.push(first[0]);
        let mut rest = [0u8; BT_HANDSHAKE_LEN - 1];
        read.read_exact(&mut rest).await?;
        prefix.extend_from_slice(&rest);
        return Ok(Outcome::Plaintext {
            read,
            write,
            prefix,
        });
    }

    let mut server_pub = [0u8; 96];
    server_pub[0] = first[0];
    read.read_exact(&mut server_pub[1..]).await?;

    // 3. Compute the shared secret and derive the RC4 states.
    let secret = dh
        .shared_secret(&server_pub)
        .ok_or_else(|| anyhow::anyhow!("MSE: degenerate remote DH key"))?;
    let (mut encrypt, mut decrypt) = derive_keys(&secret[..], &info_hash[..], true);

    // 4. Send sync hash and obfuscated SKEY hash (both plaintext).
    let sync_hash = sha1(&[&b"req1"[..], &secret[..]]);
    write.write_all(&sync_hash).await?;

    let skey_hash = sha1(&[&b"req2"[..], &info_hash[..]]);
    let xor_mask = sha1(&[&b"req3"[..], &secret[..]]);
    write.write_all(&xor20(&skey_hash, &xor_mask)).await?;

    // 5. Send encrypted VC + crypto_provide + len_pad_c + PadC + len_ia.
    let pad_c_len = rand::rng().random_range(0..=MAX_PAD);
    let mut vc_field = Vec::with_capacity(VC_LEN + 4 + 2 + pad_c_len + 2);
    vc_field.extend_from_slice(&[0u8; VC_LEN]);
    vc_field.extend_from_slice(&CRYPTO_RC4.to_be_bytes());
    vc_field.extend_from_slice(&(pad_c_len as u16).to_be_bytes());
    let mut pad_c = vec![0u8; pad_c_len];
    rand::rng().fill_bytes(&mut pad_c);
    vc_field.extend_from_slice(&pad_c);
    vc_field.extend_from_slice(&(BT_HANDSHAKE_LEN as u16).to_be_bytes());
    encrypt.apply_keystream(&mut vc_field);
    write.write_all(&vc_field).await?;

    // 6. Read the peer's encrypted VC (scan through PadB), then
    //    crypto_select + len_pad + pad. The VC decrypts to 8 zero bytes.
    let mut pad_b: Vec<u8> = Vec::with_capacity(MAX_PAD + VC_LEN);
    let mut found_vc = false;
    while pad_b.len() < MAX_PAD + VC_LEN {
        let mut enc = [0u8; 1];
        read.read_exact(&mut enc)
            .await
            .context("disconnected waiting for MSE VC")?;
        decrypt.apply_keystream(&mut enc);
        pad_b.push(enc[0]);
        if pad_b.len() >= VC_LEN && pad_b[pad_b.len() - VC_LEN..].iter().all(|&b| b == 0) {
            found_vc = true;
            break;
        }
    }
    if !found_vc {
        bail!("MSE: verification constant not found in server response");
    }

    let mut crypto_field = [0u8; 4];
    read.read_exact(&mut crypto_field).await?;
    decrypt.apply_keystream(&mut crypto_field);
    let crypto_select = u32::from_be_bytes(crypto_field);
    if crypto_select != CRYPTO_RC4 {
        bail!("MSE: unsupported crypto_select {}", crypto_select);
    }

    let mut len_pad = [0u8; 2];
    read.read_exact(&mut len_pad).await?;
    decrypt.apply_keystream(&mut len_pad);
    let pad_d_len = u16::from_be_bytes(len_pad) as usize;
    let mut pad_d = vec![0u8; pad_d_len];
    read.read_exact(&mut pad_d).await?;
    decrypt.apply_keystream(&mut pad_d);

    Ok(Outcome::Encrypted {
        read: Box::new(Rc4Reader::new(read, decrypt)),
        write: Box::new(Rc4Writer::new(write, encrypt)),
    })
}

/// Result of an incoming handshake.
pub enum IncomingOutcome<R, W> {
    Encrypted {
        read: Rc4Reader<R>,
        write: Rc4Writer<W>,
        /// The client's decrypted BT handshake (the IA payload).
        handshake_bytes: Vec<u8>,
        /// `hash("req2", info_hash)` — the session uses this to match the
        /// torrent, since the info hash itself is obfuscated.
        skey_hash: [u8; 20],
    },
    Plaintext {
        read: R,
        write: W,
        prefix: Vec<u8>,
    },
}

/// Incoming (acceptor) MSE handshake.
///
/// `lookup` maps `hash("req2", info_hash)` back to the torrent's `info_hash`
/// (20 bytes). The session iterates its known torrents to provide this.
pub async fn incoming<R, W, F>(
    mut read: R,
    mut write: W,
    lookup: F,
) -> Result<IncomingOutcome<R, W>>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: Fn(&[u8; 20]) -> Option<[u8; 20]>,
{
    // 1. Read the peer's 96-byte DH public key, or detect a plaintext handshake.
    let mut first = [0u8; 1];
    read.read_exact(&mut first)
        .await
        .context("disconnected reading MSE SYN")?;
    if first[0] == 19 {
        let mut prefix = Vec::with_capacity(BT_HANDSHAKE_LEN);
        prefix.push(first[0]);
        let mut rest = [0u8; BT_HANDSHAKE_LEN - 1];
        read.read_exact(&mut rest).await?;
        prefix.extend_from_slice(&rest);
        return Ok(IncomingOutcome::Plaintext {
            read,
            write,
            prefix,
        });
    }

    let mut client_pub = [0u8; 96];
    client_pub[0] = first[0];
    read.read_exact(&mut client_pub[1..]).await?;

    let dh = Dh768::generate(&mut rand::rng());
    let secret = dh
        .shared_secret(&client_pub)
        .ok_or_else(|| anyhow::anyhow!("MSE: degenerate remote DH key"))?;

    // 2. Scan PadA for the sync hash.
    let sync_hash = sha1(&[&b"req1"[..], &secret[..]]);
    read_scan_for_needle(&mut read, &sync_hash[..], MAX_PAD).await?;

    // 3. Read the obfuscated SKEY hash (20 plaintext bytes).
    let mut obfusc_skey = [0u8; 20];
    read.read_exact(&mut obfusc_skey).await?;
    let xor_mask = sha1(&[&b"req3"[..], &secret[..]]);
    let skey_hash = xor20(&obfusc_skey, &xor_mask);

    // 4. Resolve the info hash from the obfuscated SKEY.
    let info_hash = lookup(&skey_hash)
        .ok_or_else(|| anyhow::anyhow!("MSE: unknown info hash in SKEY"))?;

    let (mut encrypt, mut decrypt) = derive_keys(&secret[..], &info_hash[..], false);

    // 5. Read the encrypted VC + crypto_provide + len_pad_c + PadC + len_ia.
    let mut vc = [0u8; VC_LEN];
    read.read_exact(&mut vc).await?;
    decrypt.apply_keystream(&mut vc);
    if vc.iter().any(|&b| b != 0) {
        bail!("MSE: invalid verification constant");
    }

    let mut crypto_provide = [0u8; 4];
    read.read_exact(&mut crypto_provide).await?;
    decrypt.apply_keystream(&mut crypto_provide);
    let provide = u32::from_be_bytes(crypto_provide);
    if provide & CRYPTO_RC4 == 0 {
        bail!("MSE: peer does not offer RC4 encryption");
    }

    let mut len_pad_c = [0u8; 2];
    read.read_exact(&mut len_pad_c).await?;
    decrypt.apply_keystream(&mut len_pad_c);
    let pad_c_len = u16::from_be_bytes(len_pad_c) as usize;
    let mut pad_c = vec![0u8; pad_c_len];
    read.read_exact(&mut pad_c).await?;
    decrypt.apply_keystream(&mut pad_c);

    let mut len_ia = [0u8; 2];
    read.read_exact(&mut len_ia).await?;
    decrypt.apply_keystream(&mut len_ia);
    let ia_len = u16::from_be_bytes(len_ia) as usize;
    if ia_len > BT_HANDSHAKE_LEN {
        bail!("MSE: invalid encrypted handshake length {}", ia_len);
    }

    // 6. Read the encrypted IA (start of the client's BT handshake).
    let mut handshake_bytes = vec![0u8; ia_len];
    read.read_exact(&mut handshake_bytes).await?;
    decrypt.apply_keystream(&mut handshake_bytes);

    // 7. Respond: our DH public key + PadB, then encrypted VC + crypto_select +
    //    len_pad + PadD.
    write.write_all(&dh.public_key_bytes()).await?;
    let pad_b_len = rand::rng().random_range(0..=MAX_PAD);
    let mut pad_b = vec![0u8; pad_b_len];
    rand::rng().fill_bytes(&mut pad_b);
    write.write_all(&pad_b).await?;

    let pad_d_len = rand::rng().random_range(0..=MAX_PAD);
    let mut resp = Vec::with_capacity(VC_LEN + 4 + 2 + pad_d_len);
    resp.extend_from_slice(&[0u8; VC_LEN]);
    resp.extend_from_slice(&CRYPTO_RC4.to_be_bytes());
    resp.extend_from_slice(&(pad_d_len as u16).to_be_bytes());
    let mut pad_d = vec![0u8; pad_d_len];
    rand::rng().fill_bytes(&mut pad_d);
    resp.extend_from_slice(&pad_d);
    encrypt.apply_keystream(&mut resp);
    write.write_all(&resp).await?;

    Ok(IncomingOutcome::Encrypted {
        read: Rc4Reader::new(read, decrypt),
        write: Rc4Writer::new(write, encrypt),
        handshake_bytes,
        skey_hash,
    })
}
