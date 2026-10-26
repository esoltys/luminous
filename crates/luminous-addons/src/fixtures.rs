//! Embedded test fixtures for add-on bundle verification and installation tests.
//!
//! Fixtures are embedded at compile time via `include_bytes!` so unit tests are
//! completely hermetic and independent of filesystem layout, worktree location,
//! or runtime working directories (#1817).

/// Returns the raw static bytes of an embedded add-on test fixture.
///
/// # Panics
/// Panics if `name` is not a recognized fixture file.
pub(crate) fn read(name: &str) -> &'static [u8] {
    match name {
        "valid.lumadd" => include_bytes!("../tests/fixtures/addons/valid.lumadd"),
        "valid.lumadd.sig" => include_bytes!("../tests/fixtures/addons/valid.lumadd.sig"),
        "tampered-ciphertext.lumadd" => {
            include_bytes!("../tests/fixtures/addons/tampered-ciphertext.lumadd")
        }
        "tampered-header.lumadd" => {
            include_bytes!("../tests/fixtures/addons/tampered-header.lumadd")
        }
        "wrong-signer.lumadd" => include_bytes!("../tests/fixtures/addons/wrong-signer.lumadd"),
        "wrong-signer.lumadd.sig" => {
            include_bytes!("../tests/fixtures/addons/wrong-signer.lumadd.sig")
        }
        "test-addin-key.hex" => include_bytes!("../tests/fixtures/addons/test-addin-key.hex"),
        "test-public-key.hex" => include_bytes!("../tests/fixtures/addons/test-public-key.hex"),
        other => panic!("unknown add-on test fixture: {other}"),
    }
}

/// Parses a 64-character hexadecimal key fixture into a 32-byte array.
///
/// # Panics
/// Panics if the fixture is unrecognized, invalid UTF-8, or contains non-hex characters.
pub(crate) fn key32(name: &str) -> [u8; 32] {
    let text = std::str::from_utf8(read(name))
        .expect("hex key fixture is valid utf-8")
        .trim();
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).expect("valid hex byte");
    }
    out
}
