//! Comparing library file paths the way the file system does: separators
//! (`/` or `\`) and trailing ones don't matter, and on Windows neither does
//! case. Everything that asks "is this file in that folder?" or "is this the
//! same file?" about stored song paths goes through here, so a song is never
//! in a folder for one feature and outside it for another.

use std::path::Path;

/// `path` in the one form two spellings of the same location share: `/`
/// separators, no trailing one, lower-case on Windows. Use it as a map key or
/// to compare against another key; it is not a path to open.
pub fn key(path: &str) -> String {
    let p = path.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) {
        p.to_lowercase()
    } else {
        p
    }
}

/// Whether `a` and `b` name the same location.
pub fn same(a: &Path, b: &Path) -> bool {
    key(&a.to_string_lossy()) == key(&b.to_string_lossy())
}

/// Whether `file` is inside `folder`, at any depth; a folder is not inside
/// itself, `/music/a` does not contain `/music/ab/x`, and an empty folder
/// contains nothing.
pub fn is_under(file: &str, folder: &str) -> bool {
    is_under_key(&key(file), &key(folder))
}

/// [`is_under`] for paths already turned into [`key`]s, for hot loops.
pub fn is_under_key(file_key: &str, folder_key: &str) -> bool {
    !folder_key.is_empty()
        && file_key.len() > folder_key.len()
        && file_key.starts_with(folder_key)
        && file_key.as_bytes()[folder_key.len()] == b'/'
}

/// The names leading from `folder` down to `file`, spelled as `file` spells
/// them, when `file` is inside `folder` ([`is_under`]'s rule): the last is the
/// file's own name. Drive roots (`C:\`) and UNC shares (`\\srv\share`) split
/// into their components like any folder, so no name is ever empty.
pub fn segments_under(file: &str, folder: &str) -> Option<Vec<String>> {
    let components = |p: &str| -> Vec<String> {
        p.split(['/', '\\'])
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    };
    let (mut file, folder) = (components(file), components(folder));
    let inside = !folder.is_empty()
        && file.len() > folder.len()
        && folder.iter().zip(&file).all(|(a, b)| key(a) == key(b));
    inside.then(|| file.split_off(folder.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_contain_only_their_own_files() {
        assert!(is_under("/music/a/b.flac", "/music/a"));
        assert!(is_under("/music/a/b.flac", "/music/a/"));
        assert!(is_under("/music/a/x/y/b.flac", "/music/a"));
        assert!(!is_under("/music/ab/b.flac", "/music/a"));
        assert!(!is_under("/music/a", "/music/a"));
        assert!(!is_under("/music/a/b.flac", ""));
    }

    #[test]
    fn separators_and_trailing_slashes_do_not_matter() {
        assert!(is_under(r"D:\Music\a\b.flac", "D:/Music/a"));
        assert!(same(Path::new(r"D:\Music\"), Path::new("D:/Music")));
        assert_eq!(key(r"D:\Music\a\"), key("D:/Music/a"));
    }

    #[test]
    fn segments_lead_from_the_folder_to_the_file() {
        let segs = |file, folder| segments_under(file, folder);
        assert_eq!(
            segs("/music/a/b/c.flac", "/music/"),
            Some(vec!["a".into(), "b".into(), "c.flac".into()])
        );
        assert_eq!(
            segs(r"C:\Katatonia\01.flac", r"C:\"),
            Some(vec!["Katatonia".into(), "01.flac".into()])
        );
        assert_eq!(
            segs(r"\\nas\share\Music\A\01.flac", r"\\nas\share\Music"),
            Some(vec!["A".into(), "01.flac".into()])
        );
        assert_eq!(segs("/music/ab/x.flac", "/music/a"), None);
        assert_eq!(segs("/music/a", "/music/a"), None);
        assert_eq!(segs("/music/a/x.flac", ""), None);
    }

    #[cfg(windows)]
    #[test]
    fn segments_keep_the_files_own_case_on_windows() {
        assert_eq!(
            segments_under(r"d:\MUSIC\Ärzte\Ö.flac", r"D:\music"),
            Some(vec!["Ärzte".into(), "Ö.flac".into()])
        );
    }

    #[cfg(windows)]
    #[test]
    fn case_does_not_matter_on_windows() {
        assert!(is_under(
            r"d:\music\Katatonia\x.flac",
            r"D:\Music\katatonia"
        ));
        assert!(same(Path::new(r"D:\MUSIC"), Path::new(r"d:\music")));
    }

    #[cfg(not(windows))]
    #[test]
    fn case_matters_elsewhere() {
        assert!(!is_under("/Music/a/x.flac", "/music/a"));
    }
}
