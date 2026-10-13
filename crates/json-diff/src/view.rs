//! The snapshot of [`AppState`] the frontend renders. Every command returns a
//! fresh one; the frontend keeps no state of its own beyond scroll positions
//! and what is typed into its fields.
//!
//! Positions are in the units Monaco uses: 1-based line numbers where noted,
//! and UTF-16 columns for character ranges.

use crate::agents_md::InstructionFile;
use crate::state::{AppState, Side, Theme, ViewMode};
use json_diff_core::{DiffOptions, FoldMark, LineStatus, PairStatus, ParseError, SideBySide};
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
    pub padding: Vec<PaddingView>,
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

/// Blank lines inserted so both sides stay aligned, like VS Code's diff.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaddingView {
    /// The gap follows this 1-based line; 0 puts it above the first line.
    pub after_line: usize,
    pub lines: usize,
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
                    let hunk = &view.hunks[index];
                    HunkView {
                        left: [hunk.left.start, hunk.left.end],
                        right: [hunk.right.start, hunk.right.end],
                        left_status: view.hunk_status(index, true),
                        right_status: view.hunk_status(index, false),
                    }
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
        added_lines: view.added_lines(),
        removed_lines: view.removed_lines(),
        title: format!(
            "{} ↔ {} — json-diff",
            state.docs[0].label(),
            state.docs[1].label()
        ),
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

    // In each changed segment the shorter side gets the difference as a gap
    // after its last line.
    let padding = view
        .segments
        .iter()
        .filter_map(|segment| {
            let (own, other) = if left {
                (&segment.left, &segment.right)
            } else {
                (&segment.right, &segment.left)
            };
            (other.len() > own.len()).then(|| PaddingView {
                after_line: own.end,
                lines: other.len() - own.len(),
            })
        })
        .collect();

    SideView {
        text: lines.join("\n"),
        status,
        folds,
        inline,
        padding,
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
    fn sides_line_up_after_padding() {
        let mut state = sample();
        state.apply(Command::RevealChanges).unwrap();
        let view = snapshot(&mut state);
        let rows = |side: &SideView| {
            side.status.len() + side.padding.iter().map(|gap| gap.lines).sum::<usize>()
        };
        assert_eq!(rows(&view.diff.left), rows(&view.diff.right));
        assert_eq!(
            view.diff.left.status.len(),
            view.diff.left.text.lines().count()
        );
        // The inserted endpoint leaves a gap on the original side.
        assert!(!view.diff.left.padding.is_empty());
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
