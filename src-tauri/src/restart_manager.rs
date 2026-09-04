//! Windows-only support for graceful Store (MSIX) update handling (#744).
//!
//! MSIX refuses to swap a package's files while any process from that
//! package is running, so a downloaded Store update stays pending until the
//! app exits — normally surfaced to the user only as a "Retry" button in the
//! Store app. This module registers Luminous with Windows' Restart Manager
//! at startup so it can be closed and relaunched automatically when an
//! update needs to apply, and provides the pure decision logic for firing a
//! one-time "app updated" notification on the next launch.

/// Registers this process with Windows' Restart Manager so it can be
/// gracefully closed and relaunched (with its original command line) when a
/// pending MSIX update needs to apply, instead of requiring the user to
/// manually quit and click Retry in the Store.
///
/// Deliberately does not pass `RESTART_NO_CRASH`/`RESTART_NO_HANG` — Restart
/// Manager terminating and relaunching the app for servicing is exactly the
/// behavior this exists for; excluding those transitions would risk Windows
/// silently declining to restart Luminous for the MSIX-update case. Fire-
/// and-forget: failures are logged, never surfaced to the UI. No matching
/// `UnregisterApplicationRestart` call is needed — process exit clears the
/// registration.
#[cfg(target_os = "windows")]
pub fn register_for_restart() {
    use windows::core::PCWSTR;
    use windows::Win32::System::Recovery::{
        RegisterApplicationRestart, RESTART_NO_PATCH, RESTART_NO_REBOOT,
    };

    let flags = RESTART_NO_PATCH | RESTART_NO_REBOOT;
    if let Err(e) = unsafe { RegisterApplicationRestart(PCWSTR::null(), flags) } {
        log::warn!("RegisterApplicationRestart failed: {e}");
    }
}

/// Whether to fire the "Luminous updated to vX.Y.Z" OS notification on this
/// launch: only for `msix` installs (the Store applies updates entirely
/// outside Luminous's own updater, so this is the only install format with
/// no other update feedback — see #409 for the self-updatable-install
/// restart-toast flow), and only when a previously stored version exists and
/// differs from the one currently running. `stored` being `None` means this
/// is the first launch ever recorded, which isn't an "update".
pub fn should_notify_update(format: &str, stored: Option<&str>, current: &str) -> bool {
    format == "msix" && stored.is_some_and(|s| s != current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notifies_when_msix_version_changed() {
        assert!(should_notify_update("msix", Some("1.0.0"), "1.1.0"));
    }

    #[test]
    fn test_does_not_notify_when_version_unchanged() {
        assert!(!should_notify_update("msix", Some("1.1.0"), "1.1.0"));
    }

    #[test]
    fn test_does_not_notify_on_first_launch() {
        assert!(!should_notify_update("msix", None, "1.1.0"));
    }

    #[test]
    fn test_does_not_notify_for_non_msix_formats() {
        assert!(!should_notify_update("windows_setup", Some("1.0.0"), "1.1.0"));
        assert!(!should_notify_update("deb", Some("1.0.0"), "1.1.0"));
    }
}
