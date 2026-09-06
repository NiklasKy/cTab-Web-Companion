use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(crate) const TERRAIN_CACHE_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MARKER_CACHE_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CACHE_FILES: usize = 100_000;
const MAX_CACHE_DEPTH: usize = 16;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct CacheStats {
    pub bytes: u64,
    pub files: usize,
}

#[derive(Debug)]
struct CacheFile {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

pub(crate) fn stats(root: &Path) -> CacheStats {
    let mut files = Vec::new();
    collect_files(root, root, 0, &mut files);
    CacheStats {
        bytes: files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.bytes)),
        files: files.len(),
    }
}

pub(crate) fn enforce_limit(root: &Path, limit: u64, protected: Option<&Path>) {
    if !root.is_absolute() || root.parent().is_none() || limit == 0 {
        return;
    }
    let mut files = Vec::new();
    collect_files(root, root, 0, &mut files);
    let mut total = files
        .iter()
        .fold(0_u64, |sum, file| sum.saturating_add(file.bytes));
    if total <= limit {
        return;
    }
    files.sort_by_key(|file| file.modified);
    for file in files {
        if total <= limit {
            break;
        }
        if protected.is_some_and(|path| path == file.path) {
            continue;
        }
        if std::fs::remove_file(&file.path).is_ok() {
            total = total.saturating_sub(file.bytes);
        }
    }
}

fn collect_files(root: &Path, directory: &Path, depth: usize, output: &mut Vec<CacheFile>) {
    if depth > MAX_CACHE_DEPTH || output.len() >= MAX_CACHE_FILES || !directory.starts_with(root) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if output.len() >= MAX_CACHE_FILES {
            return;
        }
        let path = entry.path();
        if !path.starts_with(root) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files(root, &path, depth + 1, output);
        } else if metadata.is_file() {
            output.push(CacheFile {
                path,
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_old_files_without_deleting_the_protected_write() {
        let directory = tempfile::tempdir().expect("temporary cache");
        let first = directory.path().join("first.bin");
        let protected = directory.path().join("protected.bin");
        std::fs::write(&first, vec![1_u8; 8]).expect("first cache file");
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&protected, vec![2_u8; 8]).expect("protected cache file");
        enforce_limit(directory.path(), 8, Some(&protected));
        assert!(!first.exists());
        assert!(protected.exists());
        assert_eq!(stats(directory.path()).bytes, 8);
    }
}
