use std::path::PathBuf;
use tauri::{Manager, Runtime};

/// Resolves the app data directory, honoring `LUMINOUS_DATA_DIR` so automated
/// test runs (e.g. the Windows e2e smoke test, #779) never read or write a
/// real user's library/database — only ever set this for tests.
pub fn resolve_app_data_dir<R: Runtime>(app: &impl Manager<R>) -> PathBuf {
    if let Ok(dir) = std::env::var("LUMINOUS_DATA_DIR") {
        return PathBuf::from(dir);
    }
    app.path().app_data_dir().expect("no app data dir")
}
