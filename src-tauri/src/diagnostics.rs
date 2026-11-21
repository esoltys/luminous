// Crash/error diagnostics capture (#684, #1261). Luminous previously had no
// persisted record of crashes: a Rust panic only printed to stderr via
// `env_logger`, invisible to a user who launched the app normally instead
// of from a terminal, and frontend JS errors weren't captured at all. This
// module writes both to a bounded log file so a bug report can carry more
// than "it crashed."

use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Each log file (`crash.log` and `crash.log.old`) is capped at 500 KB so the
/// combined on-disk footprint and exported diagnostics bundle stay within 1 MB.
const MAX_LOG_BYTES: u64 = 500_000;

/// Combined diagnostics export (header, session operations, `crash.log.old`,
/// and `crash.log`) is capped at 1 MB, keeping the newest entries if trimmed.
const MAX_EXPORT_BYTES: usize = 1_000_000;

/// Log entries older than 7 days are pruned on startup and excluded from exports.
const LOG_RETENTION_DAYS: i64 = 7;

struct RepeatTracker {
    kind: String,
    body: String,
    count: u64,
    last_logged_count: u64,
}

use std::sync::LazyLock;

/// Library scans and remote syncs kept for the diagnostics export. A slow-library
/// report (#1439) needs to show where the time went, and by the time a user exports
/// the log the interesting scan was usually the one at startup.
const MAX_OPERATION_ENTRIES: usize = 30;

/// Bounded, in-memory log of the most recent scan/sync summaries (one line each).
/// Not persisted: it describes this session only, which is what a "slow since launch"
/// report is about, and it keeps timing noise out of `crash.log`.
struct OperationLog {
    entries: Mutex<VecDeque<String>>,
}

impl OperationLog {
    const fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
        }
    }

    fn record(&self, summary: &str) {
        let mut entries = self.entries.lock();
        if entries.len() == MAX_OPERATION_ENTRIES {
            entries.pop_front();
        }
        entries.push_back(format!("[{}] {summary}", crate::clock::local_rfc3339()));
    }

    fn snapshot(&self) -> Vec<String> {
        self.entries.lock().iter().cloned().collect()
    }
}

static OPERATIONS: OperationLog = OperationLog::new();

/// Records a one-line summary of a finished library scan or remote sync so it appears
/// in the next diagnostics export.
pub fn record_operation(summary: &str) {
    OPERATIONS.record(summary);
}

static TRACKERS: LazyLock<Mutex<HashMap<PathBuf, RepeatTracker>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn log_file_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("logs").join("crash.log")
}

/// Installs a Rust panic hook that appends the panic message and a
/// backtrace to `<app_data_dir>/logs/crash.log`, in addition to running the
/// default hook (which still prints to stderr). Also prunes log entries older
/// than [`LOG_RETENTION_DAYS`] on startup. Call once, early in `run()`.
pub fn install_panic_hook(app_data_dir: PathBuf) {
    prune_stale_logs(&app_data_dir, jiff::Timestamp::now());
    let path = log_file_path(&app_data_dir);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let backtrace = std::backtrace::Backtrace::force_capture();
        append_log_entry(&path, "PANIC", &format!("{info}\n{backtrace}"));
    }));
}

/// Appends a frontend-reported JS error to the same crash log. Called from
/// the `log_frontend_error` command.
pub fn log_frontend_error(app_data_dir: &Path, message: &str, stack: Option<&str>) {
    let path = log_file_path(app_data_dir);
    let body = match stack {
        Some(s) if !s.is_empty() => format!("{message}\n{s}"),
        _ => message.to_string(),
    };
    append_log_entry(&path, "FRONTEND ERROR", &body);
}

/// Appends a WebView2-level process crash or failure to the diagnostics log.
pub fn log_webview_error(app_data_dir: &Path, message: &str) {
    let path = log_file_path(app_data_dir);
    append_log_entry(&path, "WEBVIEW ERROR", message);
}

fn raw_append_log_entry(path: &Path, kind: &str, body: &str) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_LOG_BYTES {
            let rotated = dir.join("crash.log.old");
            let _ = std::fs::rename(path, rotated);
        }
    }
    let entry = format!("[{}] {kind}: {body}\n", crate::clock::local_rfc3339());
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(mut f) => {
            let _ = f.write_all(entry.as_bytes());
        }
        Err(e) => log::error!("Failed to write diagnostics log entry: {e}"),
    }
}

/// Flushes any pending repeated entry counts to disk for the given log path.
pub fn flush_repeated_entries_for_path(path: &Path) {
    let mut trackers = TRACKERS.lock();
    if let Some(tracker) = trackers.get_mut(path) {
        if tracker.count > tracker.last_logged_count {
            let count = tracker.count;
            tracker.last_logged_count = count;
            raw_append_log_entry(
                path,
                &tracker.kind,
                &format!("(message repeated {count} times)"),
            );
        }
    }
}

/// Flushes all pending repeated entry counts to disk across all active log paths.
pub fn flush_repeated_entries() {
    let mut trackers = TRACKERS.lock();
    for (path, tracker) in trackers.iter_mut() {
        if tracker.count > tracker.last_logged_count {
            let count = tracker.count;
            tracker.last_logged_count = count;
            raw_append_log_entry(
                path,
                &tracker.kind,
                &format!("(message repeated {count} times)"),
            );
        }
    }
}

fn append_log_entry(path: &Path, kind: &str, body: &str) {
    let mut trackers = TRACKERS.lock();
    if let Some(tracker) = trackers.get_mut(path) {
        if tracker.kind == kind && tracker.body == body {
            tracker.count += 1;
            let hit_milestone = tracker.count == 10
                || tracker.count == 100
                || (tracker.count >= 1000 && tracker.count % 1000 == 0);
            if hit_milestone && tracker.count > tracker.last_logged_count {
                let count = tracker.count;
                tracker.last_logged_count = count;
                raw_append_log_entry(path, kind, &format!("(message repeated {count} times)"));
            }
            return;
        } else {
            // A different message arrived; flush previous repeats if any
            if tracker.count > tracker.last_logged_count {
                let count = tracker.count;
                raw_append_log_entry(
                    path,
                    &tracker.kind,
                    &format!("(message repeated {count} times)"),
                );
            }
        }
    }

    // New distinct entry
    trackers.insert(
        path.to_path_buf(),
        RepeatTracker {
            kind: kind.to_string(),
            body: body.to_string(),
            count: 1,
            last_logged_count: 1,
        },
    );
    raw_append_log_entry(path, kind, body);
}

struct LogEntry {
    ts: Option<jiff::Timestamp>,
    text: String,
}

fn cutoff_timestamp(now: jiff::Timestamp) -> jiff::Timestamp {
    jiff::Timestamp::from_second(now.as_second().saturating_sub(LOG_RETENTION_DAYS * 86_400))
        .unwrap_or(now)
}

fn parse_entry_timestamp(line: &str) -> Option<jiff::Timestamp> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find(']')?;
    rest[..end].parse::<jiff::Timestamp>().ok()
}

fn split_log_entries(contents: &str) -> Vec<LogEntry> {
    let mut entries: Vec<LogEntry> = Vec::new();
    for line in contents.split_inclusive('\n') {
        if let Some(ts) = parse_entry_timestamp(line) {
            entries.push(LogEntry {
                ts: Some(ts),
                text: line.to_string(),
            });
        } else if let Some(last) = entries.last_mut() {
            last.text.push_str(line);
        } else {
            entries.push(LogEntry {
                ts: None,
                text: line.to_string(),
            });
        }
    }
    entries
}

fn filter_recent_entries(contents: &str, cutoff: jiff::Timestamp) -> Vec<String> {
    split_log_entries(contents)
        .into_iter()
        .filter(|entry| entry.ts.is_some_and(|ts| ts >= cutoff))
        .map(|entry| entry.text)
        .collect()
}

/// Removes log entries older than [`LOG_RETENTION_DAYS`] from `crash.log.old`
/// and `crash.log`, deleting a file if no recent entries remain.
fn prune_stale_logs(app_data_dir: &Path, now: jiff::Timestamp) {
    let cutoff = cutoff_timestamp(now);
    let log_dir = app_data_dir.join("logs");
    for name in ["crash.log.old", "crash.log"] {
        let path = log_dir.join(name);
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let kept = filter_recent_entries(&contents, cutoff);
        let pruned: String = kept.concat();
        if pruned != contents {
            if pruned.is_empty() {
                let _ = std::fs::remove_file(&path);
            } else {
                let _ = std::fs::write(&path, pruned);
            }
        }
    }
}

/// Gathers the crash log(s) plus app/OS metadata into a single text blob
/// for `export_diagnostics` to write wherever the user picks a save path.
/// Excludes entries older than [`LOG_RETENTION_DAYS`] and caps the combined
/// bundle at [`MAX_EXPORT_BYTES`], keeping the newest entries.
pub fn build_diagnostics_bundle(app_data_dir: &Path, app_version: &str) -> String {
    build_diagnostics_bundle_at(app_data_dir, app_version, jiff::Timestamp::now())
}

fn build_diagnostics_bundle_at(
    app_data_dir: &Path,
    app_version: &str,
    now: jiff::Timestamp,
) -> String {
    let path = log_file_path(app_data_dir);
    flush_repeated_entries_for_path(&path);

    let mut out = String::new();
    out.push_str("Luminous diagnostics export\n");
    out.push_str(&format!("Generated: {}\n", crate::clock::local_rfc3339()));
    out.push_str(&format!("App version: {app_version}\n"));
    out.push_str(&format!(
        "OS: {} ({})\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    let operations = OPERATIONS.snapshot();
    if !operations.is_empty() {
        out.push_str("\n--- recent scans and syncs (this session) ---\n");
        for line in operations {
            out.push_str(&line);
            out.push('\n');
        }
    }

    let cutoff = cutoff_timestamp(now);
    let log_dir = app_data_dir.join("logs");
    let mut sections: Vec<(&str, Vec<String>)> = Vec::new();
    for name in ["crash.log.old", "crash.log"] {
        let log_path = log_dir.join(name);
        if let Ok(contents) = std::fs::read_to_string(&log_path) {
            let entries = filter_recent_entries(&contents, cutoff);
            if !entries.is_empty() {
                sections.push((name, entries));
            }
        }
    }

    if sections.is_empty() {
        out.push_str("\n(no log entries recorded yet)\n");
        return out;
    }

    const TRUNCATION_NOTICE: &str = "\n... (older entries omitted to stay within 1 MB limit)\n";
    let mut budget = MAX_EXPORT_BYTES.saturating_sub(out.len());
    let mut selected_sections: Vec<(&str, Vec<String>)> = Vec::new();
    let mut truncated = false;

    // Walk newest-to-oldest so the most recent diagnostics are preserved first.
    for (name, entries) in sections.into_iter().rev() {
        let header = format!("\n--- {name} ---\n");
        let needed_overhead = header.len() + TRUNCATION_NOTICE.len();
        if budget <= needed_overhead {
            truncated = true;
            break;
        }
        budget -= header.len();

        let mut kept: Vec<String> = Vec::new();
        for entry in entries.into_iter().rev() {
            let reserve = if truncated {
                0
            } else {
                TRUNCATION_NOTICE.len()
            };
            if entry.len() + reserve <= budget {
                budget -= entry.len();
                kept.push(entry);
            } else {
                truncated = true;
                break;
            }
        }
        if !kept.is_empty() {
            kept.reverse();
            selected_sections.push((name, kept));
        } else {
            budget += header.len();
        }
        if truncated {
            break;
        }
    }

    if truncated && out.len() + TRUNCATION_NOTICE.len() <= MAX_EXPORT_BYTES {
        out.push_str(TRUNCATION_NOTICE);
    }

    selected_sections.reverse();
    for (name, entries) in selected_sections {
        out.push_str(&format!("\n--- {name} ---\n"));
        for entry in entries {
            out.push_str(&entry);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn frontend_error_is_appended_and_readable_in_bundle() {
        let dir = tempdir().unwrap();
        log_frontend_error(dir.path(), "boom", Some("at foo.js:1"));
        let bundle = build_diagnostics_bundle(dir.path(), "1.9.0");
        assert!(bundle.contains("FRONTEND ERROR: boom"));
        assert!(bundle.contains("at foo.js:1"));
    }

    #[test]
    fn recorded_operation_appears_in_bundle() {
        let dir = tempdir().unwrap();
        record_operation("scan (requested): total 1.2s, 3140 files");
        let bundle = build_diagnostics_bundle(dir.path(), "1.9.0");
        assert!(bundle.contains("recent scans and syncs"));
        assert!(bundle.contains("scan (requested): total 1.2s, 3140 files"));
    }

    #[test]
    fn operation_log_drops_oldest_entries_beyond_its_cap() {
        let log = OperationLog::new();
        for i in 0..MAX_OPERATION_ENTRIES + 5 {
            log.record(&format!("op {i}"));
        }
        let entries = log.snapshot();
        assert_eq!(entries.len(), MAX_OPERATION_ENTRIES);
        assert!(entries.first().unwrap().ends_with("op 5"));
        assert!(entries
            .last()
            .unwrap()
            .ends_with(&format!("op {}", MAX_OPERATION_ENTRIES + 4)));
    }

    #[test]
    fn bundle_notes_absence_of_log_entries() {
        let dir = tempdir().unwrap();
        let bundle = build_diagnostics_bundle(dir.path(), "1.9.0");
        assert!(bundle.contains("no log entries recorded yet"));
    }

    #[test]
    fn oversized_log_rotates_instead_of_growing_unbounded() {
        let dir = tempdir().unwrap();
        let path = log_file_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();

        append_log_entry(&path, "FRONTEND ERROR", "after rotation");

        let rotated = path.parent().unwrap().join("crash.log.old");
        assert!(rotated.exists());
        let fresh = std::fs::read_to_string(&path).unwrap();
        assert!(fresh.contains("after rotation"));
        assert!((fresh.len() as u64) < MAX_LOG_BYTES);
    }

    #[test]
    fn burst_of_identical_frontend_errors_produces_bounded_log_lines() {
        let dir = tempdir().unwrap();
        for _ in 0..1000 {
            log_frontend_error(dir.path(), "rapid failure", Some("at render.js:42"));
        }
        let bundle = build_diagnostics_bundle(dir.path(), "1.9.0");
        let occurrences = bundle.matches("rapid failure").count();
        assert_eq!(
            occurrences, 1,
            "Full error message should only be logged once"
        );
        assert!(bundle.contains("message repeated"));
        let error_lines = bundle
            .lines()
            .filter(|l| l.contains("FRONTEND ERROR"))
            .count();
        assert!(
            error_lines <= 5,
            "Burst of 1000 identical errors should produce <= 5 log lines, got {error_lines}"
        );
    }

    #[test]
    fn different_error_flushes_pending_repeats() {
        let dir = tempdir().unwrap();
        for _ in 0..5 {
            log_frontend_error(dir.path(), "first error", None);
        }
        log_frontend_error(dir.path(), "second error", None);
        let bundle = build_diagnostics_bundle(dir.path(), "1.9.0");
        assert!(bundle.contains("FRONTEND ERROR: first error"));
        assert!(bundle.contains("FRONTEND ERROR: (message repeated 5 times)"));
        assert!(bundle.contains("FRONTEND ERROR: second error"));
    }

    #[test]
    fn bundle_excludes_entries_older_than_seven_days_including_multiline() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        let now: jiff::Timestamp = "2026-11-21T13:00:00Z".parse().unwrap();
        let old_content = "\
[2026-11-10T10:00:00Z] FRONTEND ERROR: stale error
    at old.js:1
[2026-11-14T12:59:59Z] FRONTEND ERROR: just over 7 days
[2026-11-14T13:00:00Z] FRONTEND ERROR: exact 7 day boundary
";
        let current_content = "\
[2026-11-20T09:00:00Z] PANIC: recent panic
stack line 1
stack line 2
";
        std::fs::write(log_dir.join("crash.log.old"), old_content).unwrap();
        std::fs::write(log_dir.join("crash.log"), current_content).unwrap();

        let bundle = build_diagnostics_bundle_at(dir.path(), "4.0.0", now);
        assert!(!bundle.contains("stale error"));
        assert!(!bundle.contains("at old.js:1"));
        assert!(!bundle.contains("just over 7 days"));
        assert!(bundle.contains("exact 7 day boundary"));
        assert!(bundle.contains("recent panic"));
        assert!(bundle.contains("stack line 2"));
    }

    #[test]
    fn prune_stale_logs_rewrites_and_removes_empty_files() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        let now: jiff::Timestamp = "2026-11-21T13:00:00Z".parse().unwrap();
        std::fs::write(
            log_dir.join("crash.log.old"),
            "[2026-09-27T23:29:10-07:00] FRONTEND ERROR: ancient\n",
        )
        .unwrap();
        std::fs::write(
            log_dir.join("crash.log"),
            "[2026-11-01T00:00:00Z] FRONTEND ERROR: old\n[2026-11-21T12:00:00Z] FRONTEND ERROR: fresh\n",
        )
        .unwrap();

        prune_stale_logs(dir.path(), now);

        assert!(!log_dir.join("crash.log.old").exists());
        let current = std::fs::read_to_string(log_dir.join("crash.log")).unwrap();
        assert!(!current.contains("old"));
        assert!(current.contains("fresh"));
    }

    #[test]
    fn combined_export_is_capped_at_one_megabyte_keeping_newest_entries() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        let now: jiff::Timestamp = "2026-11-21T13:00:00Z".parse().unwrap();
        let pad = "x".repeat(4_000);
        let mut old_log = String::new();
        let mut cur_log = String::new();
        // ~600 KB in crash.log.old + ~600 KB in crash.log = ~1.2 MB total
        for i in 0..150 {
            old_log.push_str(&format!(
                "[2026-11-18T12:00:00Z] FRONTEND ERROR: old-{i:03} {pad}\n"
            ));
            cur_log.push_str(&format!(
                "[2026-11-21T12:00:00Z] FRONTEND ERROR: new-{i:03} {pad}\n"
            ));
        }
        std::fs::write(log_dir.join("crash.log.old"), old_log).unwrap();
        std::fs::write(log_dir.join("crash.log"), cur_log).unwrap();

        let bundle = build_diagnostics_bundle_at(dir.path(), "4.0.0", now);
        assert!(
            bundle.len() <= MAX_EXPORT_BYTES,
            "Bundle size {} exceeded MAX_EXPORT_BYTES ({MAX_EXPORT_BYTES})",
            bundle.len()
        );
        assert!(bundle.contains("older entries omitted to stay within 1 MB limit"));
        assert!(!bundle.contains("old-000"));
        assert!(bundle.contains("new-149"));
    }
}
