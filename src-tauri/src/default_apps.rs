//! Deep link into Windows' Default Apps settings for "Make Luminous the
//! default player" (#1265).
//!
//! Windows won't let an app make itself the default handler, but
//! `ms-settings:defaultapps` accepts a query parameter (Windows 11 21H2/22H2
//! with the 2023-04 CU, and 23H2+) that opens a specific app's page instead of
//! the full list. Which parameter depends on how Luminous was installed:
//!
//! - MSIX/Store: `registeredAUMID=<AUMID>`, read at runtime because the real
//!   package identity differs from `tauri.conf.json`'s identifier.
//! - NSIS: `registeredAppUser=<name>` / `registeredAppMachine=<name>`, the
//!   `RegisteredApplications` value the installer hook writes
//!   (`windows/installer-hooks.nsh`, [`REGISTERED_APP_NAME`]).
//! - Anything else (dev builds, older installs): the plain Default Apps page.
//!
//! Builds that don't understand the parameter still land on the Default Apps
//! page, so no OS version check is needed.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// The value name the NSIS installer hook registers under
/// `Software\RegisteredApplications` — keep in sync with
/// `LUMINOUS_REGISTERED_APP` in `windows/installer-hooks.nsh`.
pub const REGISTERED_APP_NAME: &str = "Luminous";

/// How the running install is known to Windows' Default Apps page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultAppsTarget {
    /// Packaged (MSIX) install, identified by its AppUserModelID.
    Aumid(String),
    /// Registered under `HKCU\Software\RegisteredApplications`.
    RegisteredUser(String),
    /// Registered under `HKLM\Software\RegisteredApplications`.
    RegisteredMachine(String),
    /// Not registered anywhere Default Apps can look it up.
    Unregistered,
}

// Unreserved URI characters stay literal; everything else (notably the `!`
// in an AUMID and spaces) is escaped, as the Default Apps docs require.
const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// The `ms-settings:` URI that opens the Default Apps page for `target`.
pub fn default_apps_uri(target: &DefaultAppsTarget) -> String {
    let (param, value) = match target {
        DefaultAppsTarget::Aumid(v) => ("registeredAUMID", v),
        DefaultAppsTarget::RegisteredUser(v) => ("registeredAppUser", v),
        DefaultAppsTarget::RegisteredMachine(v) => ("registeredAppMachine", v),
        DefaultAppsTarget::Unregistered => return "ms-settings:defaultapps".to_string(),
    };
    format!(
        "ms-settings:defaultapps?{param}={}",
        utf8_percent_encode(value, QUERY_VALUE)
    )
}

/// Works out how Windows knows the running install: package identity first
/// (an MSIX install never runs the NSIS hook), then the installer's
/// `RegisteredApplications` entry, per-user before per-machine.
#[cfg(target_os = "windows")]
pub fn current_target() -> DefaultAppsTarget {
    if let Some(aumid) = crate::restart_manager::current_application_user_model_id() {
        return DefaultAppsTarget::Aumid(aumid);
    }

    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    if registered_application_exists(HKEY_CURRENT_USER) {
        DefaultAppsTarget::RegisteredUser(REGISTERED_APP_NAME.to_string())
    } else if registered_application_exists(HKEY_LOCAL_MACHINE) {
        DefaultAppsTarget::RegisteredMachine(REGISTERED_APP_NAME.to_string())
    } else {
        DefaultAppsTarget::Unregistered
    }
}

#[cfg(target_os = "windows")]
fn registered_application_exists(root: windows::Win32::System::Registry::HKEY) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};

    let subkey = HSTRING::from(r"Software\RegisteredApplications");
    let value = HSTRING::from(REGISTERED_APP_NAME);
    // A null data pointer just asks whether the string value exists.
    unsafe { RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, None, None) }.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unregistered_install_opens_plain_default_apps_page() {
        assert_eq!(
            default_apps_uri(&DefaultAppsTarget::Unregistered),
            "ms-settings:defaultapps"
        );
    }

    #[test]
    fn test_msix_install_escapes_the_aumid_separator() {
        let target = DefaultAppsTarget::Aumid(
            "39231EricJamesSoltys.LuminousMusicPlayer_abc123xyz!App".to_string(),
        );
        assert_eq!(
            default_apps_uri(&target),
            "ms-settings:defaultapps?registeredAUMID=39231EricJamesSoltys.LuminousMusicPlayer_abc123xyz%21App"
        );
    }

    #[test]
    fn test_nsis_install_uses_the_registered_application_name_for_its_hive() {
        assert_eq!(
            default_apps_uri(&DefaultAppsTarget::RegisteredUser("Luminous".into())),
            "ms-settings:defaultapps?registeredAppUser=Luminous"
        );
        assert_eq!(
            default_apps_uri(&DefaultAppsTarget::RegisteredMachine("My Player".into())),
            "ms-settings:defaultapps?registeredAppMachine=My%20Player"
        );
    }

    /// Every extension the bundle associates must also be declared in the
    /// installer's Capabilities\FileAssociations, or it won't be offered on
    /// Luminous's Default Apps page.
    #[test]
    fn test_installer_hook_declares_every_bundled_file_association() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let hook = include_str!("../windows/installer-hooks.nsh");
        let hook_name = format!("!define LUMINOUS_REGISTERED_APP \"{REGISTERED_APP_NAME}\"");
        assert!(hook.contains(&hook_name), "hook must register {REGISTERED_APP_NAME:?}");

        for assoc in conf["bundle"]["fileAssociations"].as_array().unwrap() {
            let class = assoc["name"].as_str().unwrap();
            for ext in assoc["ext"].as_array().unwrap() {
                let line = format!(
                    "!insertmacro LuminousDeclareFileType \".{}\" \"{class}\"",
                    ext.as_str().unwrap()
                );
                assert!(hook.contains(&line), "installer hook is missing: {line}");
            }
        }
    }
}
