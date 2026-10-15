//! What an exported PDF says, independent of how it is drawn: the files
//! compared, the list of changes, and the side-by-side diff trimmed to the
//! changes and a few lines of context around each.
//!
//! The report always comes from the fully expanded documents, whatever is
//! folded on screen, so nothing in it hides behind `{…}`. Changes to keys the
//! user ignores are left out, as on screen; the report says which keys.

use crate::state::AppState;
use json_diff_core::{
    ChangeKind, DiffOptions, Folds, LineStatus, SideBySide, diff_json_texts_ignoring,
};
use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

/// Values in the summary table are cut to this many characters.
const VALUE_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// `original ↔ modified`, for page headers.
    pub title: String,
    /// Original (index 0) and modified (index 1).
    pub sources: [Source; 2],
    pub options: DiffOptions,
    /// Ignored keys, as `/a/b` or `"name" everywhere`.
    pub ignored: Vec<String>,
    /// Changes left out because their key is ignored.
    pub ignored_changes: usize,
    /// When the report was made, e.g. `2026-10-13 14:05 UTC`.
    pub generated: String,
    pub changes: Changes,
    pub totals: Totals,
    /// The side-by-side section, one entry per group of nearby changes.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub label: String,
    pub path: Option<String>,
    /// Why this side is not valid JSON, if it isn't.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Changes {
    /// Both sides are JSON: every change by pointer, in document order.
    Structural(Vec<ChangeRow>),
    /// A side is not valid JSON, so the documents were compared as text.
    TextOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRow {
    pub kind: ChangeKind,
    pub pointer: String,
    /// Compact JSON, cut to [`VALUE_CHARS`]; `None` if the node is absent.
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    /// Changed blocks, as counted by the app's Next / Previous.
    pub blocks: usize,
    pub added_lines: usize,
    pub removed_lines: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// `Change 3 of 11`, or `Changes 3–5 of 11` when nearby changes share a
    /// block, or `Whole documents`.
    pub title: String,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// One line from each side, aligned. A side is `None` where the other
    /// has more lines in the same change.
    Lines {
        left: Option<Line>,
        right: Option<Line>,
    },
    /// Unchanged lines left out, on each side.
    Skipped(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// 1-based.
    pub number: usize,
    pub text: String,
    pub status: LineStatus,
    /// Changed characters, as character (not byte) ranges of `text`.
    pub changed: Vec<Range<usize>>,
}

/// Builds the report for what `state` compares. `context` is how many
/// unchanged lines to keep around each change; `None` keeps everything.
pub fn build(state: &AppState, context: Option<usize>, generated: SystemTime) -> Report {
    let diff = diff_json_texts_ignoring(
        &state.docs[0].text,
        &state.docs[1].text,
        &state.options,
        &Folds::expanded(),
        &state.ignored,
    );
    let view = &diff.view;
    let source = |index: usize| {
        let doc = &state.docs[index];
        let error = if index == 0 {
            &diff.left_error
        } else {
            &diff.right_error
        };
        Source {
            label: doc.label(),
            path: doc.path.as_ref().map(|path| path.display().to_string()),
            error: error.as_ref().map(ToString::to_string),
        }
    };

    let mut totals = Totals {
        blocks: view.hunks.len(),
        added_lines: view.added_lines(),
        removed_lines: view.removed_lines(),
        ..Totals::default()
    };
    let changes = match &diff.changes {
        Some(changes) => Changes::Structural(
            changes
                .iter()
                .map(|change| {
                    match change.kind {
                        ChangeKind::Added => totals.added += 1,
                        ChangeKind::Removed => totals.removed += 1,
                        ChangeKind::Modified => totals.modified += 1,
                    }
                    ChangeRow {
                        kind: change.kind,
                        pointer: if change.path.is_empty() {
                            "/ (the whole document)".to_owned()
                        } else {
                            change.path.clone()
                        },
                        old: change.old.as_ref().map(compact),
                        new: change.new.as_ref().map(compact),
                    }
                })
                .collect(),
        ),
        None => Changes::TextOnly,
    };

    Report {
        title: format!("{} ↔ {}", state.docs[0].label(), state.docs[1].label()),
        sources: [source(0), source(1)],
        options: state.options,
        ignored: state.ignored_labels(),
        ignored_changes: diff.ignored_changes,
        generated: utc_timestamp(generated),
        changes,
        totals,
        blocks: blocks(view, context),
    }
}

/// `value` as compact JSON, cut to [`VALUE_CHARS`] characters.
fn compact(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(VALUE_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

/// A row of the full side-by-side listing, before trimming.
struct FullRow {
    row: Row,
    /// The change this row belongs to, if it is changed.
    hunk: Option<usize>,
}

fn blocks(view: &SideBySide, context: Option<usize>) -> Vec<Block> {
    let rows = full_rows(view);
    let total = view.hunks.len();
    let Some(context) = context else {
        return if rows.is_empty() {
            Vec::new()
        } else {
            vec![Block {
                title: "Whole documents".to_owned(),
                rows: rows.into_iter().map(|row| row.row).collect(),
            }]
        };
    };

    // Each change's rows, widened by `context` and merged where they touch.
    let mut spans: Vec<(Range<usize>, Range<usize>)> = Vec::new(); // (rows, hunks)
    let mut index = 0;
    while index < rows.len() {
        let Some(hunk) = rows[index].hunk else {
            index += 1;
            continue;
        };
        let start = index;
        while index < rows.len() && rows[index].hunk == Some(hunk) {
            index += 1;
        }
        let widened = start.saturating_sub(context)..(index + context).min(rows.len());
        match spans.last_mut() {
            Some((last, hunks)) if widened.start <= last.end => {
                last.end = widened.end;
                hunks.end = hunk + 1;
            }
            _ => spans.push((widened, hunk..hunk + 1)),
        }
    }

    let mut blocks = Vec::with_capacity(spans.len());
    let mut shown_until = 0;
    for (number, (span, hunks)) in spans.iter().enumerate() {
        let mut block_rows = Vec::new();
        if span.start > shown_until {
            block_rows.push(Row::Skipped(span.start - shown_until));
        }
        block_rows.extend(rows[span.clone()].iter().map(|row| row.row.clone()));
        if number == spans.len() - 1 && span.end < rows.len() {
            block_rows.push(Row::Skipped(rows.len() - span.end));
        }
        shown_until = span.end;
        blocks.push(Block {
            title: if hunks.len() == 1 {
                format!("Change {} of {total}", hunks.start + 1)
            } else {
                format!("Changes {}–{} of {total}", hunks.start + 1, hunks.end)
            },
            rows: block_rows,
        });
    }
    blocks
}

/// Every aligned row of the two documents, in order.
fn full_rows(view: &SideBySide) -> Vec<FullRow> {
    let mut rows = Vec::new();
    for segment in &view.segments {
        let height = segment.left.len().max(segment.right.len());
        for offset in 0..height {
            let left = (offset < segment.left.len())
                .then(|| line(view, true, segment.left.start + offset, segment.hunk));
            let right = (offset < segment.right.len())
                .then(|| line(view, false, segment.right.start + offset, segment.hunk));
            rows.push(FullRow {
                row: Row::Lines { left, right },
                hunk: segment.hunk,
            });
        }
    }
    rows
}

fn line(view: &SideBySide, left: bool, index: usize, hunk: Option<usize>) -> Line {
    let raw = if left { &view.left } else { &view.right }[index].as_str();
    let status = view.status(left)[index];
    // Uncoloured lines in a change (a new comma, an ignored key) get no
    // highlighted characters either.
    let changed = hunk
        .filter(|_| status != LineStatus::Unchanged)
        .map(|hunk| {
            let hunk = &view.hunks[hunk];
            let (range, inline) = if left {
                (&hunk.left, &hunk.left_inline)
            } else {
                (&hunk.right, &hunk.right_inline)
            };
            inline
                .get(index - range.start)
                .map(|ranges| {
                    ranges
                        .iter()
                        .filter_map(|bytes| char_range(raw, bytes))
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    Line {
        number: index + 1,
        // One character for one, so the ranges above stay valid.
        text: raw
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect(),
        status,
        changed,
    }
}

/// Byte range `bytes` of `text` as a character range.
fn char_range(text: &str, bytes: &Range<usize>) -> Option<Range<usize>> {
    let start = text.get(..bytes.start)?.chars().count();
    Some(start..start + text.get(bytes.clone())?.chars().count())
}

/// `2026-10-13 14:05 UTC`.
fn utc_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = ((seconds / 86_400) as i64, seconds % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3_600,
        rest % 3_600 / 60
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::samples;
    use crate::state::{Command, Side};
    use std::time::Duration;

    pub(crate) fn sample() -> AppState {
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

    fn lines(block: &Block) -> usize {
        block
            .rows
            .iter()
            .filter(|row| matches!(row, Row::Lines { .. }))
            .count()
    }

    #[test]
    fn summary_lists_every_change_by_pointer() {
        let report = build(&sample(), Some(3), UNIX_EPOCH);
        let Changes::Structural(changes) = &report.changes else {
            panic!("the sample is valid JSON");
        };
        assert_eq!(changes.len(), 16);
        let totals = report.totals;
        assert_eq!(totals.added + totals.removed + totals.modified, 16);
        let version = changes
            .iter()
            .find(|row| row.pointer == "/version")
            .unwrap();
        assert_eq!(version.kind, ChangeKind::Modified);
        assert_eq!(
            (version.old.as_deref(), version.new.as_deref()),
            (Some("\"2.3.1\""), Some("\"2.4.0\""))
        );
        let tracing = changes
            .iter()
            .find(|row| row.pointer == "/tracing")
            .unwrap();
        assert_eq!(
            (tracing.kind, tracing.old.as_deref()),
            (ChangeKind::Added, None)
        );
        assert_eq!(report.title, "(unsaved text) ↔ (unsaved text)");
    }

    #[test]
    fn ignored_keys_are_left_out_of_the_report() {
        let mut state = sample();
        let all = build(&state, Some(3), UNIX_EPOCH);
        for (pointer, key) in [(Some("/version"), None), (None, Some("tracing"))] {
            state
                .apply(Command::IgnoreChanges {
                    side: Side::Left,
                    pointer: pointer.map(Into::into),
                    key: key.map(Into::into),
                })
                .unwrap();
        }
        let report = build(&state, Some(3), UNIX_EPOCH);
        let Changes::Structural(changes) = &report.changes else {
            panic!("the sample is valid JSON");
        };
        assert!(
            changes
                .iter()
                .all(|row| row.pointer != "/version" && !row.pointer.starts_with("/tracing")),
            "{changes:?}"
        );
        assert_eq!(report.ignored, ["/version", "\"tracing\" everywhere"]);
        assert!(report.ignored_changes >= 2);
        let Changes::Structural(before) = &all.changes else {
            unreachable!()
        };
        assert_eq!(changes.len() + report.ignored_changes, before.len());

        // Nor do their lines show as changes on the side-by-side pages.
        let coloured = report
            .blocks
            .iter()
            .flat_map(|block| &block.rows)
            .filter_map(|row| match row {
                Row::Lines { left, right } => Some([left, right]),
                Row::Skipped(_) => None,
            })
            .flatten()
            .flatten()
            .filter(|line| line.status != LineStatus::Unchanged || !line.changed.is_empty());
        for line in coloured {
            assert!(
                !line.text.contains("\"version\"") && !line.text.contains("\"tracing\""),
                "{line:?}"
            );
        }
        assert!(
            report.totals.blocks < all.totals.blocks,
            "their blocks are gone"
        );
    }

    #[test]
    fn folds_on_screen_do_not_hide_anything() {
        // The app opens folded; the report must still show nested changes.
        let report = build(&sample(), Some(3), UNIX_EPOCH);
        let shows = |needle: &str| {
            report
                .blocks
                .iter()
                .flat_map(|block| &block.rows)
                .any(|row| match row {
                    Row::Lines {
                        right: Some(line), ..
                    } => line.text.contains(needle),
                    _ => false,
                })
        };
        assert!(shows("\"max\": 25"), "a change three levels deep");
        assert!(!shows("{…}"));
    }

    #[test]
    fn context_trims_unchanged_runs_and_merges_nearby_changes() {
        let state = sample();
        let trimmed = build(&state, Some(3), UNIX_EPOCH);
        let whole = build(&state, None, UNIX_EPOCH);
        assert_eq!(whole.blocks.len(), 1);
        assert_eq!(whole.blocks[0].title, "Whole documents");
        assert!(
            !whole.blocks[0]
                .rows
                .iter()
                .any(|row| matches!(row, Row::Skipped(_))),
            "nothing is left out"
        );

        // Nearby changes share blocks, so there are fewer blocks than changes.
        assert!(trimmed.blocks.len() < trimmed.totals.blocks);
        assert!(
            trimmed
                .blocks
                .iter()
                .any(|block| block.title.starts_with("Changes "))
        );
        // Shown plus skipped lines account for the whole document.
        let shown: usize = trimmed.blocks.iter().map(lines).sum();
        let skipped: usize = trimmed
            .blocks
            .iter()
            .flat_map(|block| &block.rows)
            .map(|row| match row {
                Row::Skipped(lines) => *lines,
                Row::Lines { .. } => 0,
            })
            .sum();
        assert_eq!(shown + skipped, lines(&whole.blocks[0]));
        assert!(skipped > 0);
    }

    #[test]
    fn changed_characters_are_character_ranges() {
        assert_eq!(char_range("\"São\": 1", &(6..7)), Some(5..6));
        let report = build(&sample(), Some(3), UNIX_EPOCH);
        let version = report
            .blocks
            .iter()
            .flat_map(|block| &block.rows)
            .find_map(|row| match row {
                Row::Lines {
                    right: Some(line), ..
                } if line.text.contains("\"version\"") => Some(line),
                _ => None,
            })
            .unwrap();
        assert_eq!(version.status, LineStatus::Added);
        let changed: String = version
            .changed
            .iter()
            .map(|range| {
                version
                    .text
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect::<String>()
            })
            .collect();
        assert!(changed.contains('4'), "highlighted {changed:?}");
    }

    #[test]
    fn invalid_or_identical_documents() {
        let mut state = sample();
        state
            .apply(Command::SetText {
                side: Side::Right,
                text: "{ not json".into(),
            })
            .unwrap();
        let report = build(&state, Some(3), UNIX_EPOCH);
        assert_eq!(report.changes, Changes::TextOnly);
        assert!(report.sources[1].error.is_some());
        assert!(!report.blocks.is_empty(), "still compared as text");

        state
            .apply(Command::SetText {
                side: Side::Right,
                text: samples::ORIGINAL.into(),
            })
            .unwrap();
        let report = build(&state, Some(3), UNIX_EPOCH);
        assert_eq!(report.changes, Changes::Structural(Vec::new()));
        assert!(report.blocks.is_empty());
    }

    #[test]
    fn timestamps_are_utc_civil_dates() {
        assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01 00:00 UTC");
        // 2026-10-13 14:05:00 UTC
        let time = UNIX_EPOCH + Duration::from_secs(1_791_900_300);
        assert_eq!(utc_timestamp(time), "2026-10-13 14:05 UTC");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400); // 2000-02-29
        assert_eq!(utc_timestamp(leap), "2000-02-29 00:00 UTC");
    }
}
