//! `.lumadd` bundle verification and decryption (format v1).
//!
//! Vendored from esoltys/luminous-store `addins/verifier` (`lumadd-verify`).
//! The format is owned by that repo (`docs/ADDINS.md`); change it there first.
//! Local additions: a cap on the decrypted archive checked before decrypting,
//! a capped entry count, and the optional `capabilities` manifest field.
//! Every failure is an error: callers must treat any `Err` as "reject the bundle".

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const MAGIC: &[u8] = b"LUMADD";
const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const SIG_DOMAIN: &[u8] = b"LUMADD-SIG-v1\0";

/// Largest decrypted archive accepted (the packer enforces the same cap).
pub const MAX_ARCHIVE_BYTES: usize = 25 * 1024 * 1024;

/// Vendor Ed25519 public key (raw 32 bytes) that every shipped bundle is signed with.
pub const VENDOR_PUBLIC_KEY: [u8; 32] = [
    0xf1, 0x73, 0xd1, 0x44, 0x62, 0xc0, 0xff, 0x71, 0x77, 0xaf, 0xa0, 0xeb, 0xed, 0x23, 0x89, 0x7b,
    0x54, 0x59, 0x5f, 0xfa, 0xf7, 0x00, 0x23, 0xc7, 0xde, 0x84, 0x56, 0xfe, 0x04, 0xe8, 0x92, 0x37,
];

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Malformed(&'static str),
    TooLarge,
    BadSignature,
    DecryptFailed,
    IdMismatch,
    AssetHashMismatch(String),
    UnsafePath(String),
    UndeclaredFile(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Malformed(why) => write!(f, "malformed add-on bundle: {why}"),
            Error::TooLarge => write!(f, "add-on bundle is too large"),
            Error::BadSignature => write!(f, "add-on bundle signature is invalid"),
            Error::DecryptFailed => write!(f, "add-on bundle could not be decrypted"),
            Error::IdMismatch => write!(f, "add-on manifest id does not match its bundle"),
            Error::AssetHashMismatch(p) => {
                write!(f, "add-on asset failed its integrity check: {p}")
            }
            Error::UnsafePath(p) => write!(f, "add-on contains an unsafe path: {p}"),
            Error::UndeclaredFile(p) => write!(f, "add-on contains an undeclared file: {p}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestAsset {
    pub path: String,
    pub sha256: String,
    pub content_type: String,
}

/// Unknown fields are ignored on purpose, so a newer bundle can add optional ones.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub store_product_id: String,
    pub min_api_version: u32,
    pub colors: HashMap<String, String>,
    pub overlay_entry: Option<String>,
    pub assets: Vec<ManifestAsset>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

pub struct Addin {
    pub manifest: Manifest,
    pub files: HashMap<String, Vec<u8>>,
}

struct Parts<'a> {
    id: &'a [u8],
    header: &'a [u8],
    nonce: &'a [u8],
    ciphertext: &'a [u8],
}

fn split(bundle: &[u8]) -> Result<Parts<'_>, Error> {
    let m = MAGIC.len();
    if bundle.len() < m + 2 || &bundle[..m] != MAGIC {
        return Err(Error::Malformed("bad magic"));
    }
    if bundle[m] != VERSION {
        return Err(Error::Malformed("unsupported version"));
    }
    let id_len = bundle[m + 1] as usize;
    let end = m + 2 + id_len + NONCE_LEN;
    if id_len == 0 || bundle.len() < end + TAG_LEN {
        return Err(Error::Malformed("truncated header"));
    }
    Ok(Parts {
        id: &bundle[m + 2..m + 2 + id_len],
        header: &bundle[..end],
        nonce: &bundle[end - NONCE_LEN..end],
        ciphertext: &bundle[end..],
    })
}

/// Check the detached signature. The signed message includes the add-on id, so a bundle
/// cannot be re-labelled as a different add-on.
pub fn verify_signature(bundle: &[u8], sig: &[u8], public_key: &[u8; 32]) -> Result<(), Error> {
    let p = split(bundle)?;
    let key =
        VerifyingKey::from_bytes(public_key).map_err(|_| Error::Malformed("bad public key"))?;
    let sig = Signature::from_slice(sig).map_err(|_| Error::BadSignature)?;
    let mut msg = SIG_DOMAIN.to_vec();
    msg.push(p.id.len() as u8);
    msg.extend_from_slice(p.id);
    msg.extend_from_slice(p.header);
    msg.extend_from_slice(p.ciphertext);
    key.verify_strict(&msg, &sig)
        .map_err(|_| Error::BadSignature)
}

/// The add-on id named in the bundle header. Unauthenticated until
/// [`verify_signature`] passes; use it only to pick a key to try.
pub fn bundle_id(bundle: &[u8]) -> Result<String, Error> {
    String::from_utf8(split(bundle)?.id.to_vec()).map_err(|_| Error::Malformed("id not utf8"))
}

fn take<'a>(buf: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
    if buf.len() < n {
        return Err(Error::Malformed("truncated archive"));
    }
    let (a, b) = buf.split_at(n);
    *buf = b;
    Ok(a)
}

fn be32(b: &[u8]) -> usize {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize
}

fn is_safe_path(path: &str) -> bool {
    !(path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|s| s == ".." || s == "." || s.is_empty()))
}

/// Verify the signature, decrypt, and validate the contents. Fails closed on any mismatch.
pub fn open(
    bundle: &[u8],
    sig: &[u8],
    public_key: &[u8; 32],
    addin_key: &[u8; 32],
) -> Result<Addin, Error> {
    verify_signature(bundle, sig, public_key)?;
    let p = split(bundle)?;
    // ChaCha20-Poly1305 is length-preserving: bound the plaintext before allocating it.
    if p.ciphertext.len() - TAG_LEN > MAX_ARCHIVE_BYTES {
        return Err(Error::TooLarge);
    }
    let cipher = ChaCha20Poly1305::new(addin_key.into());
    let nonce = Nonce::try_from(p.nonce).map_err(|_| Error::Malformed("bad nonce length"))?;
    let plain = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: p.ciphertext,
                aad: p.header,
            },
        )
        .map_err(|_| Error::DecryptFailed)?;

    let mut buf = plain.as_slice();
    let count = be32(take(&mut buf, 4)?);
    // Every entry costs at least 6 bytes, so a larger count cannot be honest.
    if count > buf.len() / 6 + 1 {
        return Err(Error::Malformed("entry count exceeds archive size"));
    }
    let mut files = HashMap::new();
    let mut manifest_bytes = None;
    for i in 0..count {
        let hdr = take(&mut buf, 2)?;
        let plen = u16::from_be_bytes([hdr[0], hdr[1]]) as usize;
        let path = String::from_utf8(take(&mut buf, plen)?.to_vec())
            .map_err(|_| Error::Malformed("path not utf8"))?;
        let dlen = be32(take(&mut buf, 4)?);
        let data = take(&mut buf, dlen)?.to_vec();
        if i == 0 {
            if path != "manifest.json" {
                return Err(Error::Malformed("first entry must be manifest.json"));
            }
            manifest_bytes = Some(data);
        } else {
            if !is_safe_path(&path) {
                return Err(Error::UnsafePath(path));
            }
            files.insert(path, data);
        }
    }
    if !buf.is_empty() {
        return Err(Error::Malformed("trailing bytes"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes.ok_or(Error::Malformed("empty archive"))?)
            .map_err(|_| Error::Malformed("bad manifest"))?;
    if manifest.id.as_bytes() != p.id {
        return Err(Error::IdMismatch);
    }
    for a in &manifest.assets {
        let data = files
            .get(&a.path)
            .ok_or_else(|| Error::AssetHashMismatch(a.path.clone()))?;
        if hex_lower(&Sha256::digest(data)) != a.sha256.to_lowercase() {
            return Err(Error::AssetHashMismatch(a.path.clone()));
        }
    }
    if let Some(extra) = files
        .keys()
        .find(|k| !manifest.assets.iter().any(|a| &a.path == *k))
    {
        return Err(Error::UndeclaredFile(extra.clone()));
    }
    Ok(Addin { manifest, files })
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/addons/");

    fn read(name: &str) -> Vec<u8> {
        std::fs::read(format!("{DIR}{name}")).unwrap()
    }

    fn key32(name: &str) -> [u8; 32] {
        let text = String::from_utf8(read(name)).unwrap();
        let text = text.trim();
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    #[test]
    fn vendor_key_matches_published_hex() {
        assert_eq!(
            hex_lower(&VENDOR_PUBLIC_KEY),
            "f173d14462c0ff7177afa0ebed23897b54595ffaf70023c7de8456fe04e89237"
        );
        assert!(VerifyingKey::from_bytes(&VENDOR_PUBLIC_KEY).is_ok());
    }

    #[test]
    fn opens_valid_bundle() {
        let addin = open(
            &read("valid.lumadd"),
            &read("valid.lumadd.sig"),
            &key32("test-public-key.hex"),
            &key32("test-addin-key.hex"),
        )
        .unwrap();
        assert_eq!(addin.manifest.id, "test-addin");
        assert_eq!(addin.files.len(), addin.manifest.assets.len());
    }

    #[test]
    fn reads_bundle_id_from_header() {
        assert_eq!(bundle_id(&read("valid.lumadd")).unwrap(), "test-addin");
    }

    #[test]
    fn rejects_tampered_ciphertext_and_header() {
        for name in ["tampered-ciphertext.lumadd", "tampered-header.lumadd"] {
            let result = open(
                &read(name),
                &read("valid.lumadd.sig"),
                &key32("test-public-key.hex"),
                &key32("test-addin-key.hex"),
            );
            assert_eq!(result.err(), Some(Error::BadSignature), "{name}");
        }
    }

    #[test]
    fn rejects_wrong_signer() {
        let result = open(
            &read("wrong-signer.lumadd"),
            &read("wrong-signer.lumadd.sig"),
            &key32("test-public-key.hex"),
            &key32("test-addin-key.hex"),
        );
        assert_eq!(result.err(), Some(Error::BadSignature));
    }

    #[test]
    fn rejects_the_vendor_key_for_test_fixtures() {
        let result = open(
            &read("valid.lumadd"),
            &read("valid.lumadd.sig"),
            &VENDOR_PUBLIC_KEY,
            &key32("test-addin-key.hex"),
        );
        assert_eq!(result.err(), Some(Error::BadSignature));
    }

    #[test]
    fn rejects_wrong_addin_key() {
        let mut wrong = key32("test-addin-key.hex");
        wrong[0] ^= 0xff;
        let result = open(
            &read("valid.lumadd"),
            &read("valid.lumadd.sig"),
            &key32("test-public-key.hex"),
            &wrong,
        );
        assert_eq!(result.err(), Some(Error::DecryptFailed));
    }

    #[test]
    fn rejects_truncated_and_garbage_input() {
        let valid = read("valid.lumadd");
        let pk = key32("test-public-key.hex");
        let k = key32("test-addin-key.hex");
        let sig = read("valid.lumadd.sig");
        assert!(open(&valid[..10], &sig, &pk, &k).is_err());
        assert!(open(&valid[..valid.len() - 1], &sig, &pk, &k).is_err());
        assert!(open(b"", &sig, &pk, &k).is_err());
        assert!(open(&valid, &sig[..63], &pk, &k).is_err());
    }

    #[test]
    fn rejects_oversized_archive_before_decrypting() {
        use ed25519_dalek::{Signer, SigningKey};
        let signer = SigningKey::from_bytes(&[7u8; 32]);
        let id = b"big";
        let mut bundle = MAGIC.to_vec();
        bundle.push(VERSION);
        bundle.push(id.len() as u8);
        bundle.extend_from_slice(id);
        bundle.extend_from_slice(&[0u8; NONCE_LEN]);
        let header_len = bundle.len();
        bundle.resize(header_len + MAX_ARCHIVE_BYTES + TAG_LEN + 1, 0);
        let p = split(&bundle).unwrap();
        let mut msg = SIG_DOMAIN.to_vec();
        msg.push(id.len() as u8);
        msg.extend_from_slice(p.id);
        msg.extend_from_slice(p.header);
        msg.extend_from_slice(p.ciphertext);
        let sig = signer.sign(&msg).to_bytes();
        let public = signer.verifying_key().to_bytes();
        let result = open(&bundle, &sig, &public, &[0u8; 32]);
        assert_eq!(result.err(), Some(Error::TooLarge));
    }

    #[test]
    fn path_safety() {
        for bad in ["/a", "a/../b", "../a", "a//b", "a\\b", "./a", "a/", "a\0b"] {
            assert!(!is_safe_path(bad), "{bad}");
        }
        for good in ["overlay.html", "assets/sprite.png"] {
            assert!(is_safe_path(good), "{good}");
        }
    }

    #[test]
    fn manifest_tolerates_unknown_fields_and_defaults_capabilities() {
        let json = r##"{"schemaVersion":1,"id":"x","name":"X","version":"1.0.0",
            "storeProductId":"p","minApiVersion":1,"colors":{},"overlayEntry":null,
            "assets":[],"futureField":{"a":1}}"##;
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert!(m.capabilities.is_empty());
    }
}
