//! The snapshot of [`AppState`] the frontend renders. Every command returns a
//! fresh one; the frontend keeps no state of its own beyond scroll positions
//! and what is typed into its fields.
//!
//! Positions are in the units Monaco uses: 1-based line numbers where noted,
//! and UTF-16 columns for character ranges.

use crate::agents_md::InstructionFile;
use crate::state::{AppState, Side, Theme, ViewMode};
use json_diff_core::{
    DiffOptions, FoldMark, Hunk, IgnoreRule, LineStatus, PairStatus, ParseError, SideBySide,
    last_key,
};
use serde::Serialize;
use std::ops::Range;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// Original (index 0) and modified (index 1) folder.
    pub folders: [Option<FolderView>; 2],
    /// Both folders are open, so a click compares a file with its twin.
    pub paired: bool,
    pub docs: [DocView; 2],
    pub diff: DiffView,
    pub current_hunk: Option<usize>,
    /// A change the frontend should scroll into view, once.
    pub reveal: Option<usize>,
    pub options: DiffOptions,
    pub view_mode: ViewMode,
    pub theme: Theme,
    pub zoom: f32,
    pub show_explorer: bool,
    pub show_agent: bool,
    pub status: Option<StatusView>,
    pub agents_md: Vec<AgentsMdView>,
    /// Folding needs formatted, valid JSON.
    pub foldable: bool,
    /// Semantic changes; `None` unless both sides parsed.
    pub semantic_changes: Option<usize>,
    /// Keys whose changes are ignored.
    pub ignored: Vec<IgnoredView>,
    /// Semantic changes left out because their key is ignored.
    pub ignored_changes: usize,
    pub added_lines: usize,
    pub removed_lines: usize,
    /// `original ↔ modified — json-diff`.
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub root: String,
    pub name: String,
    pub files: Vec<FileView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    /// Relative to the folder, with `/` separators.
    pub path: String,
    pub status: Option<PairStatus>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocView {
    pub label: String,
    pub path: Option<String>,
    pub error: Option<ErrorView>,
    /// The raw text, only sent in edit mode.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorView {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffView {
    pub left: SideView,
    pub right: SideView,
    pub hunks: Vec<HunkView>,
    /// Both documents cut into aligned blocks, in order: each is a run of
    /// unchanged lines or one change, as `[left start, left end, right start,
    /// right end]` (0-based, ends exclusive). The panes scroll through these
    /// together (see `ui/src/scroll.ts`).
    pub segments: Vec<[usize; 4]>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideView {
    pub text: String,
    /// One character per line: `.` unchanged, `+` added (green), `-`
    /// removed (red). Compact, since documents can run to 10k+ lines.
    pub status: String,
    pub folds: Vec<FoldView>,
    pub inline: Vec<InlineView>,
}

/// A foldable container's first line.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldView {
    /// 1-based.
    pub line: usize,
    pub pointer: String,
    pub collapsed: bool,
    pub children: usize,
    pub hidden_lines: usize,
    pub is_array: bool,
}

/// Changed characters within one line, highlighted on top of its colour.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineView {
    /// 1-based.
    pub line: usize,
    /// 1-based UTF-16 columns, end exclusive.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkView {
    /// 0-based, end exclusive. An empty range is an insertion point.
    pub left: [usize; 2],
    pub right: [usize; 2],
    pub left_status: LineStatus,
    pub right_status: LineStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredView {
    /// `/a/b` or `"name" everywhere`.
    pub label: String,
    pub rule: IgnoreRule,
}

/// The key under a line, for the diff's right-click menu.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyAt {
    pub pointer: String,
    /// The key's name; `None` for an array element.
    pub key: Option<String>,
    /// The rule that already ignores it, if any.
    pub ignored_by: Option<IgnoredView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsMdView {
    pub path: String,
    /// `parent/AGENTS.md`, enough to tell files apart.
    pub label: String,
    pub source: &'static str,
    pub enabled: bool,
}

/// Snapshots `state`, consuming its one-shot "scroll to the current change"
/// request.
pub fn snapshot(state: &mut AppState) -> View {
    let reveal = std::mem::take(&mut state.scroll_to_current)
        .then_some(state.current_hunk)
        .flatten();
    let diff = &state.diff;
    let view = &diff.view;
    let edit_mode = state.view_mode == ViewMode::Edit;
    let doc = |side: Side| {
        let doc = &state.docs[side.index()];
        let error = match side {
            Side::Left => &diff.left_error,
            Side::Right => &diff.right_error,
        };
        DocView {
            label: doc.label(),
            path: doc.path.as_ref().map(|path| path.display().to_string()),
            error: error.as_ref().map(error_view),
            text: edit_mode.then(|| doc.text.clone()),
        }
    };
    let folder = |side: Side| {
        state.folders[side.index()]
            .as_ref()
            .map(|folder| FolderView {
                root: folder.root.display().to_string(),
                name: folder.root.file_name().map_or_else(
                    || folder.root.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                files: folder
                    .files
                    .iter()
                    .map(|path| FileView {
                        path: slash_path(path),
                        status: state.pair_status(path),
                        selected: state.is_selected(side, path),
                    })
                    .collect(),
            })
    };

    View {
        folders: [folder(Side::Left), folder(Side::Right)],
        paired: !state.pairs.is_empty(),
        docs: [doc(Side::Left), doc(Side::Right)],
        diff: DiffView {
            left: side_view(view, true),
            right: side_view(view, false),
            hunks: (0..view.hunks.len())
                .map(|index| {
                    let (left, right) = changed_lines(view, &view.hunks[index]);
                    HunkView {
                        left: [left.start, left.end],
                        right: [right.start, right.end],
                        left_status: view.hunk_status(index, true),
                        right_status: view.hunk_status(index, false),
                    }
                })
                .collect(),
            segments: view
                .segments
                .iter()
                .map(|segment| {
                    [
                        segment.left.start,
                        segment.left.end,
                        segment.right.start,
                        segment.right.end,
                    ]
                })
                .collect(),
        },
        current_hunk: state.current_hunk,
        reveal,
        options: state.options,
        view_mode: state.view_mode,
        theme: state.theme,
        zoom: state.zoom,
        show_explorer: state.show_explorer,
        show_agent: state.show_agent,
        status: state.status.as_ref().map(|status| StatusView {
            text: status.text.clone(),
            is_error: status.is_error,
        }),
        agents_md: state.agents_md.files().iter().map(agents_md_view).collect(),
        foldable: diff.is_foldable(),
        semantic_changes: diff.changes.as_ref().map(Vec::len),
        ignored: state.ignored.iter().map(ignored_view).collect(),
        ignored_changes: diff.ignored_changes,
        added_lines: view.added_lines(),
        removed_lines: view.removed_lines(),
        title: format!(
            "{} ↔ {} — json-diff",
            state.docs[0].label(),
            state.docs[1].label()
        ),
    }
}

fn ignored_view(rule: &IgnoreRule) -> IgnoredView {
    IgnoredView {
        label: rule.to_string(),
        rule: rule.clone(),
    }
}

/// The key on 1-based `line` of one side, or `None` for the document's own
/// brackets and in plain-text mode.
pub fn key_at(state: &AppState, side: Side, line: usize) -> Option<KeyAt> {
    let view = &state.diff.view;
    let left = side == Side::Left;
    let pointer = view.pointer(left, line.checked_sub(1)?)?;
    if pointer.is_empty() {
        return None;
    }
    // A member's first line starts with its key; an array element's has
    // none, and its last pointer token is an index.
    let lines = if left { &view.left } else { &view.right };
    let first = (0..lines.len()).find(|&index| view.pointer(left, index) == Some(pointer))?;
    let key = last_key(pointer).filter(|key| {
        let quoted = serde_json::Value::String(key.clone()).to_string();
        lines[first]
            .trim_start()
            .starts_with(&format!("{quoted}: "))
    });
    let ignored_by = state
        .ignored
        .iter()
        .find(|rule| state.diff.ignores(rule, left, pointer))
        .map(ignored_view);
    Some(KeyAt {
        pointer: pointer.to_owned(),
        key,
        ignored_by,
    })
}

/// The lines of `hunk` that changed, on each side: the hunk without the
/// unchanged lines at its edges, such as a line that only gained a comma
/// because a key was added after it. A side left with nothing becomes the
/// insertion point next to its counterpart of those edge lines, so the
/// ribbon points where the change belongs. A hunk that only reformatted
/// lines keeps its full ranges.
fn changed_lines(view: &SideBySide, hunk: &Hunk) -> (Range<usize>, Range<usize>) {
    let changed = |left: bool, range: &Range<usize>| {
        let statuses = &view.status(left)[range.clone()];
        let first = statuses
            .iter()
            .position(|status| *status != LineStatus::Unchanged)?;
        let last = statuses
            .iter()
            .rposition(|status| *status != LineStatus::Unchanged)?;
        Some(range.start + first..range.start + last + 1)
    };
    // Where an empty side's insertion point goes: past as many of its lines
    // as the other side skipped before its change.
    let point = |own: &Range<usize>, other: &Range<usize>, other_changed: &Range<usize>| {
        let at = own.start + (other_changed.start - other.start).min(own.len());
        at..at
    };
    match (changed(true, &hunk.left), changed(false, &hunk.right)) {
        (Some(left), Some(right)) => (left, right),
        (None, Some(right)) => (point(&hunk.left, &hunk.right, &right), right),
        (Some(left), None) => {
            let right = point(&hunk.right, &hunk.left, &left);
            (left, right)
        }
        (None, None) => (hunk.left.clone(), hunk.right.clone()),
    }
}

fn side_view(view: &SideBySide, left: bool) -> SideView {
    let lines = if left { &view.left } else { &view.right };
    let status = view
        .status(left)
        .iter()
        .map(|status| match status {
            LineStatus::Unchanged => '.',
            LineStatus::Added => '+',
            LineStatus::Removed => '-',
        })
        .collect();
    let folds = view
        .folds(left)
        .iter()
        .enumerate()
        .filter_map(|(index, mark)| Some((index, mark.as_ref()?)))
        .map(|(index, mark)| fold_view(index, mark, &lines[index]))
        .collect();

    let mut inline = Vec::new();
    for hunk in &view.hunks {
        let (range, ranges) = if left {
            (&hunk.left, &hunk.left_inline)
        } else {
            (&hunk.right, &hunk.right_inline)
        };
        for (index, line_ranges) in range.clone().zip(ranges) {
            let text = &lines[index];
            for bytes in line_ranges {
                let Some(columns) = utf16_columns(text, bytes) else {
                    continue;
                };
                inline.push(InlineView {
                    line: index + 1,
                    start: columns.start + 1,
                    end: columns.end + 1,
                });
            }
        }
    }

    SideView {
        text: lines.join("\n"),
        status,
        folds,
        inline,
    }
}

fn fold_view(index: usize, mark: &FoldMark, line: &str) -> FoldView {
    let trimmed = line.trim_end().trim_end_matches(',');
    FoldView {
        line: index + 1,
        pointer: mark.pointer.clone(),
        collapsed: mark.collapsed,
        children: mark.children,
        hidden_lines: mark.hidden_lines,
        is_array: trimmed.ends_with('[') || trimmed.ends_with("[…]"),
    }
}

/// `bytes` of `text` as UTF-16 code unit offsets, or `None` if the range
/// does not fall on character boundaries.
fn utf16_columns(text: &str, bytes: &Range<usize>) -> Option<Range<usize>> {
    let start = text.get(..bytes.start)?.encode_utf16().count();
    let length = text.get(bytes.clone())?.encode_utf16().count();
    Some(start..start + length)
}

fn error_view(error: &ParseError) -> ErrorView {
    ErrorView {
        message: error.message.clone(),
        line: error.line,
        column: error.column,
    }
}

fn agents_md_view(file: &InstructionFile) -> AgentsMdView {
    AgentsMdView {
        path: file.path.display().to_string(),
        label: short_path(&file.path),
        source: file.source.as_str(),
        enabled: file.enabled,
    }
}

/// `parent/name`: enough to tell instruction files apart.
fn short_path(path: &Path) -> String {
    let name = path.file_name().map(|name| name.to_string_lossy());
    let parent = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy());
    match (parent, name) {
        (Some(parent), Some(name)) => format!("{parent}/{name}"),
        (None, Some(name)) => name.into_owned(),
        _ => path.display().to_string(),
    }
}

/// `path` with `/` separators on every platform.
fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;
    use crate::state::Command;

    fn sample() -> AppState {
        let mut state = AppState::default();
        for (side, text) in [
            (Side::Left, samples::ORIGINAL),
            (Side::Right, samples::MODIFIED),
        ] {
            state
                .apply(Command::SetText {
                    side,
                    text: text.into(),
                })
                .unwrap();
        }
        state
    }

    #[test]
    fn right_click_finds_the_key_and_ignoring_it_hides_its_change() {
        let mut state = AppState::default();
        for (side, text) in [
            (
                Side::Left,
                r#"{"list": [{"id": "a", "v": 1, "at": 1}], "svc": {"port": 1}}"#,
            ),
            (
                Side::Right,
                r#"{"list": [{"new": 0}, {"id": "a", "v": 1, "at": 2}], "svc": {"port": 2}}"#,
            ),
        ] {
            let text = text.into();
            state.apply(Command::SetText { side, text }).unwrap();
        }
        state.apply(Command::ExpandAll).unwrap();
        let line_of = |state: &AppState, left: bool, needle: &str| {
            let lines = if left {
                &state.diff.view.left
            } else {
                &state.diff.view.right
            };
            lines.iter().position(|line| line.contains(needle)).unwrap() + 1
        };

        let port = key_at(&state, Side::Left, line_of(&state, true, "port")).unwrap();
        assert_eq!(
            (port.pointer.as_str(), port.key.as_deref()),
            ("/svc/port", Some("port"))
        );
        // An array element has a pointer but no key name.
        let element = key_at(&state, Side::Right, line_of(&state, false, "\"new\"") - 1).unwrap();
        assert_eq!((element.pointer.as_str(), element.key), ("/list/0", None));
        assert!(
            key_at(&state, Side::Left, 1).is_none(),
            "the document itself"
        );

        // Ignoring on the modified side keeps the original's pointer.
        let at = line_of(&state, false, "\"at\"");
        let pointer = key_at(&state, Side::Right, at).unwrap().pointer;
        assert_eq!(pointer, "/list/1/at");
        state
            .apply(Command::IgnoreChanges {
                side: Side::Right,
                pointer: Some(pointer),
                key: None,
            })
            .unwrap();
        assert_eq!(state.ignored_labels(), ["/list/0/at"]);
        let view = snapshot(&mut state);
        assert_eq!(view.ignored_changes, 1);
        assert_eq!(view.ignored[0].label, "/list/0/at");
        let ignored = key_at(&state, Side::Right, line_of(&state, false, "\"at\"")).unwrap();
        assert_eq!(ignored.ignored_by.unwrap().label, "/list/0/at");

        // The assistant hears about it with every message.
        let section = state.ignored_prompt_section();
        assert!(
            section.contains("- /list/0/at") && section.contains("1 in the current comparison")
        );
        let changes = state
            .apply(Command::GetChanges {
                offset: 0,
                limit: 10,
            })
            .unwrap();
        assert_eq!(changes["ignored"]["changes"], 1);

        state.apply(Command::StopIgnoring { rule: None }).unwrap();
        assert_eq!(snapshot(&mut state).ignored_changes, 0);
        assert!(state.ignored_prompt_section().is_empty());
    }

    #[test]
    fn segments_cover_both_documents_in_order() {
        let mut state = sample();
        state.apply(Command::RevealChanges).unwrap();
        let view = snapshot(&mut state);
        let (mut left, mut right) = (0, 0);
        for [left_start, left_end, right_start, right_end] in &view.diff.segments {
            assert_eq!((*left_start, *right_start), (left, right), "no gaps");
            (left, right) = (*left_end, *right_end);
        }
        assert_eq!(left, view.diff.left.status.len());
        assert_eq!(right, view.diff.right.status.len());
        assert_eq!(
            view.diff.left.status.len(),
            view.diff.left.text.lines().count()
        );
        // The inserted endpoint is a block with no lines on the original side.
        assert!(
            view.diff
                .segments
                .iter()
                .any(|[start, end, right_start, right_end]| start == end && right_end > right_start)
        );
    }

    /// Issue #1: a key added at the end makes the line before it gain a
    /// comma. That line is not a change, so the ribbon must not span it.
    #[test]
    fn ribbons_skip_lines_that_only_gained_a_comma() {
        let mut state = AppState::default();
        let left = r#"{"id": "USR-10482", "name": "Jane Doe", "status": "pending_verification", "role": "Analyst", "department": "Risk & Compliance", "legacySystemId": "SYS-9941"}"#;
        let right = r#"{"id": "USR-10482", "name": "Jane Doe", "status": "active", "department": "Risk & Compliance", "legacySystemId": "SYS-9941", "lastLogin": "2025-05-18T14:32:00Z"}"#;
        for (side, text) in [(Side::Left, left), (Side::Right, right)] {
            state
                .apply(Command::SetText {
                    side,
                    text: text.into(),
                })
                .unwrap();
        }
        let view = snapshot(&mut state);
        let lines = |side: &SideView| side.text.lines().map(str::to_owned).collect::<Vec<_>>();
        let (left_lines, right_lines) = (lines(&view.diff.left), lines(&view.diff.right));
        let added = view.diff.hunks.last().unwrap();
        // Right: just the "lastLogin" line.
        assert_eq!(added.right[1] - added.right[0], 1);
        assert!(right_lines[added.right[0]].contains("lastLogin"));
        // Left: the point just below "legacySystemId", where it was added.
        assert_eq!(added.left[0], added.left[1]);
        assert!(left_lines[added.left[0] - 1].contains("legacySystemId"));
        // The first change, with no formatting-only lines, is untouched.
        let first = &view.diff.hunks[0];
        assert_eq!((first.left, first.right), ([3, 5], [3, 4]));
    }

    #[test]
    fn inline_ranges_are_utf16_columns() {
        assert_eq!(utf16_columns("\"São\": 1", &(6..7)), Some(5..6));
        assert_eq!(utf16_columns("é", &(1..2)), None, "inside a character");

        let mut state = sample();
        state.apply(Command::RevealChanges).unwrap();
        let view = snapshot(&mut state);
        let lines: Vec<&str> = view.diff.right.text.lines().collect();
        let version = view
            .diff
            .right
            .inline
            .iter()
            .find(|range| lines[range.line - 1].contains("\"version\""))
            .expect("the version number is highlighted");
        let line: Vec<u16> = lines[version.line - 1].encode_utf16().collect();
        let changed = String::from_utf16(&line[version.start - 1..version.end - 1]).unwrap();
        assert!("2.4.0".contains(&changed), "highlighted {changed:?}");
    }

    #[test]
    fn folds_and_reveal_are_reported_once() {
        let mut state = sample();
        let view = snapshot(&mut state);
        let database = view
            .diff
            .right
            .folds
            .iter()
            .find(|fold| fold.pointer == "/database")
            .unwrap();
        assert!(database.collapsed && !database.is_array);
        assert!(
            view.diff
                .right
                .folds
                .iter()
                .any(|fold| fold.pointer == "/features" && fold.is_array)
        );

        state.apply(Command::GoToChange { index: 2 }).unwrap();
        assert_eq!(snapshot(&mut state).reveal, Some(2));
        assert_eq!(snapshot(&mut state).reveal, None, "consumed");
    }
}
