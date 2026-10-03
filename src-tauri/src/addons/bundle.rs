//! Fetching, caching and installing `.lumadd` bundles (#1415).
//!
//! Bundles are public-safe ciphertext hosted at a fixed URL per add-on id. The
//! signature is the only integrity guarantee, so nothing here trusts the
//! transport: every byte goes through [`verifier::open`] before use. The
//! manifest version lives inside the signed archive, so the rollback guard
//! reads it only after verification.

use super::verifier::{self, Addin, MAX_ARCHIVE_BYTES};
use super::{AddonAssets, Asset};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where bundles are published. Files are `<base>/<id>.lumadd` and `.lumadd.sig`.
pub const DEFAULT_BASE_URL: &str = "https://luminous-addons.esoltys.dev";

/// Header, nonce and tag overhead allowed on top of the archive cap.
const MAX_BUNDLE_BYTES: usize = MAX_ARCHIVE_BYTES + 1024;
const MAX_SIG_BYTES: usize = 128;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub enum InstallError {
    InvalidId,
    /// No network and nothing cached to fall back to.
    Unreachable(String),
    Verify(verifier::Error),
    /// The bundle is older than one already installed.
    Rollback {
        installed: String,
        offered: String,
    },
    UnsupportedApi(u32),
    Io(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::InvalidId => write!(f, "invalid add-on id"),
            InstallError::Unreachable(e) => write!(f, "could not download the add-on: {e}"),
            InstallError::Verify(e) => write!(f, "{e}"),
            InstallError::Rollback { installed, offered } => write!(
                f,
                "refusing to replace add-on version {installed} with older version {offered}"
            ),
            InstallError::UnsupportedApi(v) => {
                write!(f, "add-on needs overlay API {v}; update Luminous to use it")
            }
            InstallError::Io(e) => write!(f, "add-on cache error: {e}"),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<verifier::Error> for InstallError {
    fn from(e: verifier::Error) -> Self {
        InstallError::Verify(e)
    }
}

/// Ids appear in URLs and file names, so keep them to a boring alphabet.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn bundle_urls(base: &str, id: &str) -> (String, String) {
    let base = base.trim_end_matches('/');
    (
        format!("{base}/{id}.lumadd"),
        format!("{base}/{id}.lumadd.sig"),
    )
}

/// Compare dotted numeric versions (`1.2.10` > `1.2.9`). `None` if either is not
/// purely numeric, in which case the caller must not guess.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    fn parts(v: &str) -> Option<Vec<u64>> {
        v.split('.').map(|p| p.parse().ok()).collect()
    }
    let (mut a, mut b) = (parts(a)?, parts(b)?);
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    Some(a.cmp(&b))
}

struct Cache {
    dir: PathBuf,
}

impl Cache {
    fn bundle(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.lumadd"))
    }
    fn sig(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.lumadd.sig"))
    }
    /// Highest version ever installed. Kept apart from the bundle so a
    /// downgrade is refused even after the cached bundle is replaced.
    fn version(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.version"))
    }

    fn read_pair(&self, id: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        Some((
            std::fs::read(self.bundle(id)).ok()?,
            std::fs::read(self.sig(id)).ok()?,
        ))
    }

    fn installed_version(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(self.version(id))
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn write_atomic(path: &Path, data: &[u8]) -> Result<(), InstallError> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, data)
            .and_then(|_| std::fs::rename(&tmp, path))
            .map_err(|e| InstallError::Io(e.to_string()))
    }

    fn store(
        &self,
        id: &str,
        bundle: &[u8],
        sig: &[u8],
        version: &str,
    ) -> Result<(), InstallError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| InstallError::Io(e.to_string()))?;
        Self::write_atomic(&self.bundle(id), bundle)?;
        Self::write_atomic(&self.sig(id), sig)?;
        Self::write_atomic(&self.version(id), version.as_bytes())
    }
}

async fn fetch(client: &reqwest::Client, url: &str, max: usize) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.without_url().to_string())?;
    if response.content_length().is_some_and(|n| n as usize > max) {
        return Err("response too large".into());
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("response too large".into());
    }
    Ok(bytes.to_vec())
}

async fn download(base: &str, id: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let (bundle_url, sig_url) = bundle_urls(base, id);
    let bundle = fetch(&client, &bundle_url, MAX_BUNDLE_BYTES).await?;
    let sig = fetch(&client, &sig_url, MAX_SIG_BYTES).await?;
    Ok((bundle, sig))
}

/// Verify, decrypt and guard a bundle. Pure so it can be tested without a network.
fn open_checked(
    id: &str,
    bundle: &[u8],
    sig: &[u8],
    public_key: &[u8; 32],
    addin_key: &[u8; 32],
    installed_version: Option<&str>,
) -> Result<Addin, InstallError> {
    let addin = verifier::open(bundle, sig, public_key, addin_key)?;
    // The id is authenticated by the signature; make sure it is the one asked for.
    if addin.manifest.id != id {
        return Err(verifier::Error::IdMismatch.into());
    }
    if addin.manifest.min_api_version > super::OVERLAY_API_VERSION {
        return Err(InstallError::UnsupportedApi(addin.manifest.min_api_version));
    }
    if let Some(installed) = installed_version {
        if compare_versions(&addin.manifest.version, installed) == Some(Ordering::Less) {
            return Err(InstallError::Rollback {
                installed: installed.to_string(),
                offered: addin.manifest.version.clone(),
            });
        }
    }
    Ok(addin)
}

fn to_assets(addin: &mut Addin) -> AddonAssets {
    let mut files = std::mem::take(&mut addin.files);
    addin
        .manifest
        .assets
        .iter()
        .filter_map(|a| {
            files.remove(&a.path).map(|data| {
                (
                    a.path.clone(),
                    Asset {
                        content_type: a.content_type.clone(),
                        data: std::sync::Arc::new(data),
                    },
                )
            })
        })
        .collect()
}

/// Download (or fall back to the cached copy when offline), verify, decrypt and
/// register an add-on's assets in memory. Returns the verified manifest.
///
/// A bundle that fails verification is never cached and never replaces a good
/// cached one. A network failure falls back to the cache so an owned add-on
/// keeps working offline.
pub async fn install(
    base_url: &str,
    cache_dir: &Path,
    id: &str,
    addin_key: &[u8; 32],
) -> Result<verifier::Manifest, InstallError> {
    if !is_valid_id(id) {
        return Err(InstallError::InvalidId);
    }
    let cache = Cache {
        dir: cache_dir.to_path_buf(),
    };
    let installed = cache.installed_version(id);
    let public_key = &verifier::VENDOR_PUBLIC_KEY;

    let (mut addin, fresh) = match download(base_url, id).await {
        Ok((bundle, sig)) => {
            let addin = open_checked(
                id,
                &bundle,
                &sig,
                public_key,
                addin_key,
                installed.as_deref(),
            )?;
            (addin, Some((bundle, sig)))
        }
        Err(network) => {
            let (bundle, sig) = cache
                .read_pair(id)
                .ok_or(InstallError::Unreachable(network))?;
            let addin = open_checked(
                id,
                &bundle,
                &sig,
                public_key,
                addin_key,
                installed.as_deref(),
            )?;
            (addin, None)
        }
    };

    if let Some((bundle, sig)) = fresh {
        cache.store(id, &bundle, &sig, &addin.manifest.version)?;
    }
    let assets = to_assets(&mut addin);
    super::register(id, assets);
    Ok(addin.manifest)
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

    fn open_fixture(installed: Option<&str>) -> Result<Addin, InstallError> {
        open_checked(
            "test-addin",
            &read("valid.lumadd"),
            &read("valid.lumadd.sig"),
            &key32("test-public-key.hex"),
            &key32("test-addin-key.hex"),
            installed,
        )
    }

    #[test]
    fn version_comparison_is_numeric() {
        assert_eq!(compare_versions("1.2.10", "1.2.9"), Some(Ordering::Greater));
        assert_eq!(compare_versions("1.0", "1.0.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("1.0.0", "2.0.0"), Some(Ordering::Less));
        assert_eq!(compare_versions("1.0.0-beta", "1.0.0"), None);
    }

    #[test]
    fn ids_are_restricted() {
        assert!(is_valid_id("mothman"));
        assert!(is_valid_id("a-b-1"));
        for bad in ["", "../x", "A", "a b", "a/b", "a.b"] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn urls_use_fixed_names() {
        let (b, s) = bundle_urls("https://x.test/", "mothman");
        assert_eq!(b, "https://x.test/mothman.lumadd");
        assert_eq!(s, "https://x.test/mothman.lumadd.sig");
    }

    #[test]
    fn rollback_is_refused_but_same_or_newer_is_accepted() {
        let version = open_fixture(None).unwrap().manifest.version;
        assert!(open_fixture(Some(&version)).is_ok());
        assert!(open_fixture(Some("0.0.1")).is_ok());
        match open_fixture(Some("999.0.0")) {
            Err(InstallError::Rollback { installed, offered }) => {
                assert_eq!(installed, "999.0.0");
                assert_eq!(offered, version);
            }
            other => panic!("expected rollback, got {:?}", other.err()),
        }
    }

    #[test]
    fn requested_id_must_match_the_signed_id() {
        let result = open_checked(
            "other",
            &read("valid.lumadd"),
            &read("valid.lumadd.sig"),
            &key32("test-public-key.hex"),
            &key32("test-addin-key.hex"),
            None,
        );
        assert!(matches!(
            result,
            Err(InstallError::Verify(verifier::Error::IdMismatch))
        ));
    }

    #[test]
    fn cache_round_trip_keeps_highest_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache {
            dir: dir.path().join("addons"),
        };
        assert_eq!(cache.installed_version("x"), None);
        assert!(cache.read_pair("x").is_none());
        cache.store("x", b"bundle", b"sig", "1.2.3").unwrap();
        assert_eq!(cache.installed_version("x").as_deref(), Some("1.2.3"));
        assert_eq!(
            cache.read_pair("x"),
            Some((b"bundle".to_vec(), b"sig".to_vec()))
        );
    }

    #[test]
    fn assets_carry_declared_content_types() {
        let mut addin = open_fixture(None).unwrap();
        let declared: Vec<_> = addin
            .manifest
            .assets
            .iter()
            .map(|a| (a.path.clone(), a.content_type.clone()))
            .collect();
        let assets = to_assets(&mut addin);
        assert_eq!(assets.len(), declared.len());
        for (path, content_type) in declared {
            assert_eq!(assets[&path].content_type, content_type);
        }
    }
}
