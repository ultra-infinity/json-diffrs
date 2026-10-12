//! Line-oriented side-by-side diff with intra-line (word level) highlights,
//! the model behind a VS Code style two-pane diff editor.

use crate::format::FoldMark;
use serde::Serialize;
use similar::{Algorithm, DiffOp, capture_diff_slices, capture_diff_slices_by_key};
use std::ops::Range;

/// Changed byte ranges for each line of a block.
type LineRanges = Vec<Vec<Range<usize>>>;

/// Hunks with more lines than this get no word-level highlights.
const INLINE_DIFF_MAX_LINES: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HunkKind {
    /// Lines exist only on the right.
    Added,
    /// Lines exist only on the left.
    Removed,
    /// Lines on the left were replaced by lines on the right.
    Modified,
}

/// A contiguous run of changed lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hunk {
    pub kind: HunkKind,
    /// Left (original) line indices. Empty for [`HunkKind::Added`]; its start
    /// is then the line the insertion sits before.
    pub left: Range<usize>,
    /// Right (modified) line indices. Empty for [`HunkKind::Removed`].
    pub right: Range<usize>,
    /// Changed byte ranges per left line of the hunk (word-level highlights).
    #[serde(skip)]
    pub left_inline: Vec<Vec<Range<usize>>>,
    /// Changed byte ranges per right line of the hunk.
    #[serde(skip)]
    pub right_inline: Vec<Vec<Range<usize>>>,
}

/// One aligned block of the two documents: either unchanged lines or a hunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub left: Range<usize>,
    pub right: Range<usize>,
    pub hunk: Option<usize>,
    /// First aligned row of this segment (see [`SideBySide::aligned_rows`]).
    pub aligned_start: usize,
}

impl Segment {
    /// Rows this segment occupies in aligned space: the taller of its sides.
    pub fn aligned_len(&self) -> usize {
        self.left.len().max(self.right.len())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SideBySide {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub hunks: Vec<Hunk>,
    /// Covers both documents completely and in order.
    pub segments: Vec<Segment>,
    /// What happened to each left line; drives its colour.
    pub left_status: Vec<LineStatus>,
    /// What happened to each right line.
    pub right_status: Vec<LineStatus>,
    /// Foldable containers on left lines (formatted JSON only).
    pub left_folds: Vec<Option<FoldMark>>,
    /// Foldable containers on right lines.
    pub right_folds: Vec<Option<FoldMark>>,
}

/// The colour class of one line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStatus {
    /// Same value on both sides (possibly reformatted, e.g. a new trailing comma).
    #[default]
    Unchanged,
    /// Exists only in the modified document (green).
    Added,
    /// Exists only in the original document (red).
    Removed,
    /// Present on both sides with a different value (yellow).
    Modified,
}

impl SideBySide {
    /// Raw texts compared line by line.
    pub fn new(left: &str, right: &str, ignore_whitespace: bool) -> Self {
        let left: Vec<String> = left.lines().map(str::to_owned).collect();
        let right: Vec<String> = right.lines().map(str::to_owned).collect();

        let ops = if ignore_whitespace {
            let key = |line: &String| line.split_whitespace().collect::<String>();
            capture_diff_slices_by_key(Algorithm::Patience, &left, &right, key)
        } else {
            capture_diff_slices(Algorithm::Patience, &left, &right)
        };
        Self::from_ops(left, right, &ops)
    }

    /// Lines aligned by a caller-supplied key per line (one key per line, see
    /// [`crate::format::FormattedJson::keys`]) instead of by their text.
    pub fn with_keys(
        left: Vec<String>,
        right: Vec<String>,
        left_keys: &[String],
        right_keys: &[String],
    ) -> Self {
        debug_assert_eq!(left.len(), left_keys.len());
        debug_assert_eq!(right.len(), right_keys.len());
        let ops = capture_diff_slices(Algorithm::Patience, left_keys, right_keys);
        Self::from_ops(left, right, &ops)
    }

    fn from_ops(left: Vec<String>, right: Vec<String>, ops: &[DiffOp]) -> Self {
        let mut hunks: Vec<Hunk> = Vec::new();
        let mut segments: Vec<Segment> = Vec::new();
        let mut aligned = 0;
        for op in ops {
            let (left_range, right_range) = (op.old_range(), op.new_range());
            let kind = match op {
                DiffOp::Equal { .. } => None,
                DiffOp::Insert { .. } => Some(HunkKind::Added),
                DiffOp::Delete { .. } => Some(HunkKind::Removed),
                DiffOp::Replace { .. } => Some(HunkKind::Modified),
            };

            // similar may emit a Delete directly followed by an Insert; that
            // is one replacement for display purposes.
            if let (Some(kind), Some(last)) = (kind, segments.last_mut())
                && let Some(index) = last.hunk
                && hunks[index].left.end == left_range.start
                && hunks[index].right.end == right_range.start
            {
                let hunk = &mut hunks[index];
                hunk.left.end = left_range.end;
                hunk.right.end = right_range.end;
                if hunk.kind != kind {
                    hunk.kind = HunkKind::Modified;
                }
                last.left = hunk.left.clone();
                last.right = hunk.right.clone();
                aligned = last.aligned_start + last.aligned_len();
                continue;
            }

            let hunk = kind.map(|kind| {
                hunks.push(Hunk {
                    kind,
                    left: left_range.clone(),
                    right: right_range.clone(),
                    left_inline: Vec::new(),
                    right_inline: Vec::new(),
                });
                hunks.len() - 1
            });
            let segment = Segment {
                left: left_range,
                right: right_range,
                hunk,
                aligned_start: aligned,
            };
            aligned += segment.aligned_len();
            segments.push(segment);
        }

        for hunk in &mut hunks {
            if hunk.kind == HunkKind::Modified
                && hunk.left.len() + hunk.right.len() <= INLINE_DIFF_MAX_LINES
            {
                let (left_inline, right_inline) =
                    inline_changes(&left[hunk.left.clone()], &right[hunk.right.clone()]);
                hunk.left_inline = left_inline;
                hunk.right_inline = right_inline;
            }
        }

        // Without semantic information, a block's kind is all we know.
        let mut left_status = vec![LineStatus::Unchanged; left.len()];
        let mut right_status = vec![LineStatus::Unchanged; right.len()];
        for hunk in &hunks {
            let (left_kind, right_kind) = match hunk.kind {
                HunkKind::Added => (LineStatus::Unchanged, LineStatus::Added),
                HunkKind::Removed => (LineStatus::Removed, LineStatus::Unchanged),
                HunkKind::Modified => (LineStatus::Modified, LineStatus::Modified),
            };
            left_status[hunk.left.clone()].fill(left_kind);
            right_status[hunk.right.clone()].fill(right_kind);
        }

        Self {
            left_folds: vec![None; left.len()],
            right_folds: vec![None; right.len()],
            left,
            right,
            hunks,
            segments,
            left_status,
            right_status,
        }
    }

    pub fn folds(&self, left: bool) -> &[Option<FoldMark>] {
        if left {
            &self.left_folds
        } else {
            &self.right_folds
        }
    }

    pub fn status(&self, left: bool) -> &[LineStatus] {
        if left {
            &self.left_status
        } else {
            &self.right_status
        }
    }

    /// The strongest status among one side's lines of hunk `index`:
    /// modified, then added/removed, then unchanged.
    pub fn hunk_status(&self, index: usize, left: bool) -> LineStatus {
        let hunk = &self.hunks[index];
        let range = if left {
            hunk.left.clone()
        } else {
            hunk.right.clone()
        };
        let statuses = &self.status(left)[range];
        if statuses.contains(&LineStatus::Modified) {
            LineStatus::Modified
        } else {
            statuses
                .iter()
                .copied()
                .find(|status| *status != LineStatus::Unchanged)
                .unwrap_or(LineStatus::Unchanged)
        }
    }

    /// Lines on one side with `status`.
    pub fn count(&self, left: bool, status: LineStatus) -> usize {
        self.status(left)
            .iter()
            .filter(|line| **line == status)
            .count()
    }

    /// Height of the aligned view, in rows: every segment takes as many rows
    /// as its taller side. Both panes scroll through this shared space.
    pub fn aligned_rows(&self) -> usize {
        self.segments
            .last()
            .map_or(0, |segment| segment.aligned_start + segment.aligned_len())
    }

    /// The fractional (left, right) line shown at aligned row `row`. Inside a
    /// hunk each side advances proportionally to its own length, so the two
    /// panes drift apart smoothly and meet again after it.
    pub fn line_at(&self, row: f32) -> (f32, f32) {
        let row = row.max(0.0);
        let index = self
            .segments
            .partition_point(|segment| (segment.aligned_start as f32) <= row)
            .saturating_sub(1);
        let Some(segment) = self.segments.get(index) else {
            return (row, row);
        };

        let height = segment.aligned_len().max(1) as f32;
        let t = ((row - segment.aligned_start as f32) / height).clamp(0.0, 1.0);
        // Past the end, both sides keep scrolling one line per row.
        let overflow = (row - (segment.aligned_start + segment.aligned_len()) as f32).max(0.0);
        (
            segment.left.start as f32 + t * segment.left.len() as f32 + overflow,
            segment.right.start as f32 + t * segment.right.len() as f32 + overflow,
        )
    }

    /// The aligned row at which hunk `index` starts.
    pub fn hunk_row(&self, index: usize) -> Option<usize> {
        self.segments
            .iter()
            .find(|segment| segment.hunk == Some(index))
            .map(|segment| segment.aligned_start)
    }

    /// The hunk covering `line` on the given side, if any.
    pub fn hunk_at_line(&self, side_is_left: bool, line: usize) -> Option<usize> {
        self.hunks.iter().position(|hunk| {
            let range = if side_is_left {
                &hunk.left
            } else {
                &hunk.right
            };
            range.contains(&line)
        })
    }

    pub fn added_lines(&self) -> usize {
        self.count(false, LineStatus::Added)
    }

    pub fn removed_lines(&self) -> usize {
        self.count(true, LineStatus::Removed)
    }

    /// Lines in the modified document whose value changed in place.
    pub fn modified_lines(&self) -> usize {
        self.count(false, LineStatus::Modified)
    }
}

/// Word-level changed byte ranges for each line of a replaced block.
fn inline_changes(left: &[String], right: &[String]) -> (LineRanges, LineRanges) {
    let left_text = left.join("\n");
    let right_text = right.join("\n");
    let left_tokens = tokenize(&left_text);
    let right_tokens = tokenize(&right_text);
    let left_words: Vec<&str> = left_tokens
        .iter()
        .map(|range| &left_text[range.clone()])
        .collect();
    let right_words: Vec<&str> = right_tokens
        .iter()
        .map(|range| &right_text[range.clone()])
        .collect();

    let mut left_changed = Vec::new();
    let mut right_changed = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &left_words, &right_words) {
        if matches!(op, DiffOp::Equal { .. }) {
            continue;
        }
        left_changed.extend(left_tokens[op.old_range()].iter().cloned());
        right_changed.extend(right_tokens[op.new_range()].iter().cloned());
    }

    (
        split_by_line(left, &left_text, left_changed),
        split_by_line(right, &right_text, right_changed),
    )
}

/// Splits `text` into words, whitespace runs, and single punctuation chars.
/// Newlines are their own tokens so highlights never span lines.
fn tokenize(text: &str) -> Vec<Range<usize>> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Other,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() && c != '\n' {
            Class::Space
        } else {
            Class::Other
        }
    };

    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let kind = class(c);
        let mut end = start + c.len_utf8();
        if kind != Class::Other {
            while let Some(&(next, d)) = chars.peek() {
                if class(d) != kind {
                    break;
                }
                end = next + d.len_utf8();
                chars.next();
            }
        }
        tokens.push(start..end);
    }
    tokens
}

/// Maps changed ranges in the newline-joined `text` back to per-line byte
/// ranges, merging adjacent ranges.
fn split_by_line(
    lines: &[String],
    text: &str,
    mut changed: Vec<Range<usize>>,
) -> Vec<Vec<Range<usize>>> {
    changed.retain(|range| &text[range.clone()] != "\n");
    changed.sort_by_key(|range| range.start);

    let mut per_line = vec![Vec::<Range<usize>>::new(); lines.len()];
    let mut line = 0;
    let mut line_start = 0;
    for range in changed {
        while line < lines.len() && range.start > line_start + lines[line].len() {
            line_start += lines[line].len() + 1;
            line += 1;
        }
        let Some(ranges) = per_line.get_mut(line) else {
            break;
        };
        let local = range.start - line_start..range.end - line_start;
        match ranges.last_mut() {
            Some(last) if last.end == local.start => last.end = local.end,
            _ => ranges.push(local),
        }
    }
    per_line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_is_an_added_hunk_with_empty_left() {
        let diff = SideBySide::new("a\nb\nc", "a\nb\nx\ny\nc", false);
        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        assert_eq!(hunk.kind, HunkKind::Added);
        assert_eq!(hunk.left, 2..2);
        assert_eq!(hunk.right, 2..4);
        assert_eq!(diff.added_lines(), 2);
        assert_eq!(diff.removed_lines(), 0);
    }

    #[test]
    fn replacement_has_inline_highlights() {
        let diff = SideBySide::new("{\n  \"a\": 1,\n}", "{\n  \"a\": 2,\n}", false);
        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        assert_eq!(hunk.kind, HunkKind::Modified);
        assert_eq!(hunk.left_inline, vec![vec![7..8]]);
        assert_eq!(hunk.right_inline, vec![vec![7..8]]);
    }

    #[test]
    fn ignore_whitespace() {
        let diff = SideBySide::new("a\n  b", "a\nb", true);
        assert!(diff.hunks.is_empty());
        let diff = SideBySide::new("a\n  b", "a\nb", false);
        assert_eq!(diff.hunks.len(), 1);
    }

    #[test]
    fn aligned_mapping_meets_after_hunk() {
        // Two lines inserted after "b": aligned rows = 2 (a,b) + 2 + 1 (c).
        let diff = SideBySide::new("a\nb\nc", "a\nb\nx\ny\nc", false);
        assert_eq!(diff.aligned_rows(), 5);
        assert_eq!(diff.line_at(1.0), (1.0, 1.0));
        // Halfway through the insertion the left side stands still.
        assert_eq!(diff.line_at(3.0), (2.0, 3.0));
        assert_eq!(diff.line_at(4.0), (2.0, 4.0));
        assert_eq!(diff.hunk_row(0), Some(2));
    }

    #[test]
    fn segments_cover_both_documents() {
        let diff = SideBySide::new("1\n2\n3\n4", "0\n2\n4\n5", false);
        let left: usize = diff.segments.iter().map(|s| s.left.len()).sum();
        let right: usize = diff.segments.iter().map(|s| s.right.len()).sum();
        assert_eq!(left, 4);
        assert_eq!(right, 4);
    }
}
