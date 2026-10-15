//! Folder scanning and pairing of JSON files across two folders.

use crate::{IgnoreRule, change_ignored, structural_diff_mapped};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// Scanning stops after this many files so a mistaken pick of `/` stays usable.
pub const MAX_FILES: usize = 10_000;

/// Every `.json` file below `root`, as sorted paths relative to it. Hidden
/// entries (dot files and dot directories) are skipped.
pub fn scan_json_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(path);
            } else if is_json(&path) {
                if let Ok(relative) = path.strip_prefix(root) {
                    files.push(relative.to_path_buf());
                }
                if files.len() >= MAX_FILES {
                    files.sort();
                    return Ok(files);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_json(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// Whether a file at `relative` (inside a folder) would be listed by
/// [`scan_json_files`]: a `.json` file with no hidden part in its path. For
/// folders handed over as a list of files, as a browser does.
pub fn is_listed_json(relative: &Path) -> bool {
    is_json(relative)
        && relative
            .components()
            .all(|part| !part.as_os_str().to_string_lossy().starts_with('.'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairStatus {
    /// Same JSON value on both sides (formatting may differ).
    Identical,
    Modified,
    OnlyLeft,
    OnlyRight,
}

/// A relative path and how the files at it compare across the two folders.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairEntry {
    pub path: PathBuf,
    pub status: PairStatus,
}

/// Pairs files with the same relative path and compares their contents
/// semantically (key order and whitespace are ignored, and so are changes
/// that `ignore` matches). Files that fail to parse are compared byte for
/// byte.
pub fn compare_folders(
    left_root: &Path,
    left_files: &[PathBuf],
    right_root: &Path,
    right_files: &[PathBuf],
    ignore: &[IgnoreRule],
) -> Vec<PairEntry> {
    compare_folder_lists(left_files, right_files, ignore, |left, path| {
        std::fs::read(if left { left_root } else { right_root }.join(path)).ok()
    })
}

/// [`compare_folders`] for folders read some other way: `read(left, path)`
/// returns the file at relative `path` in the original (`left`) or modified
/// folder, or `None` if it cannot be read.
pub fn compare_folder_lists(
    left_files: &[PathBuf],
    right_files: &[PathBuf],
    ignore: &[IgnoreRule],
    read: impl Fn(bool, &Path) -> Option<Vec<u8>>,
) -> Vec<PairEntry> {
    let mut sides: BTreeMap<&Path, (bool, bool)> = BTreeMap::new();
    for path in left_files {
        sides.entry(path).or_default().0 = true;
    }
    for path in right_files {
        sides.entry(path).or_default().1 = true;
    }

    sides
        .into_iter()
        .map(|(path, sides)| {
            let status = match sides {
                (true, false) => PairStatus::OnlyLeft,
                (false, true) => PairStatus::OnlyRight,
                _ if same_content(read(true, path), read(false, path), ignore) => {
                    PairStatus::Identical
                }
                _ => PairStatus::Modified,
            };
            PairEntry {
                path: path.to_path_buf(),
                status,
            }
        })
        .collect()
}

fn same_content(left: Option<Vec<u8>>, right: Option<Vec<u8>>, ignore: &[IgnoreRule]) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    if left == right {
        return true;
    }
    match (
        serde_json::from_slice::<serde_json::Value>(&left),
        serde_json::from_slice::<serde_json::Value>(&right),
    ) {
        (Ok(left), Ok(right)) if left == right => true,
        (Ok(_), Ok(_)) if ignore.is_empty() => false,
        (Ok(left), Ok(right)) => {
            let (changes, paths) = structural_diff_mapped(&left, &right);
            changes
                .iter()
                .all(|change| change_ignored(change, ignore, &paths))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_files_are_visible_json_files() {
        assert!(is_listed_json(Path::new("a/b.JSON")));
        assert!(!is_listed_json(Path::new("a/b.txt")));
        assert!(!is_listed_json(Path::new(".git/config.json")));
        assert!(!is_listed_json(Path::new("a/.hidden.json")));
    }

    #[test]
    fn folders_in_memory_compare_like_folders_on_disk() {
        let files = |names: &[&str]| -> Vec<PathBuf> { names.iter().map(PathBuf::from).collect() };
        let contents = |left: bool, path: &Path| -> Option<Vec<u8>> {
            let text = match (left, path.to_str()?) {
                (true, "same.json") => r#"{"a":1,"b":2}"#,
                (false, "same.json") => r#"{"b": 2, "a": 1}"#,
                (true, "changed.json") => "[1]",
                (false, "changed.json") => "[2]",
                _ => return None,
            };
            Some(text.as_bytes().to_vec())
        };
        let entries = compare_folder_lists(
            &files(&["changed.json", "same.json", "old.json"]),
            &files(&["changed.json", "same.json", "new.json"]),
            &[],
            contents,
        );
        let statuses: Vec<_> = entries.iter().map(|entry| entry.status).collect();
        assert_eq!(
            statuses,
            [
                PairStatus::Modified,
                PairStatus::OnlyRight,
                PairStatus::OnlyLeft,
                PairStatus::Identical
            ]
        );
    }

    #[test]
    fn files_differing_only_in_ignored_keys_are_identical() {
        let root = tempfile::tempdir().unwrap();
        let (left, right) = (root.path().join("left"), root.path().join("right"));
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("a.json"), r#"{"v": 1, "at": "mon"}"#).unwrap();
        std::fs::write(right.join("a.json"), r#"{"v": 1, "at": "tue"}"#).unwrap();
        let files = vec![PathBuf::from("a.json")];
        let status = |ignore: &[IgnoreRule]| {
            compare_folders(&left, &files, &right, &files, ignore)[0].status
        };
        assert_eq!(status(&[]), PairStatus::Modified);
        assert_eq!(
            status(&[IgnoreRule::Key { key: "at".into() }]),
            PairStatus::Identical
        );
    }

    #[test]
    fn pairs_and_compares_folders() {
        let root = tempfile::tempdir().unwrap();
        let (left, right) = (root.path().join("left"), root.path().join("right"));
        std::fs::create_dir_all(left.join("nested")).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("same.json"), r#"{"a":1,"b":2}"#).unwrap();
        std::fs::write(right.join("same.json"), "{\n  \"b\": 2,\n  \"a\": 1\n}").unwrap();
        std::fs::write(left.join("changed.json"), "[1]").unwrap();
        std::fs::write(right.join("changed.json"), "[2]").unwrap();
        std::fs::write(left.join("nested/old.json"), "{}").unwrap();
        std::fs::write(right.join("new.JSON"), "{}").unwrap();
        std::fs::write(right.join("notes.txt"), "ignored").unwrap();

        let left_files = scan_json_files(&left).unwrap();
        let right_files = scan_json_files(&right).unwrap();
        let entries = compare_folders(&left, &left_files, &right, &right_files, &[]);

        let summary: Vec<_> = entries
            .iter()
            .map(|entry| (entry.path.to_string_lossy().into_owned(), entry.status))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("changed.json".into(), PairStatus::Modified),
                ("nested/old.json".into(), PairStatus::OnlyLeft),
                ("new.JSON".into(), PairStatus::OnlyRight),
                ("same.json".into(), PairStatus::Identical),
            ]
        );
    }
}
