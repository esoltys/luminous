//! Reading and writing the files on one storage of a device, the same way
//! whether the storage is a mounted volume or an MTP storage (#840).

use std::io::{self, Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Suffix of the name a file is written under until it is complete.
pub(crate) const PARTIAL_SUFFIX: &str = ".luminous-partial";

/// One file or folder in a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    /// Bytes; 0 for folders.
    pub size: u64,
    /// Unix seconds, when the storage reports it.
    pub modified: Option<i64>,
}

/// Capacity of a storage, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub total_bytes: u64,
    /// Bytes available to this user.
    pub free_bytes: u64,
}

/// Called as a write progresses with the bytes written so far. Returning
/// `Break` cancels the write.
pub type Progress<'a> = &'a mut dyn FnMut(u64) -> ControlFlow<()>;

/// The files on one storage of a device.
///
/// Paths are relative to the storage root with `/` (or `\`) separators. They
/// cannot leave the storage: `..`, `.`, drive prefixes and empty parts are
/// dropped, so `"../Music//a.mp3"` is `"Music/a.mp3"` and `""` is the root.
/// Names match the device's own case rules on mounted volumes and
/// case-insensitively over MTP.
pub trait DeviceBackend: Send {
    /// The folder's entries in no particular order. A folder that does not
    /// exist lists as empty.
    fn list(&self, dir: &str) -> io::Result<Vec<DirEntry>>;

    /// The whole content of a file, `None` when there is no such file. Meant
    /// for small files: markers, manifests, playlists.
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>>;

    /// Writes `len` bytes from `source` to `path`, creating its folders and
    /// replacing any file already there. Until the write completes the file
    /// lives under a temporary name, so a failed or cancelled write, or an
    /// unplug, never leaves part of a file under `path`. A cancelled write
    /// fails with [`io::ErrorKind::Interrupted`].
    fn write(
        &self,
        path: &str,
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()>;

    /// Removes a file, or a folder and everything in it. Removing something
    /// that is not there succeeds. The root itself is never removed.
    fn remove(&self, path: &str) -> io::Result<()>;

    fn space(&self) -> io::Result<Space>;
}

/// The parts of `path` that stay on the storage (see [`DeviceBackend`]).
pub(crate) fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "." && *part != ".." && !part.ends_with(':'))
        .collect()
}

/// [`DeviceBackend`] for a mounted volume.
pub struct FsBackend {
    root: PathBuf,
}

impl FsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let mut full = self.root.clone();
        full.extend(components(path));
        full
    }
}

impl DeviceBackend for FsBackend {
    fn list(&self, dir: &str) -> io::Result<Vec<DirEntry>> {
        let entries = match std::fs::read_dir(self.resolve(dir)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut listing = Vec::new();
        for entry in entries {
            let entry = entry?;
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            listing.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64),
            });
        }
        Ok(listing)
    }

    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        let path = self.resolve(path);
        if path == self.root {
            return Ok(None);
        }
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(
        &self,
        path: &str,
        source: &mut dyn Read,
        _len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        let target = self.resolve(path);
        let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
        };
        if target == self.root {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
        }
        std::fs::create_dir_all(parent)?;
        let partial = parent.join(format!("{}{PARTIAL_SUFFIX}", name.to_string_lossy()));
        let result =
            copy_into(&partial, source, progress).and_then(|()| std::fs::rename(&partial, &target));
        if result.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        result
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        let path = self.resolve(path);
        if path == self.root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot remove the root",
            ));
        }
        let result = match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&path),
            Ok(_) => std::fs::remove_file(&path),
            Err(e) => Err(e),
        };
        match result {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    fn space(&self) -> io::Result<Space> {
        crate::platform::space(&self.root)
            .map(|(total_bytes, free_bytes)| Space {
                total_bytes,
                free_bytes,
            })
            .ok_or_else(|| io::Error::other("free space unavailable"))
    }
}

fn copy_into(path: &Path, source: &mut dyn Read, progress: Progress<'_>) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut written = 0u64;
    loop {
        let n = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        file.write_all(&buffer[..n])?;
        written += n as u64;
        if progress(written).is_break() {
            return Err(cancelled());
        }
    }
    // Data must be on the device before the rename makes it visible.
    file.sync_all()
}

pub(crate) fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "write cancelled")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(backend: &FsBackend, path: &str, text: &str) -> io::Result<()> {
        backend.write(path, &mut text.as_bytes(), text.len() as u64, &mut |_| {
            ControlFlow::Continue(())
        })
    }

    fn names(backend: &FsBackend, dir: &str) -> Vec<String> {
        let mut names: Vec<_> = backend
            .list(dir)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn write_creates_folders_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FsBackend::new(dir.path());
        write(&backend, "Music/Artist/a.mp3", "first").unwrap();
        write(&backend, "Music/Artist/a.mp3", "second").unwrap();

        assert_eq!(
            backend.read("Music/Artist/a.mp3").unwrap().as_deref(),
            Some(&b"second"[..])
        );
        assert_eq!(names(&backend, "Music/Artist"), vec!["a.mp3"]);
        let entry = &backend.list("Music").unwrap()[0];
        assert!(entry.is_dir);
        assert_eq!(entry.name, "Artist");
    }

    #[test]
    fn cancelled_write_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FsBackend::new(dir.path());
        write(&backend, "a.mp3", "old").unwrap();

        let data = vec![7u8; 600 * 1024];
        let err = backend
            .write("a.mp3", &mut data.as_slice(), data.len() as u64, &mut |n| {
                if n > 0 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert_eq!(backend.read("a.mp3").unwrap().as_deref(), Some(&b"old"[..]));
        assert_eq!(names(&backend, ""), vec!["a.mp3"]);
    }

    #[test]
    fn progress_reports_every_byte() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FsBackend::new(dir.path());
        let data = vec![1u8; 300 * 1024];
        let mut last = 0;
        backend
            .write(
                "a.flac",
                &mut data.as_slice(),
                data.len() as u64,
                &mut |n| {
                    last = n;
                    ControlFlow::Continue(())
                },
            )
            .unwrap();
        assert_eq!(last, data.len() as u64);
    }

    #[test]
    fn missing_things_are_not_errors() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FsBackend::new(dir.path());
        assert!(backend.list("Nope").unwrap().is_empty());
        assert_eq!(backend.read("Nope/a.mp3").unwrap(), None);
        backend.remove("Nope/a.mp3").unwrap();
    }

    #[test]
    fn remove_takes_folders_with_their_contents() {
        let dir = tempfile::tempdir().unwrap();
        let backend = FsBackend::new(dir.path());
        write(&backend, "Music/Album/a.mp3", "a").unwrap();
        write(&backend, "Music/b.mp3", "b").unwrap();
        backend.remove("Music/Album").unwrap();
        assert_eq!(names(&backend, "Music"), vec!["b.mp3"]);
        assert!(backend.remove("").is_err());
        assert!(dir.path().exists());
    }

    #[test]
    fn paths_cannot_leave_the_storage() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("device");
        std::fs::create_dir(&root).unwrap();
        let backend = FsBackend::new(&root);
        write(&backend, "../../escape.txt", "x").unwrap();
        assert!(root.join("escape.txt").exists());
        assert!(!parent.path().join("escape.txt").exists());
        assert_eq!(components("C:\\Music\\./a.mp3"), vec!["Music", "a.mp3"]);
    }

    #[test]
    fn space_reports_the_volume() {
        let dir = tempfile::tempdir().unwrap();
        let space = FsBackend::new(dir.path()).space().unwrap();
        assert!(space.total_bytes >= space.free_bytes && space.total_bytes > 0);
    }
}
