//! File helpers shared by projects, recovery and export: safe writes,
//! file fingerprints and paths relative to a project.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Write `bytes` to `path` without ever leaving a half-written file there:
/// write a temporary file next to it, flush it to disk, then rename it over
/// the target.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically_with(path, |file| file.write_all(bytes))
}

/// Like [`write_atomically`], with the content written by `fill`.
pub fn write_atomically_with(path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    let temporary = path.with_file_name(name);
    let result = (|| {
        let mut file = File::create(&temporary)?;
        fill(&mut file)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// SHA-256 of a file, read in pieces (originals can be large).
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Where a file the project refers to lives, and how to recognise it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    /// Absolute path when the project was saved.
    pub path: String,
    /// Path relative to the project file's folder, when there is one (so a
    /// folder moved as a whole still works).
    #[serde(default)]
    pub relative_path: Option<String>,
    pub size: u64,
    pub sha256: String,
}

impl FileRef {
    /// Fingerprint the file at `path`.
    pub fn of(path: &Path) -> io::Result<FileRef> {
        Ok(FileRef {
            path: path.to_string_lossy().into_owned(),
            relative_path: None,
            size: fs::metadata(path)?.len(),
            sha256: sha256_file(path)?,
        })
    }

    /// This reference with its path relative to `project_dir` filled in.
    pub fn relative_to(&self, project_dir: &Path) -> FileRef {
        FileRef {
            relative_path: relative_path(project_dir, Path::new(&self.path)).map(|p| p.to_string_lossy().into_owned()),
            ..self.clone()
        }
    }

    /// True when the file at `path` is this file (same size and content).
    pub fn matches(&self, path: &Path) -> io::Result<bool> {
        if fs::metadata(path)?.len() != self.size {
            return Ok(false);
        }
        Ok(sha256_file(path)? == self.sha256)
    }

    /// Where to look for the file, in order: the saved absolute path, then
    /// the path relative to the project's folder.
    pub fn candidates(&self, project_dir: Option<&Path>) -> Vec<PathBuf> {
        let mut paths = vec![PathBuf::from(&self.path)];
        if let (Some(dir), Some(relative)) = (project_dir, &self.relative_path) {
            let joined = dir.join(relative);
            if !paths.contains(&joined) {
                paths.push(joined);
            }
        }
        paths
    }
}

/// How a referenced file was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    Found(PathBuf),
    /// A file is at the expected place but it is a different file.
    Changed(PathBuf),
    Missing,
}

/// Find the file `reference` points to (see [`FileRef::candidates`]).
pub fn locate(reference: &FileRef, project_dir: Option<&Path>) -> Located {
    let mut changed = None;
    for candidate in reference.candidates(project_dir) {
        match reference.matches(&candidate) {
            Ok(true) => return Located::Found(candidate),
            Ok(false) => {
                changed.get_or_insert(candidate);
            }
            Err(_) => {}
        }
    }
    changed.map_or(Located::Missing, Located::Changed)
}

/// `target` relative to the folder `base`, when both are absolute and on
/// the same drive.
pub fn relative_path(base: &Path, target: &Path) -> Option<PathBuf> {
    if !base.is_absolute() || !target.is_absolute() {
        return None;
    }
    let base: Vec<Component> = base.components().collect();
    let target: Vec<Component> = target.components().collect();
    // Different drive or root: no relative path.
    if base.first() != target.first() {
        return None;
    }
    let common = base.iter().zip(&target).take_while(|(a, b)| same_component(a, b)).count();
    let mut out = PathBuf::new();
    for _ in common..base.len() {
        out.push("..");
    }
    for part in &target[common..] {
        out.push(part.as_os_str());
    }
    Some(out)
}

/// Windows paths compare without regard to case.
fn same_component(a: &Component, b: &Component) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

/// True when `a` and `b` name the same file (existing files are compared
/// by their resolved paths; otherwise by text, ignoring case).
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh, empty folder for one test.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aibs-files-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_partial_file() {
        let dir = scratch("atomic");
        let path = dir.join("out.bin");
        write_atomically(&path, b"first").unwrap();
        write_atomically(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(!dir.join("out.bin.partial").exists());
        // A failing writer leaves the previous file untouched.
        let failed = write_atomically_with(&path, |_| Err(io::Error::other("disk full")));
        assert!(failed.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(!dir.join("out.bin.partial").exists());
    }

    #[test]
    fn file_refs_find_moved_folders_and_notice_changed_files() {
        let dir = scratch("refs");
        let photo = dir.join("photos").join("a.jpg");
        fs::create_dir_all(photo.parent().unwrap()).unwrap();
        fs::write(&photo, b"photo bytes").unwrap();
        let reference = FileRef::of(&photo).unwrap().relative_to(&dir.join("projects"));
        assert_eq!(reference.relative_path.as_deref().map(Path::new), Some(Path::new("../photos/a.jpg")));
        assert_eq!(locate(&reference, Some(&dir.join("projects"))), Located::Found(photo.clone()));

        // The whole folder moves: found through the relative path.
        let moved = scratch("refs-moved");
        fs::create_dir_all(moved.join("photos")).unwrap();
        fs::create_dir_all(moved.join("projects")).unwrap();
        fs::copy(&photo, moved.join("photos").join("a.jpg")).unwrap();
        fs::remove_file(&photo).unwrap();
        assert_eq!(
            locate(&reference, Some(&moved.join("projects"))),
            Located::Found(moved.join("projects").join("../photos/a.jpg"))
        );
        // A different file in its place is noticed.
        fs::write(&photo, b"other bytes").unwrap();
        fs::remove_file(moved.join("photos").join("a.jpg")).unwrap();
        assert_eq!(locate(&reference, Some(&moved.join("projects"))), Located::Changed(photo.clone()));
        fs::remove_file(&photo).unwrap();
        assert_eq!(locate(&reference, None), Located::Missing);
    }

    #[test]
    fn relative_paths() {
        let base = std::env::temp_dir().join("a").join("b");
        let target = std::env::temp_dir().join("a").join("c").join("x.jpg");
        assert_eq!(relative_path(&base, &target), Some(PathBuf::from("..").join("c").join("x.jpg")));
        assert_eq!(relative_path(Path::new("relative"), &target), None);
    }
}
