// Crash/error diagnostics capture (#684). Luminous previously had no
// persisted record of crashes: a Rust panic only printed to stderr via
// `env_logger`, invisible to a user who launched the app normally instead
// of from a terminal, and frontend JS errors weren't captured at all. This
// module writes both to a bounded log file so a bug report can carry more
// than "it crashed."

use std::io::Write;
use std::path::{Path, PathBuf};

/// Crash/error log is capped at this size before rotating to `crash.log.old`,
/// so a long-running session doesn't grow the log file unbounded.
const MAX_LOG_BYTES: u64 = 1_000_000;

fn log_file_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("logs").join("crash.log")
}

/// Installs a Rust panic hook that appends the panic message and a
/// backtrace to `<app_data_dir>/logs/crash.log`, in addition to running the
/// default hook (which still prints to stderr). Call once, early in `run()`.
pub fn install_panic_hook(app_data_dir: PathBuf) {
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

fn append_log_entry(path: &Path, kind: &str, body: &str) {
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
    let entry = format!("[{}] {kind}: {body}\n", chrono::Local::now().to_rfc3339());
    match std::fs::OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            let _ = f.write_all(entry.as_bytes());
        }
        Err(e) => log::error!("Failed to write diagnostics log entry: {e}"),
    }
}

/// Gathers the crash log(s) plus app/OS metadata into a single text blob
/// for `export_diagnostics` to write wherever the user picks a save path.
pub fn build_diagnostics_bundle(app_data_dir: &Path, app_version: &str) -> String {
    let mut out = String::new();
    out.push_str("Luminous diagnostics export\n");
    out.push_str(&format!("Generated: {}\n", chrono::Local::now().to_rfc3339()));
    out.push_str(&format!("App version: {app_version}\n"));
    out.push_str(&format!(
        "OS: {} ({})\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    let log_dir = app_data_dir.join("logs");
    let mut wrote_any = false;
    for name in ["crash.log.old", "crash.log"] {
        let path = log_dir.join(name);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if !contents.is_empty() {
                out.push_str(&format!("\n--- {name} ---\n"));
                out.push_str(&contents);
                wrote_any = true;
            }
        }
    }
    if !wrote_any {
        out.push_str("\n(no log entries recorded yet)\n");
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
}
