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

/// Windows caps a process's command line at 32767 UTF-16 characters
/// (`CreateProcessW`); a large "Open All in Picard" selection could still
/// blow past that as a single argument list even after the caller has
/// deduplicated songs down to their containing folders (e.g. a library
/// where the Missing Metadata playlist spans hundreds of distinct album
/// folders). Kept well under the real ceiling so per-argument quoting
/// overhead can't tip it over, and low enough to double as a sane ARG_MAX
/// margin on Linux too.
const MAX_COMMAND_LINE_CHARS: usize = 8000;

/// Gap between launches when more than one is needed (#804 follow-up).
/// Picard's Windows single-instance IPC forwards each launch's paths to the
/// already-running instance over a named pipe that gets closed and
/// recreated after every message; firing launches at it back-to-back can
/// race that recreation (dropping paths) or destabilize Picard outright —
/// observed as Picard becoming unresponsive after a burst of launches.
/// Comfortably longer than that pipe's recreation should ever take.
const LAUNCH_STAGGER: std::time::Duration = std::time::Duration::from_secs(2);

/// Launches Picard with the given file/folder paths (Picard's CLI accepts
/// one or more absolute paths as trailing arguments, and recursively scans
/// folders). Fire-and-forget — Picard is a long-lived GUI app; Luminous
/// doesn't wait on it or read its output. Picard runs as a single instance,
/// so a launch while it's already open hands its paths to that running
/// instance rather than opening a second window.
///
/// Callers should already have deduplicated to as few paths as reasonably
/// possible (e.g. per-album folders rather than per-song files, see
/// `commands::picard::open_in_picard`) — this only splits across multiple
/// launches as a last resort, when the combined argument list would
/// otherwise exceed the OS command-line length limit. The first launch
/// happens synchronously so a real failure (bad exe, permissions) surfaces
/// to the caller immediately; any further launches happen from a
/// background thread, spaced apart, and log rather than propagate errors.
pub fn launch_picard(exe: &Path, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Err(anyhow!("No files to open in Picard"));
    }

    let mut chunks = chunk_paths_by_length(paths, MAX_COMMAND_LINE_CHARS).into_iter();
    let first = chunks
        .next()
        .expect("paths is non-empty, so at least one chunk exists");
    spawn_chunk(exe, &first).context("failed to launch MusicBrainz Picard")?;

    let remaining: Vec<Vec<PathBuf>> = chunks
        .map(|chunk| chunk.into_iter().cloned().collect())
        .collect();
    if !remaining.is_empty() {
        let exe = exe.to_path_buf();
        std::thread::spawn(move || {
            for chunk in remaining {
                std::thread::sleep(LAUNCH_STAGGER);
                if let Err(e) = spawn_chunk(&exe, &chunk) {
                    log::warn!("Failed to hand additional paths to Picard: {e}");
                }
            }
        });
    }

    Ok(())
}

fn spawn_chunk<P: AsRef<std::ffi::OsStr>>(exe: &Path, chunk: &[P]) -> Result<()> {
    Command::new(exe).args(chunk).spawn()?;
    Ok(())
}

/// Groups paths into runs whose combined character count (plus one
/// separator per path) stays within `max_chars`, without ever splitting a
/// single path across chunks.
fn chunk_paths_by_length(paths: &[PathBuf], max_chars: usize) -> Vec<Vec<&PathBuf>> {
    let mut chunks: Vec<Vec<&PathBuf>> = Vec::new();
    let mut current: Vec<&PathBuf> = Vec::new();
    let mut current_len = 0;

    for path in paths {
        let len = path.as_os_str().len() + 1;
        if !current.is_empty() && current_len + len > max_chars {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }
        current_len += len;
        current.push(path);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
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

    #[test]
    fn chunk_paths_by_length_keeps_short_lists_in_one_chunk() {
        let paths: Vec<PathBuf> = (0..5)
            .map(|i| PathBuf::from(format!("song{i}.mp3")))
            .collect();
        let chunks = chunk_paths_by_length(&paths, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 5);
    }

    #[test]
    fn chunk_paths_by_length_splits_when_over_the_limit() {
        // 1500 paths of ~40 chars each (~60,000 chars total) would blow past
        // Windows' ~32K command-line limit as a single argument list (#804).
        let paths: Vec<PathBuf> = (0..1500)
            .map(|i| PathBuf::from(format!(r"C:\Music\Artist\Album\track_{i:04}.flac")))
            .collect();
        let chunks = chunk_paths_by_length(&paths, 8000);

        assert!(chunks.len() > 1);
        // No path is dropped or duplicated across chunks.
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, paths.len());
        for chunk in &chunks {
            let len: usize = chunk.iter().map(|p| p.as_os_str().len() + 1).sum();
            assert!(len <= 8000);
        }
    }

    #[test]
    fn chunk_paths_by_length_never_splits_a_single_oversized_path() {
        let huge_path = PathBuf::from("a".repeat(20_000));
        let chunks = chunk_paths_by_length(std::slice::from_ref(&huge_path), 8000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 1);
    }

    #[test]
    fn launch_picard_returns_promptly_for_a_large_selection() {
        let exe = std::env::current_exe().unwrap();
        let paths: Vec<PathBuf> = (0..1500)
            .map(|i| PathBuf::from(format!(r"C:\Music\Artist\Album{i:04}")))
            .collect();
        // `current_exe()` isn't Picard, but this checks that (a) the first
        // chunk's spawn succeeds without hitting a command-line length
        // error — it would previously fail as one oversized argument list
        // (#804) — and (b) any remaining chunks are handed off to a
        // background thread rather than staggered inline, so this returns
        // immediately rather than blocking for `LAUNCH_STAGGER` per chunk.
        let start = std::time::Instant::now();
        let result = launch_picard(&exe, &paths);
        assert!(result.is_ok());
        assert!(start.elapsed() < LAUNCH_STAGGER);
    }
}
