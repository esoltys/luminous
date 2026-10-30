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
