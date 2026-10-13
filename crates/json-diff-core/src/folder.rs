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
                _ if same_content(&left_root.join(path), &right_root.join(path), ignore) => {
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

fn same_content(left: &Path, right: &Path, ignore: &[IgnoreRule]) -> bool {
    let (Ok(left), Ok(right)) = (std::fs::read(left), std::fs::read(right)) else {
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
