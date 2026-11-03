//! Opening a file in the app the system opens its type with, for sidecar
//! files in the Files view (#1885). What may be opened at all is decided by
//! `files::openable_file`; this module only hands a file over.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

/// What happened to a file asked to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenOutcome {
    /// Handed to the app its type opens with.
    Opened,
    /// No app opens its type, so it was shown in the file manager instead.
    Revealed,
}

/// Opens `file` in its type's default app or, when there is none, shows it
/// selected in the file manager. `launch` and `reveal` do the handing over
/// (the opener plugin's `open_path` and `reveal_item_in_dir`). Fails only
/// when neither works, as for a file deleted since it was listed.
pub fn open_or_reveal(
    file: &Path,
    launch: impl FnOnce(&Path) -> Result<()>,
    reveal: impl FnOnce(&Path) -> Result<()>,
) -> Result<OpenOutcome> {
    open_or_reveal_given(has_default_app(file), file, launch, reveal)
}

fn open_or_reveal_given(
    has_app: bool,
    file: &Path,
    launch: impl FnOnce(&Path) -> Result<()>,
    reveal: impl FnOnce(&Path) -> Result<()>,
) -> Result<OpenOutcome> {
    if has_app {
        match launch(file) {
            Ok(()) => return Ok(OpenOutcome::Opened),
            Err(e) => log::warn!(
                "Couldn't open {}, showing it instead: {e:#}",
                file.display()
            ),
        }
    }
    reveal(file)?;
    Ok(OpenOutcome::Revealed)
}

/// Whether Windows has an app for `file`'s type. Asked up front because
/// opening a type with none succeeds anyway: Windows shows its own "How do
/// you want to open this file?" picker instead of failing.
#[cfg(windows)]
pub fn has_default_app(file: &Path) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::{
        AssocQueryStringW, ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_FRIENDLYAPPNAME,
    };
    let Some(extension) = file.extension() else {
        return false;
    };
    let extension = HSTRING::from(format!(".{}", extension.to_string_lossy()));
    let mut len = 0u32;
    // With no buffer this only measures the answer; there is one to measure
    // only when an app is registered.
    unsafe {
        AssocQueryStringW(
            ASSOCF_INIT_IGNOREUNKNOWN,
            ASSOCSTR_FRIENDLYAPPNAME,
            &extension,
            PCWSTR::null(),
            None,
            &mut len,
        )
    }
    .is_ok()
}

/// Elsewhere the launcher (`xdg-open`, `open`) is asked directly: it fails,
/// or the desktop offers its own choice, when nothing handles the type.
#[cfg(not(windows))]
pub fn has_default_app(_file: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::cell::Cell;

    #[test]
    fn a_file_with_an_app_is_opened_not_shown() {
        let revealed = Cell::new(false);
        let outcome = open_or_reveal_given(
            true,
            Path::new("notes.txt"),
            |_| Ok(()),
            |_| {
                revealed.set(true);
                Ok(())
            },
        );
        assert_eq!(outcome.unwrap(), OpenOutcome::Opened);
        assert!(!revealed.get());
    }

    #[test]
    fn a_file_with_no_app_is_shown_without_launching() {
        let launched = Cell::new(false);
        let outcome = open_or_reveal_given(
            false,
            Path::new("rip.cue"),
            |_| {
                launched.set(true);
                Ok(())
            },
            |_| Ok(()),
        );
        assert_eq!(outcome.unwrap(), OpenOutcome::Revealed);
        assert!(!launched.get());
    }

    #[test]
    fn a_failed_launch_falls_back_to_showing_the_file() {
        let outcome = open_or_reveal_given(
            true,
            Path::new("a.nfo"),
            |_| Err(anyhow!("no handler")),
            |_| Ok(()),
        );
        assert_eq!(outcome.unwrap(), OpenOutcome::Revealed);
    }

    #[test]
    fn fails_only_when_the_file_cannot_even_be_shown() {
        let outcome = open_or_reveal_given(
            true,
            Path::new("gone.txt"),
            |_| Err(anyhow!("not found")),
            |_| Err(anyhow!("not found")),
        );
        assert!(outcome.is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_knows_text_files_and_not_made_up_types() {
        assert!(has_default_app(Path::new("notes.txt")));
        assert!(!has_default_app(Path::new("probe.zzqx")));
        assert!(!has_default_app(Path::new("no-extension")));
    }
}
