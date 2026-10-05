//! Remembering a released add-on key for 30 days (#1429).
//!
//! The key Worker is asked for an add-on key only when none is remembered or
//! the remembered one is older than [`GRACE`]. Inside that window the app skips
//! the Worker, so it is not called on every launch and an owner keeps their
//! add-on offline. The Store is still asked about ownership on every launch
//! (see `entitlement.rs`), so a refund is noticed as soon as the app is online.
//!
//! The key sits on disk only after being protected with the signed-in OS
//! user's credentials (DPAPI on Windows). Where there is no such protection,
//! nothing is written: the key then lives in memory only, as before. It is the
//! same key for every owner, so this exposes no more than reading it from
//! memory while the app runs.

use super::bundle::is_valid_id;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a remembered key stays usable without asking the Worker again.
pub const GRACE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// A released key and when the Worker last handed it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedKey {
    pub key: [u8; 32],
    pub checked_at: SystemTime,
}

impl CachedKey {
    /// Within [`GRACE`] of `now`. A key stamped in the future is not fresh, so
    /// setting the clock back cannot stretch the window.
    pub fn is_fresh(&self, now: SystemTime) -> bool {
        now.duration_since(self.checked_at)
            .is_ok_and(|age| age <= GRACE)
    }
}

/// Where remembered keys live. Failures are swallowed: a key that cannot be
/// stored or read is simply not remembered, and the online path covers it.
pub trait KeyVault: Send + Sync {
    fn load(&self, id: &str) -> Option<CachedKey>;
    fn save(&self, id: &str, key: &CachedKey);
    /// Forget the key. Forgetting one that is not there is not an error.
    fn forget(&self, id: &str);
}

/// A vault that remembers nothing.
pub struct NoVault;

impl KeyVault for NoVault {
    fn load(&self, _: &str) -> Option<CachedKey> {
        None
    }
    fn save(&self, _: &str, _: &CachedKey) {}
    fn forget(&self, _: &str) {}
}

/// Seals bytes so only the signed-in OS user can open them.
pub trait Protector: Send + Sync {
    fn protect(&self, plain: &[u8]) -> Option<Vec<u8>>;
    fn unprotect(&self, sealed: &[u8]) -> Option<Vec<u8>>;
}

/// Payload: seconds since the Unix epoch (u64, little endian) then the 32 key bytes.
const PAYLOAD_LEN: usize = 8 + 32;

/// One protected file per add-on, `<id>.key`, beside the cached bundles.
pub struct FileKeyVault {
    dir: PathBuf,
    protector: Arc<dyn Protector>,
}

impl FileKeyVault {
    pub fn new(dir: PathBuf, protector: Arc<dyn Protector>) -> Self {
        Self { dir, protector }
    }

    /// Ids end up in file names, so only the same boring alphabet the bundle
    /// cache accepts gets a path.
    fn path(&self, id: &str) -> Option<PathBuf> {
        is_valid_id(id).then(|| self.dir.join(format!("{id}.key")))
    }
}

impl KeyVault for FileKeyVault {
    fn load(&self, id: &str) -> Option<CachedKey> {
        let sealed = std::fs::read(self.path(id)?).ok()?;
        let plain = self.protector.unprotect(&sealed)?;
        if plain.len() != PAYLOAD_LEN {
            return None;
        }
        let secs = u64::from_le_bytes(plain[..8].try_into().ok()?);
        let mut key = [0u8; 32];
        key.copy_from_slice(&plain[8..]);
        Some(CachedKey {
            key,
            checked_at: UNIX_EPOCH + Duration::from_secs(secs),
        })
    }

    fn save(&self, id: &str, cached: &CachedKey) {
        let Some(path) = self.path(id) else {
            return;
        };
        let secs = cached
            .checked_at
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut plain = Vec::with_capacity(PAYLOAD_LEN);
        plain.extend_from_slice(&secs.to_le_bytes());
        plain.extend_from_slice(&cached.key);
        let Some(sealed) = self.protector.protect(&plain) else {
            return;
        };
        let tmp = path.with_extension("tmp");
        let written = std::fs::create_dir_all(&self.dir)
            .and_then(|_| std::fs::write(&tmp, sealed))
            .and_then(|_| std::fs::rename(&tmp, &path));
        if let Err(e) = written {
            log::warn!("add-on {id}: could not remember the key: {e}");
        }
    }

    fn forget(&self, id: &str) {
        if let Some(path) = self.path(id) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The vault for this build: DPAPI-protected files on Windows, nothing elsewhere.
pub fn default_vault(dir: PathBuf) -> Arc<dyn KeyVault> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(FileKeyVault::new(dir, Arc::new(dpapi::Dpapi)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = dir;
        Arc::new(NoVault)
    }
}

#[cfg(target_os = "windows")]
mod dpapi {
    use super::Protector;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// Windows Data Protection API, scoped to the current user.
    pub struct Dpapi;

    fn run(
        input: &[u8],
        op: unsafe fn(
            *const CRYPT_INTEGER_BLOB,
            *mut CRYPT_INTEGER_BLOB,
        ) -> windows::core::Result<()>,
    ) -> Option<Vec<u8>> {
        let blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(input.len()).ok()?,
            pbData: input.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        // SAFETY: `blob` points at `input` for the duration of the call, and
        // `out` is filled by the OS with a buffer we copy and then free.
        unsafe {
            op(&blob, &mut out).ok()?;
            let copied = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
            Some(copied)
        }
    }

    unsafe fn protect_op(
        input: *const CRYPT_INTEGER_BLOB,
        out: *mut CRYPT_INTEGER_BLOB,
    ) -> windows::core::Result<()> {
        CryptProtectData(
            input,
            windows::core::w!("Luminous add-on key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            out,
        )
    }

    unsafe fn unprotect_op(
        input: *const CRYPT_INTEGER_BLOB,
        out: *mut CRYPT_INTEGER_BLOB,
    ) -> windows::core::Result<()> {
        CryptUnprotectData(
            input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            out,
        )
    }

    impl Protector for Dpapi {
        fn protect(&self, plain: &[u8]) -> Option<Vec<u8>> {
            run(plain, protect_op)
        }
        fn unprotect(&self, sealed: &[u8]) -> Option<Vec<u8>> {
            run(sealed, unprotect_op)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn round_trips_and_does_not_store_the_plaintext() {
            let plain = [0x5au8; 40];
            let sealed = Dpapi.protect(&plain).expect("protect");
            assert_ne!(sealed, plain);
            assert!(!sealed.windows(plain.len()).any(|w| w == plain));
            assert_eq!(Dpapi.unprotect(&sealed).expect("unprotect"), plain);
        }

        #[test]
        fn refuses_bytes_it_did_not_seal() {
            assert_eq!(Dpapi.unprotect(b"not a dpapi blob"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reversible stand-in for the OS protection.
    struct Xor;

    impl Protector for Xor {
        fn protect(&self, plain: &[u8]) -> Option<Vec<u8>> {
            Some(plain.iter().map(|b| b ^ 0xa5).collect())
        }
        fn unprotect(&self, sealed: &[u8]) -> Option<Vec<u8>> {
            self.protect(sealed)
        }
    }

    /// Refuses to seal anything, like a platform without user-scoped protection.
    struct Refuses;

    impl Protector for Refuses {
        fn protect(&self, _: &[u8]) -> Option<Vec<u8>> {
            None
        }
        fn unprotect(&self, _: &[u8]) -> Option<Vec<u8>> {
            None
        }
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn cached(secs: u64) -> CachedKey {
        CachedKey {
            key: [9; 32],
            checked_at: at(secs),
        }
    }

    #[test]
    fn fresh_for_exactly_the_grace_period_and_not_after() {
        let c = cached(1_000);
        assert!(c.is_fresh(at(1_000)));
        assert!(c.is_fresh(at(1_000) + GRACE));
        assert!(!c.is_fresh(at(1_000) + GRACE + Duration::from_secs(1)));
    }

    #[test]
    fn a_key_stamped_in_the_future_is_not_fresh() {
        assert!(!cached(5_000).is_fresh(at(4_999)));
    }

    #[test]
    fn saves_and_loads_a_key_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileKeyVault::new(dir.path().to_path_buf(), Arc::new(Xor));
        vault.save("mothman", &cached(1_700_000_000));
        assert_eq!(vault.load("mothman"), Some(cached(1_700_000_000)));
    }

    #[test]
    fn the_key_is_not_on_disk_in_the_clear() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileKeyVault::new(dir.path().to_path_buf(), Arc::new(Xor));
        vault.save("mothman", &cached(1));
        let raw = std::fs::read(dir.path().join("mothman.key")).unwrap();
        assert!(!raw.windows(32).any(|w| w == [9u8; 32]));
    }

    #[test]
    fn nothing_is_written_when_protection_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileKeyVault::new(dir.path().to_path_buf(), Arc::new(Refuses));
        vault.save("mothman", &cached(1));
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
        assert_eq!(vault.load("mothman"), None);
    }

    #[test]
    fn forgets_and_forgetting_a_missing_key_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileKeyVault::new(dir.path().to_path_buf(), Arc::new(Xor));
        vault.forget("mothman");
        vault.save("mothman", &cached(1));
        vault.forget("mothman");
        assert_eq!(vault.load("mothman"), None);
    }

    #[test]
    fn unreadable_or_tampered_files_are_not_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileKeyVault::new(dir.path().to_path_buf(), Arc::new(Xor));
        std::fs::write(dir.path().join("mothman.key"), b"short").unwrap();
        assert_eq!(vault.load("mothman"), None);
        std::fs::write(dir.path().join("mothman.key"), [0u8; 200]).unwrap();
        assert_eq!(vault.load("mothman"), None);
    }

    #[test]
    fn ids_that_could_leave_the_folder_get_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileKeyVault::new(dir.path().join("keys"), Arc::new(Xor));
        vault.save("../escape", &cached(1));
        assert_eq!(vault.load("../escape"), None);
        assert!(!dir.path().join("escape.key").exists());
        assert!(!dir.path().join("keys").exists());
    }

    #[test]
    fn no_vault_remembers_nothing() {
        let v = NoVault;
        v.save("mothman", &cached(1));
        assert_eq!(v.load("mothman"), None);
    }
}
