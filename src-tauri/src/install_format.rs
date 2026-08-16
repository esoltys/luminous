//! Detects how this build was installed, so the frontend updater can decide
//! whether an in-app update is possible or the user must update through
//! their package manager/app store instead.
//!
//! Detection is best-effort heuristics (env vars set by AppImage/Flatpak/
//! Snap runners, and installation path conventions for system packages) —
//! there's no authoritative "installed via X" marker on any platform, so a
//! result of `linux_generic`/fallthrough just means none of the known
//! signals matched, not that detection failed.

use serde::Serialize;
use std::env;

/// `format` is a stable machine-readable id (used by `updater.test.ts` and
/// other call sites that branch on it); `human_name` is the user-facing
/// label. `supports_self_update` gates whether the frontend offers in-app
/// updates (`FoldersView.svelte`) — false for anything managed by an
/// external package manager (deb/rpm/flatpak/snap), which would just have
/// its files overwritten out from under it, or reverted, by a self-update,
/// and false for AppImage, which can't be cleanly patched in place either.
#[derive(Debug, Clone, Serialize)]
pub struct InstallFormatInfo {
    pub format: String,
    pub human_name: String,
    pub supports_self_update: bool,
}

#[tauri::command]
pub fn get_install_format() -> InstallFormatInfo {
    detect_install_format()
}

/// Checked in order: packaging-runtime env vars (AppImage/Flatpak/Snap),
/// then — on Linux only — whether the executable lives under `/usr/` with a
/// distro release file present, indicating a native deb/rpm install. On
/// Windows every path currently resolves to the same installer info (both
/// branches return identical values) since NSIS/MSI is the only shipped
/// distribution today.
pub fn detect_install_format() -> InstallFormatInfo {
    #[cfg(target_os = "linux")]
    {
        if env::var("APPIMAGE").is_ok() {
            // AppImages are monolithic binary bundles the user downloaded and
            // must manually replace; there's no external package manager to
            // clash with, but there's also no way to patch the running binary
            // in place, so this behaves like the managed-package formats below.
            return InstallFormatInfo {
                format: "appimage".to_string(),
                human_name: "Linux AppImage".to_string(),
                supports_self_update: false,
            };
        }

        if env::var("FLATPAK_ID").is_ok() || std::path::Path::new("/.flatpak-info").exists() {
            return InstallFormatInfo {
                format: "flatpak".to_string(),
                human_name: "Flatpak Package".to_string(),
                supports_self_update: false,
            };
        }

        if env::var("SNAP").is_ok() {
            return InstallFormatInfo {
                format: "snap".to_string(),
                human_name: "Snap Package".to_string(),
                supports_self_update: false,
            };
        }

        if let Ok(exe_path) = env::current_exe() {
            let path_str = exe_path.to_string_lossy();
            if path_str.starts_with("/usr/") {
                if std::path::Path::new("/etc/debian_version").exists() {
                    return InstallFormatInfo {
                        format: "deb".to_string(),
                        human_name: "Debian Package (.deb)".to_string(),
                        supports_self_update: false,
                    };
                } else if std::path::Path::new("/etc/redhat-release").exists()
                    || std::path::Path::new("/etc/fedora-release").exists()
                {
                    return InstallFormatInfo {
                        format: "rpm".to_string(),
                        human_name: "RPM Package (.rpm)".to_string(),
                        supports_self_update: false,
                    };
                }
                return InstallFormatInfo {
                    format: "system_pkg".to_string(),
                    human_name: "System Package".to_string(),
                    supports_self_update: false,
                };
            }
        }

        InstallFormatInfo {
            format: "linux_generic".to_string(),
            human_name: "Linux Executable".to_string(),
            supports_self_update: false,
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = env::current_exe() {
            let path_lower = exe_path.to_string_lossy().to_lowercase();
            // MSIX/Store installs live under `...\WindowsApps\<PackageFamilyName>_...\`,
            // a system-protected, read-only directory the in-app updater can't write to.
            // The Store owns updates for these installs, so the downloaded installer can
            // never actually take effect — without this check the app would keep seeing
            // itself as out-of-date and re-download/re-"install" the same build on every
            // launch.
            if path_lower.contains("\\windowsapps\\") {
                return InstallFormatInfo {
                    format: "msix".to_string(),
                    human_name: "Microsoft Store".to_string(),
                    supports_self_update: false,
                };
            }

            if path_lower.contains("appdata\\local\\programs")
                || path_lower.contains("program files")
            {
                return InstallFormatInfo {
                    format: "windows_setup".to_string(),
                    human_name: "Windows Installer (.exe / .msi)".to_string(),
                    supports_self_update: true,
                };
            }
        }

        InstallFormatInfo {
            format: "windows_setup".to_string(),
            human_name: "Windows Installer (.exe / .msi)".to_string(),
            supports_self_update: true,
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        InstallFormatInfo {
            format: "unknown".to_string(),
            human_name: "Desktop Application".to_string(),
            supports_self_update: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_install_format() {
        let info = detect_install_format();
        assert!(!info.format.is_empty());
        assert!(!info.human_name.is_empty());
    }

    // AppImages are standalone bundles the user must manually re-download and
    // replace; there's no way to patch the running binary in place, so this
    // must behave like the managed-package formats (deb/rpm/flatpak/snap)
    // rather than like a real in-app-updatable install (#411).
    #[cfg(target_os = "linux")]
    #[test]
    fn test_appimage_does_not_support_self_update() {
        env::set_var("APPIMAGE", "/tmp/Luminous.AppImage");
        let info = detect_install_format();
        env::remove_var("APPIMAGE");

        assert_eq!(info.format, "appimage");
        assert!(!info.supports_self_update);
    }
}
