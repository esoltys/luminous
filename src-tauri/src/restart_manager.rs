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

/// Reads this process's own AppUserModelID via `GetCurrentApplicationUserModelId`
/// — the same identifier Windows registered for it when the MSIX package was
/// installed (`{PackageFamilyName}!{ApplicationId}`). `tauri-plugin-notification`
/// instead hardcodes the Tauri config's `identifier` string as the toast's
/// AUMID, which Windows doesn't recognize as a real registered app for an
/// MSIX install, so toasts sent through it are silently dropped there (#744).
/// Returns `None` if the call fails, e.g. because the process isn't running
/// with package identity (not an MSIX/APPX install).
#[cfg(target_os = "windows")]
pub(crate) fn current_application_user_model_id() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows::Win32::Storage::Packaging::Appx::GetCurrentApplicationUserModelId;

    unsafe {
        let mut len: u32 = 0;
        let probe = GetCurrentApplicationUserModelId(&mut len, None);
        if probe != ERROR_INSUFFICIENT_BUFFER || len == 0 {
            return None;
        }

        let mut buf: Vec<u16> = vec![0; len as usize];
        let result = GetCurrentApplicationUserModelId(&mut len, Some(PWSTR(buf.as_mut_ptr())));
        if result != ERROR_SUCCESS {
            return None;
        }

        // `len` includes the null terminator on success.
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

/// English text used when no translated template is available. Deliberately a
/// copy of the `osNotification.appUpdated` catalog entry: it is the fallback
/// for when the frontend has never run to cache a translation.
const DEFAULT_UPDATE_TEMPLATE: &str = "Luminous updated to v{version}";

/// Reads the UI-language template the frontend caches in `app_state` (see the
/// i18n store) because this toast fires at launch, before any catalog is
/// loaded. `None` when the frontend has never cached one.
pub fn load_update_template(conn: &rusqlite::Connection) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_state WHERE key = 'update_notification_template'",
        [],
        |row| row.get(0),
    )
    .ok()
}

/// Body of the "app updated" toast: `template` with `{version}` replaced by
/// `version`. A missing, blank, or placeholder-less template yields the English
/// default, so callers never handle a "no translation" case.
pub fn update_notification_body(template: Option<&str>, version: &str) -> String {
    template
        .filter(|t| t.contains("{version}"))
        .unwrap_or(DEFAULT_UPDATE_TEMPLATE)
        .replace("{version}", version)
}

/// Shows the "Luminous updated to vX.Y.Z" OS notification directly via
/// `tauri-winrt-notification` (the same crate `tauri-plugin-notification`
/// uses internally), built with this process's real AUMID rather than the
/// plugin's hardcoded, MSIX-incompatible one. Fire-and-forget: failures are
/// logged, never surfaced to the UI. `template` is the cached UI-language text
/// described on [`load_update_template`].
#[cfg(target_os = "windows")]
pub fn show_update_notification(current_version: &str, template: Option<&str>) {
    use tauri_winrt_notification::Toast;

    let Some(aumid) = current_application_user_model_id() else {
        log::warn!(
            "Could not read AppUserModelID (not running with package identity?); \
             skipping update notification"
        );
        return;
    };
    log::debug!("Resolved AUMID for update notification: {aumid:?}");

    let body = update_notification_body(template, current_version);
    match Toast::new(&aumid).title("Luminous").text1(&body).show() {
        Ok(()) => log::info!("Sent MSIX update notification (updated to v{current_version})"),
        Err(e) => log::warn!("Failed to show update notification: {e:?}"),
    }
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
    fn test_body_uses_cached_template() {
        assert_eq!(
            update_notification_body(
                Some("Luminous a été mis à jour vers la v{version}"),
                "1.2.3"
            ),
            "Luminous a été mis à jour vers la v1.2.3"
        );
    }

    #[test]
    fn test_body_falls_back_to_english() {
        let english = "Luminous updated to v1.2.3";
        assert_eq!(update_notification_body(None, "1.2.3"), english);
        assert_eq!(update_notification_body(Some(""), "1.2.3"), english);
        assert_eq!(
            update_notification_body(Some("no placeholder"), "1.2.3"),
            english
        );
    }

    #[test]
    fn test_body_replaces_every_placeholder() {
        assert_eq!(
            update_notification_body(Some("v{version} / {version}"), "2.0.0"),
            "v2.0.0 / 2.0.0"
        );
    }

    #[test]
    fn test_does_not_notify_for_non_msix_formats() {
        assert!(!should_notify_update(
            "windows_setup",
            Some("1.0.0"),
            "1.1.0"
        ));
        assert!(!should_notify_update("deb", Some("1.0.0"), "1.1.0"));
    }
}
