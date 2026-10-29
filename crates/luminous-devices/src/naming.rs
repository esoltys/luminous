//! Readable names for devices whose volume has no label.

use crate::Volume;

/// Words that describe the storage rather than the product. Removed
/// case-insensitively, so "HiBy R1 U-DISK" reads "HiBy R1" and "Generic
/// STORAGE DEVICE" leaves nothing.
const GENERIC_WORDS: &[&str] = &[
    "usb", "device", "u-disk", "udisk", "disk", "flash", "drive", "mass", "storage", "generic",
    "media", "reader", "gadget",
];

pub(crate) fn model_name(vendor: Option<&str>, model: Option<&str>) -> Option<String> {
    let vendor = vendor.map(str::trim).unwrap_or("");
    let model = model.map(str::trim).unwrap_or("");
    // Models often repeat the vendor ("SanDisk Cruzer"), and MTP devices
    // give the company's full name ("Apple Inc." + "Apple iPhone"); don't
    // say the brand twice.
    let brand = vendor
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let full = if !vendor.is_empty() && !model.to_ascii_lowercase().starts_with(&brand) {
        format!("{vendor} {model}")
    } else {
        model.to_string()
    };
    let words: Vec<&str> = full
        .split(|c: char| c.is_whitespace() || c == '_')
        .filter(|w| !w.is_empty())
        .filter(|w| !GENERIC_WORDS.contains(&w.to_ascii_lowercase().as_str()))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

pub(crate) fn default_name(volume: &Volume) -> String {
    if let Some(label) = volume
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        return label.to_string();
    }
    if let Some(name) = volume.model_name() {
        return name;
    }
    mount_name(volume)
}

/// `E:` for a Windows drive root, else the mount point's last component.
fn mount_name(volume: &Volume) -> String {
    let path = volume.mount_path.to_string_lossy();
    let trimmed = path.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        return trimmed.to_string();
    }
    volume
        .mount_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn volume(
        label: Option<&str>,
        vendor: Option<&str>,
        model: Option<&str>,
        mount: &str,
    ) -> Volume {
        Volume {
            mount_path: PathBuf::from(mount),
            label: label.map(String::from),
            filesystem: "FAT32".into(),
            serial: Some("0088-5572".into()),
            vendor: vendor.map(String::from),
            model: model.map(String::from),
            total_bytes: 0,
            free_bytes: 0,
            os_ref: String::new(),
        }
    }

    #[test]
    fn model_name_drops_generic_storage_words() {
        assert_eq!(
            model_name(Some("HiBy    "), Some("R1 U-DISK       ")).as_deref(),
            Some("HiBy R1")
        );
        assert_eq!(model_name(Some("Generic"), Some("STORAGE DEVICE")), None);
        assert_eq!(model_name(None, Some("R1_U-DISK")).as_deref(), Some("R1"));
    }

    #[test]
    fn model_name_does_not_repeat_the_vendor() {
        assert_eq!(
            model_name(Some("SanDisk"), Some("SanDisk Cruzer Blade")).as_deref(),
            Some("SanDisk Cruzer Blade")
        );
        assert_eq!(
            model_name(Some("Apple Inc."), Some("Apple iPhone")).as_deref(),
            Some("Apple iPhone")
        );
        assert_eq!(
            model_name(
                Some("SAMSUNG Electronics Co., Ltd."),
                Some("SAMSUNG_Android")
            )
            .as_deref(),
            Some("SAMSUNG Android")
        );
    }

    #[test]
    fn default_name_prefers_label_then_model_then_mount() {
        let labelled = volume(Some("WALKMAN"), Some("Sony"), Some("NW-A306"), "E:\\");
        assert_eq!(labelled.default_name(), "WALKMAN");

        let unlabelled = volume(Some("  "), Some("HiBy"), Some("R1 U-DISK"), "E:\\");
        assert_eq!(unlabelled.default_name(), "HiBy R1");

        let anonymous = volume(None, Some("Generic"), Some("STORAGE DEVICE"), "E:\\");
        assert_eq!(anonymous.default_name(), "E:");

        let linux = volume(None, None, None, "/run/media/me/0088-5572");
        assert_eq!(linux.default_name(), "0088-5572");
    }

    #[test]
    fn contains_matches_whole_path_components() {
        let (root, inside, sibling) = if cfg!(windows) {
            ("E:\\", "e:\\Music\\a.flac", "F:\\Music")
        } else {
            ("/media/R1", "/media/R1/Music/a.flac", "/media/R10/Music")
        };
        let v = volume(None, None, None, root);
        assert!(v.contains(inside));
        assert!(v.contains(root));
        assert!(!v.contains(sibling));
    }

    #[test]
    fn provisional_id_comes_from_the_serial() {
        let mut v = volume(None, None, None, "E:\\");
        v.serial = Some("0088-55ab".into());
        assert_eq!(v.provisional_id(), "vol-0088-55AB");
    }
}
