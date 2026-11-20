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

/// Returns `true` if Luminous is executing inside a Linux Flatpak sandbox (#1668).
/// Probed via `/.flatpak-info` presence or the standard `FLATPAK_ID` environment variable.
#[cfg(not(windows))]
fn is_flatpak_sandbox() -> bool {
    Path::new("/.flatpak-info").exists() || std::env::var_os("FLATPAK_ID").is_some()
}

/// Constructs a `Command` targeting either the host environment (via `flatpak-spawn --host`
/// when sandboxed) or running directly on the system.
fn host_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    #[cfg(not(windows))]
    {
        if is_flatpak_sandbox() {
            let mut cmd = Command::new("flatpak-spawn");
            cmd.arg("--host").arg(program);
            return cmd;
        }
    }
    Command::new(program)
}

/// Checks if a file exists, taking Flatpak sandboxing into account.
/// When sandboxed, host paths (e.g. `/var/lib/flatpak/exports/bin/...` or user custom paths)
/// are not directly visible inside Luminous's filesystem mount, so we query `test -f` via `flatpak-spawn --host`.
fn path_is_file(path: &Path) -> bool {
    #[cfg(not(windows))]
    {
        if is_flatpak_sandbox() {
            return host_command("test")
                .arg("-f")
                .arg(path)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        }
    }
    path.is_file()
}

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
        if path_is_file(&path) {
            return Some(path);
        }
    }

    if let Ok(env_path) = std::env::var("PICARD_PATH") {
        if !env_path.trim().is_empty() {
            let path = PathBuf::from(&env_path);
            if path_is_file(&path) {
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
        if let Some(p) = which_on_path("picard") {
            return Some(p);
        }

        // picard.musicbrainz.org's own Linux install instructions lead with
        // Flatpak, which doesn't put a `picard` binary on PATH — it exports
        // a wrapper script named after the app ID instead (which itself
        // invokes `flatpak run org.musicbrainz.Picard`, so it can be spawned
        // exactly like any other executable). Check that exported name on
        // PATH first, then the well-known export directories directly in
        // case the user's PATH doesn't include them.
        if let Some(p) = which_on_path("org.musicbrainz.Picard") {
            return Some(p);
        }
        let mut flatpak_candidates = vec![PathBuf::from(
            "/var/lib/flatpak/exports/bin/org.musicbrainz.Picard",
        )];
        if let Ok(home) = std::env::var("HOME") {
            flatpak_candidates.push(PathBuf::from(format!(
                "{home}/.local/share/flatpak/exports/bin/org.musicbrainz.Picard"
            )));
        }
        flatpak_candidates.into_iter().find(|p| path_is_file(p))
    }
}

/// Parses standard stdout from `which <name>`, returning the resolved path
/// if it points to an absolute path that is not an error message.
#[cfg(not(windows))]
fn parse_which_output(stdout: &str) -> Option<PathBuf> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("which: no ") && trimmed.starts_with('/') {
            return Some(PathBuf::from(trimmed));
        }
    }
    None
}

/// Bare-name `PATH` lookup without shelling out on native systems, or
/// dispatching `which` via `flatpak-spawn --host` when running in a Flatpak sandbox.
fn which_on_path(name: &str) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        if is_flatpak_sandbox() {
            let output = host_command("which").arg(name).output().ok()?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return parse_which_output(&stdout);
            }
            return None;
        }
    }

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
    launch_picard_with(exe, paths, LAUNCH_STAGGER, spawn_chunk).map(|_| ())
}

/// Core implementation of [`launch_picard`] with an injectable spawner and
/// stagger duration so tests can verify chunking and background handoff
/// without spawning real processes or sleeping.
///
/// Returns `Ok(Some(handle))` when remaining chunks were deferred to a
/// background thread, or `Ok(None)` when everything fit in the first chunk.
fn launch_picard_with<F>(
    exe: &Path,
    paths: &[PathBuf],
    stagger: std::time::Duration,
    mut spawn: F,
) -> Result<Option<std::thread::JoinHandle<()>>>
where
    F: FnMut(&Path, &[PathBuf]) -> Result<()> + Send + 'static,
{
    if paths.is_empty() {
        return Err(anyhow!("No files to open in Picard"));
    }

    #[cfg(not(windows))]
    check_flatpak_filesystem_access(exe, paths)?;

    let mut chunks = chunk_paths_by_length(paths, MAX_COMMAND_LINE_CHARS)
        .into_iter()
        .map(|chunk| chunk.into_iter().cloned().collect::<Vec<PathBuf>>());
    let first = chunks
        .next()
        .expect("paths is non-empty, so at least one chunk exists");
    spawn(exe, &first).context("failed to launch MusicBrainz Picard")?;

    let remaining: Vec<Vec<PathBuf>> = chunks.collect();
    if remaining.is_empty() {
        return Ok(None);
    }

    let exe = exe.to_path_buf();
    let handle = std::thread::spawn(move || {
        for chunk in remaining {
            if !stagger.is_zero() {
                std::thread::sleep(stagger);
            }
            if let Err(e) = spawn(&exe, &chunk) {
                log::warn!("Failed to hand additional paths to Picard: {e}");
            }
        }
    });

    Ok(Some(handle))
}

fn spawn_chunk<P: AsRef<std::ffi::OsStr>>(exe: &Path, chunk: &[P]) -> Result<()> {
    // Picard would otherwise inherit our stdout/stderr, so its Qt chatter
    // ("QEventDispatcherWin32::wakeUp ...") lands in Luminous's log looking
    // like ours.
    host_command(exe)
        .args(chunk)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
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

/// If `exe` is a Flatpak app's exported launcher, and Picard's Flatpak
/// sandbox doesn't grant access to one of `paths`, returns an error naming
/// the blocked path and the `flatpak override` command to fix it — rather
/// than letting the launch silently no-op. Confirmed by hand: launching the
/// Flatpak build with a path outside its default grants (`home`,
/// `xdg-music`, `/tmp`) sends the LOAD message to the running instance
/// successfully (so `Command::spawn()` sees no error at all), and Picard
/// itself just logs "No such file or directory" — never surfaced to the
/// user, since Luminous doesn't watch Picard's process/output (see this
/// module's doc comment).
#[cfg(not(windows))]
fn check_flatpak_filesystem_access(exe: &Path, paths: &[PathBuf]) -> Result<()> {
    let Some(app_id) = flatpak_app_id(exe) else {
        return Ok(());
    };
    let Some(roots) = flatpak_filesystem_roots(&app_id) else {
        return Ok(()); // `flatpak info` unavailable, or grants full host access.
    };

    if let Some(blocked) = paths
        .iter()
        .find(|p| !roots.iter().any(|r| p.starts_with(r)))
    {
        return Err(anyhow!(
            "MusicBrainz Picard is installed as a Flatpak, and its sandbox doesn't have access to {}. Grant access with:\n  flatpak override --user --filesystem=\"{}\" {app_id}\nor allow it to see your whole filesystem with:\n  flatpak override --user --filesystem=host {app_id}",
            blocked.display(),
            blocked.display(),
        ));
    }
    Ok(())
}

/// Returns `exe`'s Flatpak app ID (e.g. `org.musicbrainz.Picard`) if it
/// actually resolves to an installed Flatpak app, `None` otherwise (native
/// install, or a custom path pointing outside Flatpak entirely).
#[cfg(not(windows))]
fn flatpak_app_id(exe: &Path) -> Option<String> {
    let name = exe.file_name()?.to_str()?;
    if !name.contains('.') {
        return None; // Flatpak app IDs are reverse-DNS style; a bare binary name can't be one.
    }
    let status = host_command("flatpak")
        .args(["info", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    status.success().then(|| name.to_string())
}

/// Resolves a Flatpak app's `filesystems=` sandbox grants (from `flatpak
/// info --show-permissions`) into real filesystem roots it can see. Returns
/// `None` if the app has unrestricted host access (`filesystems=host` or
/// `host:ro`/`host:rw`) or if permissions couldn't be read at all — both
/// treated as "can't tell it's blocked", so callers don't false-positive.
#[cfg(not(windows))]
fn flatpak_filesystem_roots(app_id: &str) -> Option<Vec<PathBuf>> {
    let output = host_command("flatpak")
        .args(["info", "--show-permissions", app_id])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|l| l.starts_with("filesystems="))?;
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    parse_filesystem_grants(line, home.as_deref(), &|kind, fallback| {
        xdg_user_dir(kind, fallback, home.as_deref())
    })
}

/// Turns a `filesystems=a;b;c;` line from `flatpak info --show-permissions`
/// into real filesystem roots, resolving `home`/`xdg-*` tokens against
/// `home` (via `resolve_xdg` for the XDG ones, so tests can stub it out
/// without invoking `xdg-user-dir`). Returns `None` for `host` access, same
/// as the caller.
#[cfg(not(windows))]
fn parse_filesystem_grants(
    line: &str,
    home: Option<&Path>,
    resolve_xdg: &dyn Fn(&str, &str) -> PathBuf,
) -> Option<Vec<PathBuf>> {
    let value = line.trim_start_matches("filesystems=");
    let mut roots = Vec::new();
    for token in value.split(';').map(str::trim).filter(|t| !t.is_empty()) {
        let kind = token.split(':').next().unwrap_or(token);
        match kind {
            "host" => return None,
            "home" => roots.extend(home.map(PathBuf::from)),
            "xdg-music" => roots.push(resolve_xdg("MUSIC", "Music")),
            "xdg-download" => roots.push(resolve_xdg("DOWNLOAD", "Downloads")),
            "xdg-documents" => roots.push(resolve_xdg("DOCUMENTS", "Documents")),
            "xdg-desktop" => roots.push(resolve_xdg("DESKTOP", "Desktop")),
            _ if kind.starts_with('/') => roots.push(PathBuf::from(kind)),
            _ => {} // Other portal-only grants (xdg-run, xdg-config/*, etc.) aren't music folder locations.
        }
    }
    Some(roots)
}

/// Resolves an XDG user directory (e.g. the real `~/Music`, which the user
/// may have relocated via `~/.config/user-dirs.dirs`) by asking
/// `xdg-user-dir`, falling back to `$HOME/<fallback>` if that's unavailable.
#[cfg(not(windows))]
fn xdg_user_dir(name: &str, fallback: &str, home: Option<&Path>) -> PathBuf {
    host_command("xdg-user-dir")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| home.map(|h| h.join(fallback)))
        .unwrap_or_else(|| PathBuf::from(fallback))
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

    #[cfg(not(windows))]
    #[test]
    fn parse_filesystem_grants_resolves_home_and_absolute_paths() {
        let roots = parse_filesystem_grants(
            "filesystems=home;xdg-music;/tmp;",
            Some(Path::new("/home/user")),
            &|_, fallback| PathBuf::from("/home/user").join(fallback),
        )
        .unwrap();
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/home/user"),
                PathBuf::from("/home/user/Music"),
                PathBuf::from("/tmp"),
            ]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn parse_filesystem_grants_returns_none_for_host_access() {
        let roots = parse_filesystem_grants(
            "filesystems=home;host;",
            Some(Path::new("/home/user")),
            &|_, fallback| PathBuf::from(fallback),
        );
        assert!(roots.is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn check_flatpak_filesystem_access_ignores_non_flatpak_exe() {
        let exe = std::env::current_exe().unwrap();
        let result = check_flatpak_filesystem_access(&exe, &[PathBuf::from("/var/tmp/music")]);
        assert!(result.is_ok());
    }

    #[test]
    fn launch_picard_spawns_first_chunk_inline_and_defers_rest_to_background() {
        use std::sync::{Arc, Mutex};
        use std::thread::ThreadId;

        let exe = Path::new("/fake/picard");
        let paths: Vec<PathBuf> = (0..1500)
            .map(|i| PathBuf::from(format!(r"C:\Music\Artist\Album{i:04}")))
            .collect();

        type RecordedChunks = Arc<Mutex<Vec<(ThreadId, Vec<PathBuf>)>>>;

        let caller_tid = std::thread::current().id();
        let recorded: RecordedChunks = Arc::new(Mutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded);

        let handle = launch_picard_with(
            exe,
            &paths,
            std::time::Duration::ZERO,
            move |spawned_exe, chunk| {
                assert_eq!(spawned_exe, Path::new("/fake/picard"));
                recorded_clone
                    .lock()
                    .unwrap()
                    .push((std::thread::current().id(), chunk.to_vec()));
                Ok(())
            },
        )
        .expect("launch_picard_with should succeed")
        .expect("1500 paths should spill into a background thread");

        handle.join().expect("background thread should not panic");

        let calls = recorded.lock().unwrap();
        assert!(
            calls.len() > 1,
            "expected multiple chunks, got {}",
            calls.len()
        );
        assert_eq!(
            calls[0].0, caller_tid,
            "first chunk must be spawned inline on the calling thread"
        );
        for (idx, (tid, _)) in calls.iter().enumerate().skip(1) {
            assert_ne!(
                *tid, caller_tid,
                "chunk {idx} must be deferred to a background thread"
            );
        }

        for (idx, (_, chunk)) in calls.iter().enumerate() {
            let cmd_len: usize = chunk.iter().map(|p| p.as_os_str().len() + 1).sum();
            assert!(
                cmd_len <= MAX_COMMAND_LINE_CHARS,
                "chunk {idx} length {cmd_len} exceeds MAX_COMMAND_LINE_CHARS ({MAX_COMMAND_LINE_CHARS})"
            );
        }

        let flattened: Vec<PathBuf> = calls.iter().flat_map(|(_, c)| c.iter().cloned()).collect();
        assert_eq!(flattened, paths);
    }

    #[test]
    fn launch_picard_single_chunk_does_not_spawn_background_thread() {
        use std::sync::{Arc, Mutex};

        let paths: Vec<PathBuf> = (0..5)
            .map(|i| PathBuf::from(format!("song{i}.mp3")))
            .collect();
        let count = Arc::new(Mutex::new(0usize));
        let count_clone = Arc::clone(&count);

        let handle = launch_picard_with(
            Path::new("/fake/picard"),
            &paths,
            std::time::Duration::ZERO,
            move |_, chunk| {
                assert_eq!(chunk.len(), 5);
                *count_clone.lock().unwrap() += 1;
                Ok(())
            },
        )
        .expect("single-chunk launch should succeed");

        assert!(handle.is_none(), "single chunk should not spawn a thread");
        assert_eq!(*count.lock().unwrap(), 1);
    }

    #[test]
    fn launch_picard_propagates_first_chunk_error_and_continues_on_background_errors() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let paths: Vec<PathBuf> = (0..1500)
            .map(|i| PathBuf::from(format!(r"C:\Music\Artist\Album{i:04}")))
            .collect();

        // First-chunk failure propagates immediately.
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = Arc::clone(&calls);
        let err = launch_picard_with(
            Path::new("/fake/picard"),
            &paths,
            std::time::Duration::ZERO,
            move |_, _| {
                calls_clone.fetch_add(1, Ordering::SeqCst);
                Err(anyhow!("spawn failed"))
            },
        )
        .expect_err("first-chunk failure should propagate");
        assert!(err
            .to_string()
            .contains("failed to launch MusicBrainz Picard"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Background-chunk failure logs a warning and continues spawning remaining chunks.
        let expected_chunks = chunk_paths_by_length(&paths, MAX_COMMAND_LINE_CHARS).len();
        assert!(expected_chunks >= 3, "test requires >= 3 chunks");
        let bg_calls = Arc::new(AtomicUsize::new(0));
        let bg_calls_clone = Arc::clone(&bg_calls);
        let handle = launch_picard_with(
            Path::new("/fake/picard"),
            &paths,
            std::time::Duration::ZERO,
            move |_, _| {
                let n = bg_calls_clone.fetch_add(1, Ordering::SeqCst);
                if n == 1 {
                    Err(anyhow!("transient second-chunk failure"))
                } else {
                    Ok(())
                }
            },
        )
        .expect("background failure must not fail launch_picard_with")
        .expect("expected background thread handle");

        handle.join().expect("background thread should not panic");
        assert_eq!(bg_calls.load(Ordering::SeqCst), expected_chunks);
    }

    #[cfg(not(windows))]
    #[test]
    fn parse_which_output_extracts_clean_binary_path() {
        let stdout = "/var/lib/flatpak/exports/bin/org.musicbrainz.Picard\n";
        assert_eq!(
            parse_which_output(stdout),
            Some(PathBuf::from(
                "/var/lib/flatpak/exports/bin/org.musicbrainz.Picard"
            ))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn parse_which_output_ignores_error_lines() {
        let stdout = "which: no picard in (/usr/bin:/bin)\n";
        assert_eq!(parse_which_output(stdout), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn host_command_dispatches_expected_program() {
        let cmd = host_command("test");
        let program = cmd.get_program();
        if is_flatpak_sandbox() {
            assert_eq!(program, "flatpak-spawn");
            let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
            assert_eq!(args.first().and_then(|a| a.to_str()), Some("--host"));
            assert_eq!(args.get(1).and_then(|a| a.to_str()), Some("test"));
        } else {
            assert_eq!(program, "test");
        }
    }
}
