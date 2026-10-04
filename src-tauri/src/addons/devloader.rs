//! Debug-only loader for unpacked add-on folders (#1426).
//!
//! Lets an add-on be previewed and edited live without a Store entitlement,
//! release key or production signature. Compiled out of release builds, so none
//! of this can weaken the signed-bundle path that ships.
//!
//! `LUMINOUS_ADDON_DEV_DIR` names a folder holding `<id>/manifest.json` (for
//! example `luminous-store/addins`). Assets are registered through the same
//! [`super::register`] the real installer uses, so the `luminous-addon` scheme,
//! its CSP and the sandboxed iframe behave identically. Asset hashes are not
//! checked, which keeps the edit loop free of a `gen-manifest` step.

use super::bundle::{is_valid_id, InstallError};
use super::entitlement::Provisioner;
use super::keyclient::KeyError;
use super::verifier::{Manifest, MAX_ARCHIVE_BYTES};
use super::{AddonAssets, Asset, OVERLAY_API_VERSION};
use notify::{RecursiveMode, Watcher};
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// How long to wait for an editor's burst of writes to settle before reloading.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// The folder named by `LUMINOUS_ADDON_DEV_DIR`, if set and non-empty.
pub fn dev_dir() -> Option<PathBuf> {
    std::env::var_os("LUMINOUS_ADDON_DEV_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// A manifest asset path must be a plain relative path with no way out of the folder.
fn is_plain_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// Read `<root>/<id>/manifest.json` and every asset it declares. Only declared
/// assets are loaded, symlinks that leave the folder are refused, and the total
/// is capped like a real bundle.
pub fn load_folder(root: &Path, id: &str) -> Result<(Manifest, AddonAssets), InstallError> {
    if !is_valid_id(id) {
        return Err(InstallError::InvalidId);
    }
    let io = |what: &str, e: &dyn std::fmt::Display| InstallError::Io(format!("{what}: {e}"));
    let dir = root.join(id);
    let dir = dir.canonicalize().map_err(|e| io("add-on folder", &e))?;
    let text =
        std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| io("manifest.json", &e))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| io("manifest.json", &e))?;
    if manifest.id != id {
        return Err(InstallError::Io(format!(
            "manifest id \"{}\" does not match folder \"{id}\"",
            manifest.id
        )));
    }
    if manifest.min_api_version > OVERLAY_API_VERSION {
        return Err(InstallError::UnsupportedApi(manifest.min_api_version));
    }

    let mut assets = AddonAssets::new();
    let mut total = 0usize;
    for declared in &manifest.assets {
        if !is_plain_relative(&declared.path) {
            return Err(InstallError::Io(format!(
                "unsafe asset path \"{}\"",
                declared.path
            )));
        }
        let file = dir
            .join(&declared.path)
            .canonicalize()
            .map_err(|e| io(&declared.path, &e))?;
        if !file.starts_with(&dir) {
            return Err(InstallError::Io(format!(
                "asset \"{}\" is outside the add-on folder",
                declared.path
            )));
        }
        let data = std::fs::read(&file).map_err(|e| io(&declared.path, &e))?;
        total += data.len();
        if total > MAX_ARCHIVE_BYTES {
            return Err(InstallError::Io("add-on assets are too large".into()));
        }
        assets.insert(
            declared.path.clone(),
            Asset {
                content_type: declared.content_type.clone(),
                data: Arc::new(data),
            },
        );
    }
    Ok((manifest, assets))
}

/// Load a folder and make it servable, replacing any earlier registration.
pub fn install_folder(root: &Path, id: &str) -> Result<Manifest, InstallError> {
    let (manifest, assets) = load_folder(root, id)?;
    super::register(id, assets);
    Ok(manifest)
}

/// Stands in for the key Worker and CDN: no ticket, no key, no download.
pub struct DevFolderProvisioner {
    root: PathBuf,
}

impl DevFolderProvisioner {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Provisioner for DevFolderProvisioner {
    fn service_ticket(&self) -> BoxFuture<Result<String, KeyError>> {
        Box::pin(async { Ok("dev".to_string()) })
    }

    fn release_key(&self, _: String, _: String) -> BoxFuture<Result<[u8; 32], KeyError>> {
        Box::pin(async { Ok([0u8; 32]) })
    }

    fn install(&self, id: String, _: [u8; 32]) -> BoxFuture<Result<Manifest, InstallError>> {
        let root = self.root.clone();
        Box::pin(async move { install_folder(&root, &id) })
    }
}

/// Re-register an add-on whenever a file under `<root>/<id>/` changes. A load
/// that fails (a half-saved manifest, say) is reported and the previous
/// registration keeps serving. The watcher lives on its own thread for the rest
/// of the process.
pub fn watch(
    root: PathBuf,
    on_reload: impl Fn(&str, Result<Manifest, InstallError>) + Send + 'static,
) -> notify::Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    let root = root.canonicalize().unwrap_or(root);
    std::thread::Builder::new()
        .name("addon-dev-watch".into())
        .spawn(move || {
            let _watcher = watcher;
            while let Ok(first) = rx.recv() {
                let mut ids = std::collections::BTreeSet::new();
                let mut collect = |event: notify::Result<notify::Event>| {
                    for path in event.map(|e| e.paths).unwrap_or_default() {
                        let rel = path.strip_prefix(&root).unwrap_or(&path);
                        if let Some(Component::Normal(id)) = rel.components().next() {
                            ids.insert(id.to_string_lossy().into_owned());
                        }
                    }
                };
                collect(first);
                while let Ok(next) = rx.recv_timeout(DEBOUNCE) {
                    collect(next);
                }
                for id in ids {
                    // Only reload add-ons that were already registered; an
                    // untouched folder must not appear without being acquired.
                    if super::is_registered(&id) {
                        on_reload(&id, install_folder(&root, &id));
                    }
                }
            }
        })
        .map(|_| ())
        .map_err(notify::Error::io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn manifest(id: &str, assets: &str) -> String {
        format!(
            r##"{{"schemaVersion":1,"id":"{id}","name":"T","version":"1.0.0","storeProductId":"X",
            "minApiVersion":1,"colors":{{"bg-main":"#000000"}},"overlayEntry":"overlay.html",
            "assets":[{assets}]}}"##
        )
    }

    const OVERLAY: &str = r#"{"path":"overlay.html","sha256":"00","contentType":"text/html"}"#;

    #[test]
    fn loads_declared_assets_without_checking_hashes() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "demo/manifest.json",
            &manifest("demo", OVERLAY),
        );
        write(root.path(), "demo/overlay.html", "<html></html>");
        write(root.path(), "demo/undeclared.js", "alert(1)");

        let (m, assets) = load_folder(root.path(), "demo").unwrap();
        assert_eq!(m.name, "T");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets["overlay.html"].content_type, "text/html");
    }

    #[test]
    fn rejects_id_mismatch_bad_ids_and_missing_files() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "demo/manifest.json",
            &manifest("other", OVERLAY),
        );
        write(root.path(), "demo/overlay.html", "x");
        assert!(matches!(
            load_folder(root.path(), "demo"),
            Err(InstallError::Io(_))
        ));
        assert!(matches!(
            load_folder(root.path(), "../demo"),
            Err(InstallError::InvalidId)
        ));

        write(
            root.path(),
            "gone/manifest.json",
            &manifest("gone", OVERLAY),
        );
        assert!(matches!(
            load_folder(root.path(), "gone"),
            Err(InstallError::Io(_))
        ));
    }

    #[test]
    fn rejects_paths_that_leave_the_folder() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "secret.txt", "s");
        for bad in [
            "../secret.txt",
            "/etc/passwd",
            "a\\\\b",
            "",
            "./overlay.html",
        ] {
            let asset = format!(r#"{{"path":"{bad}","sha256":"00","contentType":"text/plain"}}"#);
            write(root.path(), "demo/manifest.json", &manifest("demo", &asset));
            assert!(
                matches!(load_folder(root.path(), "demo"), Err(InstallError::Io(_))),
                "{bad} should be refused"
            );
        }
    }

    #[test]
    fn refuses_a_newer_overlay_api() {
        let root = tempfile::tempdir().unwrap();
        let text = manifest("demo", OVERLAY).replace("\"minApiVersion\":1", "\"minApiVersion\":99");
        write(root.path(), "demo/manifest.json", &text);
        write(root.path(), "demo/overlay.html", "x");
        assert!(matches!(
            load_folder(root.path(), "demo"),
            Err(InstallError::UnsupportedApi(99))
        ));
    }
}
