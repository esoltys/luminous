//! Bridge to the external MusicBrainz Picard application (#367). A one-way
//! handoff only: Luminous discovers and launches Picard with a list of file
//! paths so the user can tag them there; Luminous never calls the
//! MusicBrainz API itself (see AGENTS.md's "not best-of-all-worlds" product
//! scope — canonical tag lookup is Picard's job) and does not watch the
//! Picard process for completion. Tag edits made in Picard are picked up
//! later by the existing file-watcher/incremental scanner, same as any
//! other out-of-band edit.
//!
//! Deliberately scoped to process discovery/launch only, and kept separate
//! from any future MusicBrainz *API* integration (e.g. a Details Pane
//! pulling artist bios/images, tracked loosely under epic #677) — that is a
//! different kind of integration and should live in its own module rather
//! than being bolted onto this one.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locates the Picard executable: an optional caller-supplied custom path
/// (persisted via `set_app_setting("picard_path", ...)`) takes priority,
/// then the `PICARD_PATH` env var (dev/packaging override), then a bare
/// `picard`/`picard.exe` lookup on `PATH`, then a short list of common
/// per-OS install locations. Returns `None` if nothing is found — callers
/// turn that into a "Picard not found" message rather than a raw spawn
/// failure. macOS is not a Luminous build target, so no macOS-specific
/// discovery is included.
pub fn find_picard(custom_path: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = custom_path.filter(|p| !p.trim().is_empty()) {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Ok(env_path) = std::env::var("PICARD_PATH") {
        if !env_path.trim().is_empty() {
            let path = PathBuf::from(&env_path);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(p) = which_on_path("picard.exe") {
            return Some(p);
        }
        let candidates = [
            PathBuf::from(r"C:\Program Files\MusicBrainz Picard\picard.exe"),
            PathBuf::from(r"C:\Program Files (x86)\MusicBrainz Picard\picard.exe"),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let candidate = PathBuf::from(local).join(r"Programs\MusicBrainz Picard\picard.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        which_on_path("picard")
    }
}

/// Bare-name `PATH` lookup without shelling out — the cross-platform
/// equivalent of `which`/`where`.
fn which_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|full| full.is_file())
}

/// Launches Picard with the given file/folder paths (Picard's CLI accepts
/// one or more absolute paths as trailing arguments). Fire-and-forget —
/// Picard is a long-lived GUI app; Luminous doesn't wait on it or read its
/// output.
pub fn launch_picard(exe: &Path, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Err(anyhow!("No files to open in Picard"));
    }
    let mut cmd = Command::new(exe);
    cmd.args(paths);
    cmd.spawn().context("failed to launch MusicBrainz Picard")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_picard_prefers_custom_path_when_it_exists() {
        let exe = std::env::current_exe().unwrap();
        let found = find_picard(Some(exe.to_str().unwrap()));
        assert_eq!(found.as_deref(), Some(exe.as_path()));
    }

    #[test]
    fn find_picard_ignores_custom_path_that_does_not_exist() {
        let found = find_picard(Some("/definitely/not/a/real/path/picard"));
        // Falls through to PICARD_PATH/PATH lookup, neither of which should
        // resolve in a clean test environment.
        assert_ne!(
            found.as_deref().map(|p| p.to_string_lossy().to_string()),
            Some("/definitely/not/a/real/path/picard".to_string())
        );
    }

    #[test]
    fn launch_picard_rejects_empty_path_list() {
        let exe = std::env::current_exe().unwrap();
        let result = launch_picard(&exe, &[]);
        assert!(result.is_err());
    }
}
