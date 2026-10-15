//! UI-independent JSON diffing: a structural diff addressed by JSON pointer,
//! and a structure-aware line diff for side-by-side display.

pub mod folder;
pub mod folds;
pub mod format;
pub mod ignore;
pub mod structural;
pub mod text;
pub mod tree;

pub use folder::{
    PairEntry, PairStatus, compare_folder_lists, compare_folders, is_listed_json, scan_json_files,
};
pub use folds::Folds;
pub use format::{FoldMark, FoldOptions, FormattedJson, format_json, format_json_folded};
pub use ignore::{IgnoreRule, is_ignored, last_key};
pub use structural::{Change, ChangeKind, PathMap, structural_diff, structural_diff_mapped};
pub use text::{Hunk, HunkKind, LineStatus, Segment, SideBySide};
pub use tree::{TreeOptions, TreeRow, TreeStatus};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

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
    /// Semantic changes, without ignored ones; `None` unless both sides
    /// parsed.
    pub changes: Option<Vec<Change>>,
    /// Semantic changes left out because an [`IgnoreRule`] matched them.
    pub ignored_changes: usize,
    pub left_error: Option<ParseError>,
    pub right_error: Option<ParseError>,
    /// The parsed documents, kept so folding and the tree can be rebuilt
    /// without re-parsing or re-diffing. `None` if a side is not valid JSON.
    documents: Option<Documents>,
}

#[derive(Debug, Clone)]
struct Documents {
    /// The line view is pretty-printed from these (Format JSON), so it can
    /// be folded; otherwise it shows the texts as they are.
    formatted: bool,
    left: Option<Value>,
    right: Option<Value>,
    sort_keys: bool,
    paths: PathMap,
    ignore: Vec<IgnoreRule>,
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
    diff_json_texts_ignoring(left, right, options, folds, &[])
}

/// [`diff_json_texts_folded`], leaving out changes that `ignore` matches.
/// In plain-text mode (not formatted, or invalid JSON) lines have no
/// pointers, so only the structural list leaves them out.
pub fn diff_json_texts_ignoring(
    left: &str,
    right: &str,
    options: &DiffOptions,
    folds: &Folds,
    ignore: &[IgnoreRule],
) -> JsonDiff {
    let (left_value, left_error) = parse(left);
    let (right_value, right_error) = parse(right);

    let mut ignored_changes = 0;
    let (changes, paths) = match (&left_value, &right_value) {
        (Some(left), Some(right)) => {
            let (mut changes, paths) = structural_diff_mapped(left, right);
            let before = changes.len();
            changes.retain(|change| !change_ignored(change, ignore, &paths));
            ignored_changes = before - changes.len();
            (Some(changes), paths)
        }
        _ => (None, PathMap::default()),
    };

    let parsed_or_blank = left_error.is_none() && right_error.is_none();
    let mut diff = JsonDiff {
        view: SideBySide::default(),
        changes,
        ignored_changes,
        left_error,
        right_error,
        documents: None,
    };
    let formatted = options.format && parsed_or_blank;
    if parsed_or_blank {
        diff.documents = Some(Documents {
            formatted,
            left: left_value,
            right: right_value,
            sort_keys: options.sort_keys,
            paths,
            ignore: ignore.to_vec(),
        });
    }
    if formatted {
        diff.refold(folds);
    } else {
        diff.view = SideBySide::new(left, right, options.ignore_whitespace);
    }
    diff
}

impl JsonDiff {
    /// Whether the view is formatted JSON, which can be folded.
    pub fn is_foldable(&self) -> bool {
        self.documents
            .as_ref()
            .is_some_and(|documents| documents.formatted)
    }

    /// Rebuilds the view for new fold state; cheap, as nothing is re-parsed.
    pub fn refold(&mut self, folds: &Folds) {
        if let Some(documents) = self
            .documents
            .as_ref()
            .filter(|documents| documents.formatted)
        {
            self.view = render(documents, self.changes.as_deref(), folds);
        }
    }

    /// The pointer on the other side of the node at `pointer`.
    pub fn counterpart(&self, pointer: &str, from_left: bool) -> Option<String> {
        self.documents.as_ref()?.paths.translate(pointer, from_left)
    }

    /// Whether `rule` ignores the node at `pointer` on one side; see
    /// [`ignored_on`].
    pub fn ignores(&self, rule: &IgnoreRule, left: bool, pointer: &str) -> bool {
        let none = PathMap::default();
        let paths = self
            .documents
            .as_ref()
            .map_or(&none, |documents| &documents.paths);
        ignored_on(std::slice::from_ref(rule), paths, left, pointer)
    }

    /// Both documents as one tree, for the tree view; `None` unless both
    /// sides are valid JSON (and not blank).
    pub fn tree(&self, options: &TreeOptions) -> Option<Vec<TreeRow>> {
        let documents = self.documents.as_ref()?;
        let (Some(left), Some(right)) = (&documents.left, &documents.right) else {
            return None;
        };
        Some(tree::rows(
            left,
            right,
            &documents.paths,
            &documents.ignore,
            documents.sort_keys,
            options,
        ))
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
    if !documents.ignore.is_empty() {
        // A block that still differs in text but has nothing coloured, and
        // involves an ignored key, is only the ignored change (plus commas
        // around it): it is no longer a change.
        let ignored = |left: bool, pointers: &[String], range: std::ops::Range<usize>| {
            pointers[range]
                .iter()
                .any(|pointer| ignored_on(&documents.ignore, &documents.paths, left, pointer))
        };
        let (left_status, right_status) = (&view.left_status, &view.right_status);
        let plain = |status: &[LineStatus], range: std::ops::Range<usize>| {
            status[range]
                .iter()
                .all(|status| *status == LineStatus::Unchanged)
        };
        let keep: Vec<bool> = view
            .hunks
            .iter()
            .map(|hunk| {
                !(plain(left_status, hunk.left.clone())
                    && plain(right_status, hunk.right.clone())
                    && (ignored(true, &left.pointers, hunk.left.clone())
                        || ignored(false, &right.pointers, hunk.right.clone())))
            })
            .collect();
        view.retain_hunks(&keep);
    }
    view.left_folds = left.folds;
    view.right_folds = right.folds;
    view.left_pointers = left.pointers;
    view.right_pointers = right.pointers;
    view
}

/// Whether `rules` ignore `change`, on either side.
pub(crate) fn change_ignored(change: &Change, rules: &[IgnoreRule], paths: &PathMap) -> bool {
    change
        .left_path()
        .is_some_and(|path| ignored_on(rules, paths, true, path))
        || change
            .right_path()
            .is_some_and(|path| ignored_on(rules, paths, false, path))
}

/// Whether `rules` ignore the node at `pointer` on one side. A pointer rule
/// names a node as the original document has it, so a modified-side pointer
/// is first translated through matched array elements; one with no
/// counterpart is compared as it is.
pub(crate) fn ignored_on(rules: &[IgnoreRule], paths: &PathMap, left: bool, pointer: &str) -> bool {
    rules.iter().any(|rule| match rule {
        IgnoreRule::Pointer { .. } if !left => rule.matches(
            &paths
                .translate(pointer, false)
                .unwrap_or_else(|| pointer.to_owned()),
        ),
        _ => rule.matches(pointer),
    })
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

/// The colour of a changed line on one side, as in VS Code: red in the
/// original, green in the modified document.
fn side_colour(left: bool) -> LineStatus {
    if left {
        LineStatus::Removed
    } else {
        LineStatus::Added
    }
}

/// The structural changes of one side, indexed by pointer.
struct ChangeIndex<'a> {
    left: bool,
    /// Nodes on this side that were themselves added, removed or modified.
    changed: HashSet<&'a str>,
    /// Every container on this side with a change somewhere below it.
    above_changes: HashSet<String>,
}

impl<'a> ChangeIndex<'a> {
    fn new(changes: &'a [Change], left: bool, paths: &PathMap) -> Self {
        let mut changed = HashSet::new();
        let mut above_changes = HashSet::new();
        for change in changes {
            let own = if left {
                change.left_path()
            } else {
                change.right_path()
            };
            changed.extend(own);
            // Containers that gained or lost a child changed, on both sides,
            // even where the child itself is missing.
            above_changes.extend(containers_holding(change, left, paths));
        }
        Self {
            left,
            changed,
            above_changes,
        }
    }

    /// Whether the node at `pointer`, or anything below it, changed.
    fn touches(&self, pointer: &str) -> bool {
        self.changed.contains(pointer) || self.above_changes.contains(pointer)
    }

    /// Colours each line, red on the original side and green on the
    /// modified one:
    /// - a node that was itself added, removed or modified, and everything
    ///   inside it;
    /// - a folded container with changes inside, since its one line stands
    ///   for them (expanded, it stays plain and its changed children carry
    ///   the colour);
    /// - anything else is unchanged, even if its text differs (e.g. a comma
    ///   added because an element now follows).
    fn statuses(&self, pointers: &[String], folds: &[Option<FoldMark>]) -> Vec<LineStatus> {
        pointers
            .iter()
            .zip(folds)
            .map(|(pointer, fold)| {
                let mut current = pointer.as_str();
                loop {
                    if self.changed.contains(current) {
                        return side_colour(self.left);
                    }
                    match current.rfind('/') {
                        Some(end) => current = &current[..end],
                        None => break,
                    }
                }
                if fold.as_ref().is_some_and(|fold| fold.collapsed)
                    && self.above_changes.contains(pointer)
                {
                    side_colour(self.left)
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
                ("\"version\": \"1\",", Removed),
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
                ("\"version\": \"2\",", Added),
                ("\"tags\": [", Unchanged),
                // ^ expanded, the key stays plain: its new child is coloured.
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
                // A child was added inside the folded key: red here, green there.
                ("\"env\": {…},", Removed),
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
                ("\"env\": {…},", Added),
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
    fn expanding_colours_the_child_not_the_key() {
        use LineStatus::*;
        let mut folds = Folds::default();
        let mut diff = diff_json_texts_folded(LEFT, RIGHT, &DiffOptions::default(), &folds);
        folds.set_expanded(false, "/env", true);
        folds.set_expanded(true, "/env", true);
        diff.refold(&folds);
        let right = coloured(&diff.view, false);
        assert!(right.contains(&("\"env\": {".to_owned(), Unchanged)));
        assert!(right.contains(&("\"LOG\": \"info\",".to_owned(), Unchanged)));
        assert!(right.contains(&("\"TRACE\": \"1\"".to_owned(), Added)));
        assert!(coloured(&diff.view, true).contains(&("\"env\": {".to_owned(), Unchanged)));
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

    #[test]
    fn ignored_keys_are_no_longer_changes() {
        let left =
            r#"{"svc": {"port": 1, "name": "a"}, "items": [{"at": 1, "v": 1}], "legacy": "x"}"#;
        let right = r#"{"svc": {"port": 2, "name": "b"}, "items": [{"at": 2, "v": 1}], "legacy": "x", "seen": 3}"#;
        let options = DiffOptions::default();
        let all = diff_json_texts(left, right, &options);
        assert_eq!(all.changes.as_ref().map(Vec::len), Some(4));

        let rules = [
            IgnoreRule::Pointer {
                pointer: "/svc/port".into(),
            },
            IgnoreRule::Key { key: "at".into() },
            IgnoreRule::Key { key: "seen".into() },
        ];
        let diff = diff_json_texts_ignoring(left, right, &options, &Folds::expanded(), &rules);
        let changes = diff.changes.unwrap();
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert_eq!(changes[0].path, "/svc/name");
        assert_eq!(diff.ignored_changes, 3);

        // Only `name` is coloured or a change block; `"legacy": "x"` gained
        // a comma for the ignored `seen` and stays plain.
        let view = &diff.view;
        assert_eq!(view.hunks.len(), 1);
        let coloured = |left: bool| -> Vec<&str> {
            let lines = if left { &view.left } else { &view.right };
            lines
                .iter()
                .zip(view.status(left))
                .filter(|(_, status)| **status != LineStatus::Unchanged)
                .map(|(line, _)| line.trim())
                .collect()
        };
        assert_eq!(coloured(true), [r#""name": "a""#]);
        assert_eq!(coloured(false), [r#""name": "b""#]);
        assert!(
            view.segments
                .iter()
                .filter_map(|segment| segment.hunk)
                .eq(0..1),
            "segments point at the remaining hunk"
        );
        assert_eq!(view.pointer(false, 2), Some("/svc/port"));
    }
}
