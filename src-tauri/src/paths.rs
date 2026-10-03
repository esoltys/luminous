use std::path::{Path, PathBuf};
use tauri::{Manager, Runtime};

/// Information about where and how the application data directory is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDataDirInfo {
    pub path: PathBuf,
    pub is_portable: bool,
}

/// Detects the directory containing the running executable or package binary.
/// On Linux AppImage, `APPIMAGE` env var points to the `.AppImage` bundle,
/// whose parent is the host directory where the user placed it.
pub fn detect_executable_base_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(appimage_path) = std::env::var("APPIMAGE") {
            if let Some(parent) = Path::new(&appimage_path).parent() {
                return Some(parent.to_path_buf());
            }
        }
    }

    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// Checks if a directory is eligible to operate in portable mode.
///
/// Portable mode is considered active if:
/// 1. A file named `portable` exists in `base_dir`, OR
/// 2. A file named `luminous.portable` exists in `base_dir`, OR
/// 3. A directory named `data` exists in `base_dir`.
///
/// Sandboxed and system-managed installation locations (WindowsApps, Program Files,
/// AppData\Local\Programs, Flatpak, Snap, /usr/) are strictly excluded from portable mode.
pub fn is_eligible_portable_dir(base_dir: &Path) -> bool {
    let path_str = base_dir.to_string_lossy().to_lowercase();

    #[cfg(target_os = "windows")]
    {
        if path_str.contains("\\windowsapps\\")
            || path_str.contains("program files")
            || path_str.contains("appdata\\local\\programs")
        {
            return false;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if std::env::var("FLATPAK_ID").is_ok()
            || std::env::var("SNAP").is_ok()
            || path_str.starts_with("/usr/")
        {
            return false;
        }
    }

    // Check for explicit marker files
    if base_dir.join("portable").is_file() || base_dir.join("luminous.portable").is_file() {
        return true;
    }

    // Check for an existing data directory alongside the binary
    if base_dir.join("data").is_dir() {
        return true;
    }

    false
}

/// Resolves the app data directory and reports whether portable mode is active.
/// Honoring `LUMINOUS_DATA_DIR` so automated test runs (e.g. the Windows e2e smoke test, #779)
/// never read or write a real user's library/database — only ever set this for tests.
pub fn resolve_app_data_dir_info<R: Runtime>(app: &impl Manager<R>) -> AppDataDirInfo {
    if let Ok(dir) = std::env::var("LUMINOUS_DATA_DIR") {
        return AppDataDirInfo {
            path: PathBuf::from(dir),
            is_portable: false,
        };
    }

    if let Some(base_dir) = detect_executable_base_dir() {
        if is_eligible_portable_dir(&base_dir) {
            return AppDataDirInfo {
                path: base_dir.join("data"),
                is_portable: true,
            };
        }
    }

    AppDataDirInfo {
        path: app.path().app_data_dir().expect("no app data dir"),
        is_portable: false,
    }
}

/// Resolves the app data directory. Convenience wrapper around [`resolve_app_data_dir_info`].
pub fn resolve_app_data_dir<R: Runtime>(app: &impl Manager<R>) -> PathBuf {
    resolve_app_data_dir_info(app).path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_eligible_portable_dir_with_marker_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = temp_dir.path();

        assert!(!is_eligible_portable_dir(base));

        // Create `portable` marker file
        let marker = base.join("portable");
        std::fs::write(&marker, "").unwrap();
        assert!(is_eligible_portable_dir(base));

        std::fs::remove_file(&marker).unwrap();
        assert!(!is_eligible_portable_dir(base));

        // Create `luminous.portable` marker file
        let alt_marker = base.join("luminous.portable");
        std::fs::write(&alt_marker, "").unwrap();
        assert!(is_eligible_portable_dir(base));
    }

    #[test]
    fn test_is_eligible_portable_dir_with_data_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = temp_dir.path();

        let data_dir = base.join("data");
        std::fs::create_dir(&data_dir).unwrap();
        assert!(is_eligible_portable_dir(base));
    }

    #[test]
    fn test_is_eligible_portable_dir_ignores_system_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = temp_dir.path().join("Program Files").join("Luminous");
        std::fs::create_dir_all(&base).unwrap();

        let marker = base.join("portable");
        std::fs::write(&marker, "").unwrap();

        #[cfg(target_os = "windows")]
        assert!(!is_eligible_portable_dir(&base));
    }
}
