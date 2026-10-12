//! UI-independent JSON diffing: a structural diff addressed by JSON pointer,
//! and a structure-aware line diff for side-by-side display.

pub mod folder;
pub mod folds;
pub mod format;
pub mod structural;
pub mod text;

pub use folder::{PairEntry, PairStatus, compare_folders, scan_json_files};
pub use folds::Folds;
pub use format::{FoldMark, FoldOptions, FormattedJson, format_json, format_json_folded};
pub use structural::{Change, ChangeKind, PathMap, structural_diff, structural_diff_mapped};
pub use text::{Hunk, HunkKind, LineStatus, Segment, SideBySide};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
    /// The parsed documents, kept so folding can re-render without
    /// re-parsing or re-diffing. `None` in plain-text mode.
    documents: Option<Documents>,
}

#[derive(Debug, Clone)]
struct Documents {
    left: Option<Value>,
    right: Option<Value>,
    sort_keys: bool,
    paths: PathMap,
}

/// Compares two JSON documents given as text, fully expanded. See
/// [`diff_json_texts_folded`].
pub fn diff_json_texts(left: &str, right: &str, options: &DiffOptions) -> JsonDiff {
    diff_json_texts_folded(left, right, options, &Folds::expanded())
}

/// Compares two JSON documents given as text, showing containers as `folds`
/// says. A side that is blank counts as an empty document; a side that fails
/// to parse is still shown and compared as plain text (unfolded), with its
/// error reported.
pub fn diff_json_texts_folded(
    left: &str,
    right: &str,
    options: &DiffOptions,
    folds: &Folds,
) -> JsonDiff {
    let (left_value, left_error) = parse(left);
    let (right_value, right_error) = parse(right);

    let (changes, paths) = match (&left_value, &right_value) {
        (Some(left), Some(right)) => {
            let (changes, paths) = structural_diff_mapped(left, right);
            (Some(changes), paths)
        }
        _ => (None, PathMap::default()),
    };

    let parsed_or_blank = left_error.is_none() && right_error.is_none();
    let mut diff = JsonDiff {
        view: SideBySide::default(),
        changes,
        left_error,
        right_error,
        documents: None,
    };
    if options.format && parsed_or_blank {
        diff.documents = Some(Documents {
            left: left_value,
            right: right_value,
            sort_keys: options.sort_keys,
            paths,
        });
        diff.refold(folds);
    } else {
        diff.view = SideBySide::new(left, right, options.ignore_whitespace);
    }
    diff
}

impl JsonDiff {
    /// Whether the view is formatted JSON, which can be folded.
    pub fn is_foldable(&self) -> bool {
        self.documents.is_some()
    }

    /// Rebuilds the view for new fold state; cheap, as nothing is re-parsed.
    pub fn refold(&mut self, folds: &Folds) {
        if let Some(documents) = &self.documents {
            self.view = render(documents, self.changes.as_deref(), folds);
        }
    }

    /// The pointer on the other side of the node at `pointer`.
    pub fn counterpart(&self, pointer: &str, from_left: bool) -> Option<String> {
        self.documents.as_ref()?.paths.translate(pointer, from_left)
    }

    /// Folds that show every change: all collapsed except the containers
    /// leading to a change.
    pub fn reveal_changes(&self) -> Folds {
        let mut folds = Folds::collapsed();
        let Some(documents) = &self.documents else {
            return folds;
        };
        for change in self.changes.iter().flatten() {
            for left in [true, false] {
                for container in containers_holding(change, left, &documents.paths) {
                    folds.set_expanded(left, &container, true);
                }
            }
        }
        folds
    }
}

fn render(documents: &Documents, changes: Option<&[Change]>, folds: &Folds) -> SideBySide {
    let left_index = changes.map(|changes| ChangeIndex::new(changes, true, &documents.paths));
    let right_index = changes.map(|changes| ChangeIndex::new(changes, false, &documents.paths));

    let format = |value: &Option<Value>, index: &Option<ChangeIndex>, left: bool| {
        let Some(value) = value else {
            return FormattedJson::default();
        };
        let expanded = |pointer: &str| folds.is_expanded(left, pointer);
        let changed = |pointer: &str| index.as_ref().is_some_and(|index| index.touches(pointer));
        let options = FoldOptions {
            expanded: &expanded,
            changed: &changed,
            side: if left { 'L' } else { 'R' },
        };
        format_json_folded(value, documents.sort_keys, &options)
    };
    let left = format(&documents.left, &left_index, true);
    let right = format(&documents.right, &right_index, false);

    let mut view = SideBySide::with_keys(left.lines, right.lines, &left.keys, &right.keys);
    if let (Some(left_index), Some(right_index)) = (&left_index, &right_index) {
        view.left_status = left_index.statuses(&left.pointers, &left.folds);
        view.right_status = right_index.statuses(&right.pointers, &right.folds);
    }
    view.left_folds = left.folds;
    view.right_folds = right.folds;
    view
}

/// The containers on one side that hold `change`: the ancestors of its node.
/// When the node only exists on the other side (an addition seen from the
/// original, or a removal seen from the modified document), its parent
/// still exists here and is found through `paths`. The root is included.
fn containers_holding(change: &Change, left: bool, paths: &PathMap) -> Vec<String> {
    let (own, other) = if left {
        (change.left_path(), change.right_path())
    } else {
        (change.right_path(), change.left_path())
    };
    let parent = match (own, other) {
        (Some(path), _) => path[..path.rfind('/').unwrap_or(0)].to_owned(),
        (None, Some(path)) => match paths.translate(&path[..path.rfind('/').unwrap_or(0)], !left) {
            Some(parent) => parent,
            None => return Vec::new(),
        },
        (None, None) => return Vec::new(),
    };
    let mut containers = vec![parent.clone()];
    let mut pointer = parent.as_str();
    while let Some(end) = pointer.rfind('/') {
        pointer = &pointer[..end];
        containers.push(pointer.to_owned());
    }
    containers
}

/// The structural changes of one side, indexed by pointer.
struct ChangeIndex<'a> {
    /// The status each changed node's own lines get.
    by_path: HashMap<&'a str, LineStatus>,
    /// Every container on this side with a change somewhere below it.
    above_changes: HashSet<String>,
}

impl<'a> ChangeIndex<'a> {
    fn new(changes: &'a [Change], left: bool, paths: &PathMap) -> Self {
        let mut by_path = HashMap::new();
        let mut above_changes = HashSet::new();
        for change in changes {
            let own = if left {
                change.left_path()
            } else {
                change.right_path()
            };
            if let Some(path) = own {
                let status = match change.kind {
                    ChangeKind::Modified => LineStatus::Modified,
                    ChangeKind::Added | ChangeKind::Removed if left => LineStatus::Removed,
                    ChangeKind::Added | ChangeKind::Removed => LineStatus::Added,
                };
                by_path.insert(path, status);
            }
            // Containers that gained or lost a child were modified, on both
            // sides, even where the child itself is missing.
            above_changes.extend(containers_holding(change, left, paths));
        }
        Self {
            by_path,
            above_changes,
        }
    }

    /// Whether the node at `pointer`, or anything below it, changed.
    fn touches(&self, pointer: &str) -> bool {
        self.by_path.contains_key(pointer) || self.above_changes.contains(pointer)
    }

    /// Colours each line:
    /// - a node that was itself added, removed or modified, and everything
    ///   inside it, takes that change's colour (green, red, yellow);
    /// - the key line of a container with changes inside is yellow, folded or
    ///   not, since the container's value changed;
    /// - anything else is unchanged, even if its text differs (e.g. a comma
    ///   added because an element now follows).
    fn statuses(&self, pointers: &[String], folds: &[Option<FoldMark>]) -> Vec<LineStatus> {
        pointers
            .iter()
            .zip(folds)
            .map(|(pointer, fold)| {
                let mut current = pointer.as_str();
                loop {
                    if let Some(status) = self.by_path.get(current) {
                        return *status;
                    }
                    match current.rfind('/') {
                        Some(end) => current = &current[..end],
                        None => break,
                    }
                }
                if fold.is_some() && self.above_changes.contains(pointer) {
                    LineStatus::Modified
                } else {
                    LineStatus::Unchanged
                }
            })
            .collect()
    }
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
                ("\"tags\": [", Modified),
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
                ("\"tags\": [", Modified),
                // ^ the existing key gained a child, so it was modified.
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

    /// (trimmed line, status) per line of one side.
    fn coloured(view: &SideBySide, left: bool) -> Vec<(String, LineStatus)> {
        let lines = if left { &view.left } else { &view.right };
        lines
            .iter()
            .zip(view.status(left))
            .map(|(line, status)| (line.trim().to_owned(), *status))
            .collect()
    }

    fn owned(lines: &[(&str, LineStatus)]) -> Vec<(String, LineStatus)> {
        lines
            .iter()
            .map(|(line, status)| (line.to_string(), *status))
            .collect()
    }

    const LEFT: &str =
        r#"{"name": "svc", "env": {"LOG": "info"}, "gone": {"x": 1}, "list": [1, 2]}"#;
    const RIGHT: &str =
        r#"{"name": "svc", "env": {"LOG": "info", "TRACE": "1"}, "list": [1, 2], "new": {"y": 2}}"#;

    #[test]
    fn default_folding_shows_top_level_keys_coloured() {
        use LineStatus::*;
        let diff = diff_json_texts_folded(LEFT, RIGHT, &DiffOptions::default(), &Folds::default());
        assert_eq!(
            coloured(&diff.view, true),
            owned(&[
                ("{", Unchanged),
                ("\"name\": \"svc\",", Unchanged),
                // A child was added inside: the existing key is modified.
                ("\"env\": {…},", Modified),
                // Removed top-level key.
                ("\"gone\": {…},", Removed),
                ("\"list\": […]", Unchanged),
                ("}", Unchanged),
            ])
        );
        assert_eq!(
            coloured(&diff.view, false),
            owned(&[
                ("{", Unchanged),
                ("\"name\": \"svc\",", Unchanged),
                ("\"env\": {…},", Modified),
                ("\"list\": […],", Unchanged),
                // Added top-level key.
                ("\"new\": {…}", Added),
                ("}", Unchanged),
            ])
        );
        // The folded, modified key is a change block of its own, so
        // navigation stops on it and a ribbon joins its two sides.
        let env = diff
            .view
            .hunks
            .iter()
            .find(|hunk| {
                diff.view.left[hunk.left.clone()]
                    .iter()
                    .any(|line| line.contains("env"))
            })
            .expect("a hunk for the folded, modified key");
        assert!(
            diff.view.right[env.right.clone()]
                .iter()
                .any(|line| line.contains("env"))
        );
    }

    #[test]
    fn expanding_keeps_the_key_yellow_and_colours_the_child() {
        use LineStatus::*;
        let mut folds = Folds::default();
        let mut diff = diff_json_texts_folded(LEFT, RIGHT, &DiffOptions::default(), &folds);
        folds.set_expanded(false, "/env", true);
        folds.set_expanded(true, "/env", true);
        diff.refold(&folds);
        let right = coloured(&diff.view, false);
        assert!(right.contains(&("\"env\": {".to_owned(), Modified)));
        assert!(right.contains(&("\"LOG\": \"info\",".to_owned(), Unchanged)));
        assert!(right.contains(&("\"TRACE\": \"1\"".to_owned(), Added)));
        assert!(coloured(&diff.view, true).contains(&("\"env\": {".to_owned(), Modified)));
    }

    #[test]
    fn fold_marks_describe_the_hidden_content() {
        let diff = diff_json_texts_folded(LEFT, RIGHT, &DiffOptions::default(), &Folds::default());
        let mark = diff.view.right_folds[2].as_ref().expect("env is foldable");
        assert_eq!(mark.pointer, "/env");
        assert!(mark.collapsed);
        assert_eq!((mark.children, mark.hidden_lines), (2, 3));
        assert!(diff.is_foldable());
        assert!(
            !diff_json_texts_folded("{", "{}", &DiffOptions::default(), &Folds::default())
                .is_foldable()
        );
    }

    #[test]
    fn reveal_changes_expands_only_the_way_to_each_change() {
        let left = r#"{"a": {"b": {"c": 1}, "quiet": {"q": 1}}, "list": [{"x": 1}]}"#;
        let right = r#"{"a": {"b": {"c": 2}, "quiet": {"q": 1}}, "list": [{"n": 0}, {"x": 1}]}"#;
        let diff = diff_json_texts_folded(left, right, &DiffOptions::default(), &Folds::default());
        let folds = diff.reveal_changes();
        for left in [true, false] {
            assert!(folds.is_expanded(left, "/a"));
            assert!(folds.is_expanded(left, "/a/b"));
            assert!(!folds.is_expanded(left, "/a/quiet"));
        }
        assert!(folds.is_expanded(false, "/list"));
        // The original side opens the list too, to show where the element went.
        assert!(folds.is_expanded(true, "/list"));
        // The inserted element itself stays folded (it is one green line).
        assert!(!folds.is_expanded(false, "/list/0"));
        assert_eq!(
            diff.counterpart("/list/1/x", false).as_deref(),
            Some("/list/0/x")
        );
    }
}
