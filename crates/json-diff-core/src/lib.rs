//! UI-independent JSON diffing: a structural diff addressed by JSON pointer,
//! and a structure-aware line diff for side-by-side display.

pub mod folder;
pub mod format;
pub mod structural;
pub mod text;

pub use folder::{PairEntry, PairStatus, compare_folders, scan_json_files};
pub use format::{FormattedJson, format_json};
pub use structural::{Change, ChangeKind, structural_diff};
pub use text::{Hunk, HunkKind, LineStatus, Segment, SideBySide};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffOptions {
    /// Pretty print both documents before comparing, so formatting-only
    /// differences disappear and lines align by JSON structure.
    pub format: bool,
    /// Sort object keys when formatting.
    pub sort_keys: bool,
    /// When not formatting, ignore whitespace while matching lines.
    pub ignore_whitespace: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            format: true,
            sort_keys: false,
            ignore_whitespace: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (line {}, column {})",
            self.message, self.line, self.column
        )
    }
}

/// The result of comparing two JSON texts.
#[derive(Debug, Clone, Default)]
pub struct JsonDiff {
    pub view: SideBySide,
    /// Semantic changes; `None` unless both sides parsed.
    pub changes: Option<Vec<Change>>,
    pub left_error: Option<ParseError>,
    pub right_error: Option<ParseError>,
}

/// Compares two JSON documents given as text. A side that is blank counts as
/// an empty document; a side that fails to parse is still shown and compared
/// as plain text, with its error reported.
pub fn diff_json_texts(left: &str, right: &str, options: &DiffOptions) -> JsonDiff {
    let (left_value, left_error) = parse(left);
    let (right_value, right_error) = parse(right);

    let changes = match (&left_value, &right_value) {
        (Some(left), Some(right)) => Some(structural_diff(left, right)),
        _ => None,
    };

    let parsed_or_blank = left_error.is_none() && right_error.is_none();
    let view = if options.format && parsed_or_blank {
        let format = |value: &Option<Value>| {
            value
                .as_ref()
                .map(|value| format_json(value, options.sort_keys))
                .unwrap_or_default()
        };
        let (left, right) = (format(&left_value), format(&right_value));
        let mut view = SideBySide::with_keys(left.lines, right.lines, &left.keys, &right.keys);
        if let Some(changes) = &changes {
            view.left_status = semantic_status(&left.pointers, changes, Change::left_path, true);
            view.right_status =
                semantic_status(&right.pointers, changes, Change::right_path, false);
        }
        view
    } else {
        SideBySide::new(left, right, options.ignore_whitespace)
    };

    JsonDiff {
        view,
        changes,
        left_error,
        right_error,
    }
}

/// Colours each line by the structural change covering its JSON pointer (the
/// node itself or an added/removed/retyped ancestor). Lines no change covers
/// are unchanged even if their text differs, e.g. a comma added because an
/// element now follows.
fn semantic_status(
    pointers: &[String],
    changes: &[Change],
    path_of: fn(&Change) -> Option<&str>,
    left: bool,
) -> Vec<LineStatus> {
    let by_path: HashMap<&str, LineStatus> = changes
        .iter()
        .filter_map(|change| {
            let status = match change.kind {
                ChangeKind::Modified => LineStatus::Modified,
                ChangeKind::Added | ChangeKind::Removed if left => LineStatus::Removed,
                ChangeKind::Added | ChangeKind::Removed => LineStatus::Added,
            };
            path_of(change).map(|path| (path, status))
        })
        .collect();

    pointers
        .iter()
        .map(|pointer| {
            let mut current = pointer.as_str();
            loop {
                if let Some(status) = by_path.get(current) {
                    return *status;
                }
                match current.rfind('/') {
                    Some(end) => current = &current[..end],
                    None => return LineStatus::Unchanged,
                }
            }
        })
        .collect()
}

fn parse(text: &str) -> (Option<Value>, Option<ParseError>) {
    if text.trim().is_empty() {
        return (None, None);
    }
    match serde_json::from_str(text) {
        Ok(value) => (Some(value), None),
        Err(error) => (
            None,
            Some(ParseError {
                message: error.to_string(),
                line: error.line(),
                column: error.column(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_differences_vanish() {
        let diff = diff_json_texts(
            r#"{"a":1,"b":[1,2]}"#,
            "{\n \"a\": 1, \"b\": [1,2]\n}",
            &DiffOptions::default(),
        );
        assert!(diff.view.hunks.is_empty());
        assert_eq!(diff.changes, Some(vec![]));
    }

    #[test]
    fn structure_aware_alignment() {
        // The `"id": 1,` line exists under both objects; it must align with
        // its own object's line, not the first look-alike.
        let left = r#"{"x": {"id": 1, "v": 1}, "y": {"id": 1, "v": 2}}"#;
        let right = r#"{"y": {"id": 1, "v": 2}}"#;
        let diff = diff_json_texts(left, right, &DiffOptions::default());
        assert_eq!(diff.view.hunks.len(), 1);
        assert_eq!(diff.view.hunks[0].kind, HunkKind::Removed);
        assert_eq!(diff.view.hunks[0].left, 1..5);
    }

    #[test]
    fn invalid_side_falls_back_to_text() {
        let diff = diff_json_texts("{\"a\": 1}", "{\"a\": ", &DiffOptions::default());
        assert!(diff.changes.is_none());
        assert!(diff.left_error.is_none());
        assert_eq!(diff.right_error.as_ref().map(|error| error.line), Some(1));
        assert_eq!(diff.view.right, vec!["{\"a\": "]);
    }

    #[test]
    fn blank_side_shows_everything_added() {
        let diff = diff_json_texts("", "{\"a\": 1}", &DiffOptions::default());
        assert_eq!(diff.view.hunks.len(), 1);
        assert_eq!(diff.view.hunks[0].kind, HunkKind::Added);
        assert!(diff.changes.is_none());
    }

    #[test]
    fn lines_are_coloured_by_semantic_change() {
        let left = r#"{"version": "1", "tags": ["a"], "old": true}"#;
        let right = r#"{"version": "2", "tags": ["a", "b"], "new": {"x": 1}}"#;
        let diff = diff_json_texts(left, right, &DiffOptions::default());
        let colours = |lines: &[String], status: &[LineStatus]| {
            lines
                .iter()
                .zip(status)
                .map(|(line, status)| (line.trim().to_owned(), *status))
                .collect::<Vec<_>>()
        };
        use LineStatus::*;
        assert_eq!(
            colours(&diff.view.left, &diff.view.left_status),
            [
                ("{", Unchanged),
                ("\"version\": \"1\",", Modified),
                ("\"tags\": [", Unchanged),
                ("\"a\"", Unchanged),
                ("],", Unchanged),
                ("\"old\": true", Removed),
                ("}", Unchanged),
            ]
            .map(|(line, status)| (line.to_owned(), status))
        );
        assert_eq!(
            colours(&diff.view.right, &diff.view.right_status),
            [
                ("{", Unchanged),
                ("\"version\": \"2\",", Modified),
                ("\"tags\": [", Unchanged),
                // Only a comma was added: the value is unchanged.
                ("\"a\",", Unchanged),
                ("\"b\"", Added),
                ("],", Unchanged),
                ("\"new\": {", Added),
                ("\"x\": 1", Added),
                ("}", Added),
                ("}", Unchanged),
            ]
            .map(|(line, status)| (line.to_owned(), status))
        );
    }
}
