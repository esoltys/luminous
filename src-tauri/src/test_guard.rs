//! Safety guard for test and automated runs (`cfg(test)` or `LUMINOUS_TEST=1`).
//!
//! Refuses access to the user's real profile (SQLite database, settings, caches)
//! and real music library files so tests, BDD suites, and scripted runs never
//! accidentally read from or mutate real user data.

use std::path::{Component, Path, PathBuf};

/// Kinds of safety violations refused by the test-mode guard.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SafetyViolation {
    #[error("test-mode guard refused access to real profile directory: {0}")]
    RealProfileAccess(PathBuf),
    #[error("test-mode guard refused write to real library or fixture file: {0}")]
    RealLibraryWrite(PathBuf),
}

/// Returns `true` if the current process is executing in test mode.
///
/// Test mode is active when:
/// 1. The code is compiled with `cfg(test)` (unit test builds).
/// 2. The `LUMINOUS_TEST` environment variable is set to a non-empty, non-zero value.
/// 3. The current executable is running from a Cargo test runner (`target/.../deps/...`).
pub fn is_test_mode() -> bool {
    if cfg!(test) {
        return true;
    }
    if let Some(val) = std::env::var_os("LUMINOUS_TEST") {
        if let Some(s) = val.to_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false") {
                return true;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy();
        if (exe_str.contains("/deps/") || exe_str.contains("\\deps\\"))
            && (exe_str.contains("/target/") || exe_str.contains("\\target\\"))
        {
            return true;
        }
    }
    false
}

/// Normalizes a path by resolving `.` and `..` components lexically without touching the filesystem.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = components.last() {
                    match last {
                        Component::Normal(_) => {
                            components.pop();
                        }
                        Component::RootDir => {}
                        _ => components.push(component),
                    }
                } else {
                    components.push(component);
                }
            }
            _ => components.push(component),
        }
    }
    components.into_iter().collect()
}

/// Checks whether `candidate` is equal to, or a descendant of, `parent`.
/// On Windows, comparison is case-insensitive.
pub fn is_same_or_descendant(candidate: &Path, parent: &Path) -> bool {
    let cand_norm = normalize_path(candidate);
    let parent_norm = normalize_path(parent);

    #[cfg(target_os = "windows")]
    {
        let cand_str = cand_norm.to_string_lossy().to_lowercase();
        let parent_str = parent_norm.to_string_lossy().to_lowercase();
        if cand_str == parent_str {
            return true;
        }
        let prefix = if parent_str.ends_with('\\') || parent_str.ends_with('/') {
            parent_str
        } else {
            format!("{parent_str}\\")
        };
        cand_str.starts_with(&prefix)
    }

    #[cfg(not(target_os = "windows"))]
    {
        cand_norm == parent_norm || cand_norm.starts_with(&parent_norm)
    }
}

/// Returns known default system application data and config directories for Luminous.
pub fn get_real_profile_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let app_identifiers = [
        "org.luminous.music",
        "39231EricJamesSoltys.LuminousMusicPlayer",
    ];

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            for id in &app_identifiers {
                dirs.push(PathBuf::from(&appdata).join(id));
            }
        }
        if let Some(localappdata) = std::env::var_os("LOCALAPPDATA") {
            for id in &app_identifiers {
                dirs.push(PathBuf::from(&localappdata).join(id));
            }
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let base = PathBuf::from(&userprofile).join("AppData");
            for id in &app_identifiers {
                dirs.push(base.join("Roaming").join(id));
                dirs.push(base.join("Local").join(id));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(xdg_data) = std::env::var_os("XDG_DATA_HOME") {
            for id in &app_identifiers {
                dirs.push(PathBuf::from(&xdg_data).join(id));
            }
        } else if let Some(home) = std::env::var_os("HOME") {
            let base = PathBuf::from(&home).join(".local").join("share");
            for id in &app_identifiers {
                dirs.push(base.join(id));
            }
        }

        if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
            for id in &app_identifiers {
                dirs.push(PathBuf::from(&xdg_config).join(id));
            }
        } else if let Some(home) = std::env::var_os("HOME") {
            let base = PathBuf::from(&home).join(".config");
            for id in &app_identifiers {
                dirs.push(base.join(id));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let app_support = PathBuf::from(&home)
                .join("Library")
                .join("Application Support");
            for id in &app_identifiers {
                dirs.push(app_support.join(id));
            }
        }
    }

    dirs
}

/// Returns known user music directories (e.g. `~/Music`, `%USERPROFILE%\Music`).
pub fn get_real_music_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            dirs.push(PathBuf::from(&userprofile).join("Music"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(&home).join("Music"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            let user_dirs_path = PathBuf::from(home).join(".config/user-dirs.dirs");
            if let Ok(content) = std::fs::read_to_string(&user_dirs_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("XDG_MUSIC_DIR=\"") {
                        if let Some(val) = rest.strip_suffix('\"') {
                            if let Some(sub) = val.strip_prefix("$HOME/") {
                                if let Some(h) = std::env::var_os("HOME") {
                                    dirs.push(PathBuf::from(h).join(sub));
                                }
                            } else if !val.starts_with('$') {
                                dirs.push(PathBuf::from(val));
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(&home).join("Music"));
        }
    }

    dirs
}

/// Checks whether `path` points to or inside a real application profile directory.
pub fn is_prohibited_profile_path(path: &Path) -> bool {
    let real_dirs = get_real_profile_dirs();
    is_prohibited_profile_path_against(path, &real_dirs)
}

/// Checks whether `path` points to or inside any of the given `protected_dirs`.
pub fn is_prohibited_profile_path_against(path: &Path, protected_dirs: &[PathBuf]) -> bool {
    for protected in protected_dirs {
        if is_same_or_descendant(path, protected) {
            return true;
        }
    }
    false
}

/// Checks whether `path` points to or inside a protected music library directory or source fixture tree.
pub fn is_prohibited_library_write_path(path: &Path) -> bool {
    let music_dirs = get_real_music_dirs();
    for music_dir in &music_dirs {
        if is_same_or_descendant(path, music_dir) {
            return true;
        }
    }

    // In tests, also prevent mutating test fixtures within CARGO_MANIFEST_DIR
    if let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR") {
        let manifest_dir = PathBuf::from(manifest);
        // If path is inside CARGO_MANIFEST_DIR, but NOT inside target/, reject it
        let norm_path = normalize_path(path);
        let norm_manifest = normalize_path(&manifest_dir);
        let target_dir = norm_manifest.join("target");
        if is_same_or_descendant(&norm_path, &norm_manifest)
            && !is_same_or_descendant(&norm_path, &target_dir)
        {
            return true;
        }
    }

    false
}

/// Validates that `path` is not a real user profile directory when in test mode.
///
/// Returns `Ok(())` if safe, or `Err(SafetyViolation::RealProfileAccess)` if prohibited.
pub fn check_profile_path(path: &Path) -> Result<(), SafetyViolation> {
    if !is_test_mode() {
        return Ok(());
    }
    if is_prohibited_profile_path(path) {
        let violation = SafetyViolation::RealProfileAccess(path.to_path_buf());
        log::error!("Test-mode guard triggered: {violation}");
        return Err(violation);
    }
    Ok(())
}

/// Asserts that `path` is safe to use as a profile directory.
/// Panics if the test-mode guard triggers.
pub fn assert_safe_profile_path(path: &Path) {
    if let Err(violation) = check_profile_path(path) {
        panic!("FATAL SAFETY VIOLATION: {violation}");
    }
}

/// Validates that `path` is safe to write to (not within real user music or repository fixtures)
/// when in test mode.
///
/// Returns `Ok(())` if safe, or `Err(SafetyViolation::RealLibraryWrite)` if prohibited.
pub fn check_library_write_path(path: &Path) -> Result<(), SafetyViolation> {
    if !is_test_mode() {
        return Ok(());
    }
    if is_prohibited_library_write_path(path) {
        let violation = SafetyViolation::RealLibraryWrite(path.to_path_buf());
        log::error!("Test-mode guard triggered: {violation}");
        return Err(violation);
    }
    Ok(())
}

/// Asserts that `path` is safe to write to.
/// Panics if the test-mode guard triggers.
pub fn assert_safe_library_write_path(path: &Path) {
    if let Err(violation) = check_library_write_path(path) {
        panic!("FATAL SAFETY VIOLATION: {violation}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        let p = Path::new("/a/b/../c/./d");
        assert_eq!(normalize_path(p), PathBuf::from("/a/c/d"));
    }

    #[test]
    fn test_is_same_or_descendant() {
        let parent = Path::new("/var/app/profile");
        assert!(is_same_or_descendant(parent, parent));
        assert!(is_same_or_descendant(
            Path::new("/var/app/profile/luminous.db"),
            parent
        ));
        assert!(is_same_or_descendant(
            Path::new("/var/app/profile/sub/file"),
            parent
        ));
        assert!(!is_same_or_descendant(Path::new("/var/app/other"), parent));
        assert!(!is_same_or_descendant(Path::new("/tmp/profile"), parent));
    }

    #[test]
    fn test_profile_path_prohibition() {
        let fake_profile = PathBuf::from("/home/user/.local/share/org.luminous.music");
        let protected = vec![fake_profile.clone()];

        assert!(is_prohibited_profile_path_against(
            &fake_profile,
            &protected
        ));
        assert!(is_prohibited_profile_path_against(
            &fake_profile.join("luminous.db"),
            &protected
        ));
        assert!(is_prohibited_profile_path_against(
            &fake_profile.join("covers/thumb.jpg"),
            &protected
        ));

        // Temp dirs are allowed
        let temp_dir = PathBuf::from("/tmp/luminous-test-12345");
        assert!(!is_prohibited_profile_path_against(&temp_dir, &protected));
    }

    #[test]
    fn test_assert_safe_profile_path_allows_tempdir() {
        let temp = tempfile::tempdir().unwrap();
        // Should not panic
        assert_safe_profile_path(temp.path());
        assert_safe_profile_path(&temp.path().join("luminous.db"));
    }

    #[test]
    #[should_panic(expected = "FATAL SAFETY VIOLATION")]
    fn test_assert_safe_profile_path_panics_on_real_profile() {
        let real_dirs = get_real_profile_dirs();
        if let Some(real_dir) = real_dirs.first() {
            assert_safe_profile_path(real_dir);
        } else {
            // If running in an unusual container without HOME/APPDATA, simulate
            panic!("FATAL SAFETY VIOLATION: simulated");
        }
    }
}
