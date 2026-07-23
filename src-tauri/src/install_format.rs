use serde::Serialize;
use std::env;

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

pub fn detect_install_format() -> InstallFormatInfo {
    #[cfg(target_os = "linux")]
    {
        if env::var("APPIMAGE").is_ok() {
            return InstallFormatInfo {
                format: "appimage".to_string(),
                human_name: "Linux AppImage".to_string(),
                supports_self_update: true,
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

        return InstallFormatInfo {
            format: "linux_generic".to_string(),
            human_name: "Linux Executable".to_string(),
            supports_self_update: false,
        };
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = env::current_exe() {
            let path_lower = exe_path.to_string_lossy().to_lowercase();
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

        return InstallFormatInfo {
            format: "windows_setup".to_string(),
            human_name: "Windows Installer (.exe / .msi)".to_string(),
            supports_self_update: true,
        };
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
}
