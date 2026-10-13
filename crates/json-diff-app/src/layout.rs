//! Where everything in a [`Report`] goes on paper: pages of text and filled
//! boxes, ready for [`crate::pdf`] to draw.
//!
//! Everything is set in one monospaced font in fixed-height rows, so layout
//! is arithmetic: a column holds a known number of characters, a page a known
//! number of rows. Long lines wrap by character count; a line that fits on a
//! page never breaks across two.

use crate::report::{Block, Changes, Line, Report, Row};
use json_diff_core::{ChangeKind, LineStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Advance width of every DejaVu Sans Mono glyph, per point of font size.
pub const ADVANCE: f32 = 1233.0 / 2048.0;

const MARGIN: f32 = 36.0;
/// Space between the two columns of the side-by-side pages.
const GUTTER: f32 = 14.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    /// US Letter, 8.5 × 11 in.
    #[default]
    Letter,
    /// A4, 210 × 297 mm.
    A4,
}

impl Paper {
    /// Width and height in points, upright.
    fn portrait(self) -> (f32, f32) {
        match self {
            Paper::Letter => (612.0, 792.0),
            Paper::A4 => (595.28, 841.89),
        }
    }
}

pub type Color = (u8, u8, u8);

const INK: Color = (30, 30, 30);
const GREY: Color = (120, 120, 120);
const RULE: Color = (215, 215, 215);
const SKIPPED_BAND: Color = (242, 242, 242);
const ADDED_LINE: Color = (222, 245, 222);
const ADDED_CHARS: Color = (165, 218, 165);
const ADDED_INK: Color = (36, 128, 58);
const REMOVED_LINE: Color = (255, 226, 226);
const REMOVED_CHARS: Color = (244, 170, 170);
const REMOVED_INK: Color = (190, 40, 40);
const MODIFIED_INK: Color = (25, 95, 170);

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `y` is the text's baseline; coordinates run from the page's top left.
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        color: Color,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
}

/// An entry in the PDF's outline (bookmarks).
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub page: usize,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub pages: Vec<Page>,
    pub bookmarks: Vec<Bookmark>,
}

pub fn lay_out(report: &Report, paper: Paper) -> Layout {
    let mut layout = Layout {
        pages: Vec::new(),
        bookmarks: Vec::new(),
    };
    summary(&mut layout, report, paper);
    if !report.blocks.is_empty() {
        side_by_side(&mut layout, report, paper);
    }
    footers(&mut layout, report);
    layout
}

/// Rows of text flowing down pages of one size.
struct Flow<'a> {
    layout: &'a mut Layout,
    width: f32,
    height: f32,
    size: f32,
    line: f32,
    /// Top of the first row on a page, and the limit rows must end above.
    top: f32,
    bottom: f32,
    /// Top of the next row.
    y: f32,
    /// Drawn at the top of every new page (the side-by-side title).
    heading: Option<String>,
}

impl<'a> Flow<'a> {
    fn new(layout: &'a mut Layout, (width, height): (f32, f32), size: f32, line: f32) -> Self {
        Self {
            layout,
            width,
            height,
            size,
            line,
            top: MARGIN,
            bottom: height - MARGIN - 14.0,
            y: f32::INFINITY,
            heading: None,
        }
    }

    fn char_width(&self) -> f32 {
        self.size * ADVANCE
    }

    /// Characters that fit in `width` points.
    fn chars_in(&self, width: f32) -> usize {
        (width / self.char_width()).floor() as usize
    }

    fn rows_left(&self) -> usize {
        if self.y.is_infinite() {
            0
        } else {
            ((self.bottom - self.y) / self.line).floor().max(0.0) as usize
        }
    }

    fn rows_per_page(&self) -> usize {
        let header = if self.heading.is_some() {
            2.0 * self.line
        } else {
            0.0
        };
        ((self.bottom - self.top - header) / self.line).floor() as usize
    }

    fn new_page(&mut self) {
        self.layout.pages.push(Page {
            width: self.width,
            height: self.height,
            items: Vec::new(),
        });
        self.y = self.top;
        if let Some(title) = self.heading.clone() {
            let room =
                ((self.width - 2.0 * MARGIN) / ((self.size + 1.0) * ADVANCE)).floor() as usize;
            let title = shorten(&title, room);
            self.text(MARGIN, self.y, self.size + 1.0, true, INK, title);
            self.y += 2.0 * self.line;
        }
    }

    /// Makes room for `rows` rows, starting a page if they don't fit.
    fn reserve(&mut self, rows: usize) {
        if self.rows_left() < rows {
            self.new_page();
        }
    }

    /// The top of a new row, on a new page if this one is full.
    fn row(&mut self) -> f32 {
        self.reserve(1);
        let top = self.y;
        self.y += self.line;
        top
    }

    fn at_page_top(&self) -> bool {
        self.y
            <= self.top
                + if self.heading.is_some() {
                    2.0 * self.line
                } else {
                    0.0
                }
    }

    fn page(&mut self) -> &mut Page {
        self.layout.pages.last_mut().expect("a page was started")
    }

    /// Baseline for text in the row starting at `top`.
    fn baseline(&self, top: f32) -> f32 {
        top + (self.line - self.size) / 2.0 + self.size * 0.78
    }

    fn text(&mut self, x: f32, top: f32, size: f32, bold: bool, color: Color, text: String) {
        let y = self.baseline(top);
        if !text.is_empty() {
            self.page().items.push(Item::Text {
                x,
                y,
                size,
                bold,
                color,
                text,
            });
        }
    }

    fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.page().items.push(Item::Rect {
            x,
            y,
            width,
            height,
            color,
        });
    }

    fn bookmark(&mut self, title: String) {
        let page = self.layout.pages.len() - 1;
        let y = self.y;
        self.layout.bookmarks.push(Bookmark { title, page, y });
    }
}

/// The first pages: what was compared, and every change by pointer.
fn summary(layout: &mut Layout, report: &Report, paper: Paper) {
    let mut flow = Flow::new(layout, paper.portrait(), 8.0, 11.0);
    flow.new_page();
    flow.bookmark("Summary".to_owned());
    let title_top = flow.row();
    flow.text(
        MARGIN,
        title_top,
        15.0,
        true,
        INK,
        "json-diff report".to_owned(),
    );
    flow.y += flow.line;

    let columns = flow.chars_in(flow.width - 2.0 * MARGIN);
    let paragraph = |flow: &mut Flow, text: &str, bold: bool, color: Color| {
        for piece in wrap(text, columns) {
            let top = flow.row();
            flow.text(MARGIN, top, flow.size, bold, color, piece);
        }
    };
    for (name, source) in ["Original", "Modified"].iter().zip(&report.sources) {
        // A file opened in the browser has only its name, already shown.
        let path = source
            .path
            .as_ref()
            .filter(|path| **path != source.label)
            .map_or_else(String::new, |path| format!("  ({path})"));
        paragraph(
            &mut flow,
            &format!("{name:<9} {}{path}", source.label),
            false,
            INK,
        );
        if let Some(error) = &source.error {
            paragraph(
                &mut flow,
                &format!("{:9} not valid JSON: {error}", ""),
                false,
                REMOVED_INK,
            );
        }
    }
    let on = |enabled: bool| if enabled { "on" } else { "off" };
    let options = &report.options;
    paragraph(
        &mut flow,
        &format!(
            "Options   Format JSON {} · Sort keys {} · Ignore whitespace {}",
            on(options.format),
            on(options.sort_keys),
            on(options.ignore_whitespace)
        ),
        false,
        INK,
    );
    if !report.ignored.is_empty() {
        paragraph(
            &mut flow,
            &format!(
                "Ignoring  {} ({} {} left out)",
                report.ignored.join(" · "),
                report.ignored_changes,
                if report.ignored_changes == 1 {
                    "change"
                } else {
                    "changes"
                },
            ),
            false,
            INK,
        );
    }
    paragraph(
        &mut flow,
        &format!("Made with {} on {}", report.producer, report.generated),
        false,
        GREY,
    );
    flow.y += flow.line;

    let totals = report.totals;
    match &report.changes {
        Changes::Structural(changes) if changes.is_empty() => {
            paragraph(&mut flow, "The documents are identical.", true, INK);
        }
        Changes::Structural(changes) => {
            paragraph(
                &mut flow,
                &format!(
                    "{} changes: {} added, {} removed, {} modified · {} changed blocks · +{} −{} lines",
                    changes.len(),
                    totals.added,
                    totals.removed,
                    totals.modified,
                    totals.blocks,
                    totals.added_lines,
                    totals.removed_lines
                ),
                true,
                INK,
            );
            flow.y += flow.line;
            change_table(&mut flow, changes, columns);
        }
        Changes::TextOnly => paragraph(
            &mut flow,
            &format!(
                "Compared as text: a document is not valid JSON, so there is no list of changes. \
                 The side-by-side pages show every difference ({} changed blocks, +{} −{} lines).",
                totals.blocks, totals.added_lines, totals.removed_lines
            ),
            false,
            INK,
        ),
    }
}

fn change_table(flow: &mut Flow, changes: &[crate::report::ChangeRow], columns: usize) {
    const GAP: usize = 2;
    let number = changes.len().to_string().len().max(1) + 1;
    let kind = "modified".len();
    let rest = columns.saturating_sub(number + kind + 4 * GAP);
    let pointer = rest * 3 / 10;
    let old = (rest - pointer) / 2;
    let new = rest - pointer - old;
    let widths = [number, kind, pointer, old, new];
    let cw = flow.char_width();
    let xs: Vec<f32> = widths
        .iter()
        .scan(MARGIN, |x, width| {
            let at = *x;
            *x += (width + GAP) as f32 * cw;
            Some(at)
        })
        .collect();

    let header = |flow: &mut Flow| {
        let top = flow.row();
        for (x, title) in xs
            .iter()
            .zip(["#", "Change", "JSON pointer", "Original", "New"])
        {
            flow.text(*x, top, flow.size, true, INK, title.to_owned());
        }
        let width = flow.width - 2.0 * MARGIN;
        flow.rect(MARGIN, top + flow.line - 1.0, width, 0.75, GREY);
    };
    header(flow);

    for (index, change) in changes.iter().enumerate() {
        let (label, color) = match change.kind {
            ChangeKind::Added => ("added", ADDED_INK),
            ChangeKind::Removed => ("removed", REMOVED_INK),
            ChangeKind::Modified => ("modified", MODIFIED_INK),
        };
        let cells = [
            wrap(&(index + 1).to_string(), widths[0]),
            wrap(label, widths[1]),
            wrap(&change.pointer, widths[2]),
            wrap(change.old.as_deref().unwrap_or("—"), widths[3]),
            wrap(change.new.as_deref().unwrap_or("—"), widths[4]),
        ];
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        if flow.rows_left() < height {
            flow.new_page();
            header(flow);
        }
        for row in 0..height {
            let top = flow.row();
            for (column, (x, cell)) in xs.iter().zip(&cells).enumerate() {
                if let Some(piece) = cell.get(row) {
                    let (bold, ink) = if column == 1 {
                        (true, color)
                    } else {
                        (false, INK)
                    };
                    flow.text(*x, top, flow.size, bold, ink, piece.clone());
                }
            }
        }
        let width = flow.width - 2.0 * MARGIN;
        let y = flow.y - 0.5;
        flow.rect(MARGIN, y, width, 0.5, RULE);
    }
}

/// The side-by-side pages, landscape: each block with its context.
fn side_by_side(layout: &mut Layout, report: &Report, paper: Paper) {
    let (width, height) = paper.portrait();
    let mut flow = Flow::new(layout, (height, width), 7.0, 9.0);
    flow.heading = Some(report.title.clone());

    let column = (flow.width - 2.0 * MARGIN - GUTTER) / 2.0;
    let chars = flow.chars_in(column);
    let widest = report
        .blocks
        .iter()
        .flat_map(|block| &block.rows)
        .filter_map(|row| match row {
            Row::Lines { left, right } => Some(
                left.iter()
                    .chain(right)
                    .map(|line| line.number)
                    .max()
                    .unwrap_or(1),
            ),
            Row::Skipped(_) => None,
        })
        .max()
        .unwrap_or(1);
    let columns = Columns {
        x: [MARGIN, MARGIN + column + GUTTER],
        width: column,
        number: widest.to_string().len().max(3),
        text: chars
            .saturating_sub(widest.to_string().len().max(3) + 3)
            .max(10),
    };

    let mut first = true;
    for block in &report.blocks {
        if first {
            flow.new_page();
            first = false;
        } else if !flow.at_page_top() {
            flow.y += flow.line;
        }
        // Keep a heading with at least its first lines.
        flow.reserve(3);
        flow.bookmark(block.title.clone());
        let top = flow.row();
        flow.text(
            MARGIN,
            top,
            flow.size,
            true,
            MODIFIED_INK,
            block.title.clone(),
        );
        draw_block(&mut flow, block, &columns);
    }
}

/// Geometry of the two columns on side-by-side pages.
struct Columns {
    x: [f32; 2],
    width: f32,
    /// Characters for line numbers, and for the text after them.
    number: usize,
    text: usize,
}

fn draw_block(flow: &mut Flow, block: &Block, columns: &Columns) {
    for row in &block.rows {
        match row {
            Row::Skipped(lines) => {
                let top = flow.row();
                let width = flow.width - 2.0 * MARGIN;
                flow.rect(MARGIN, top, width, flow.line, SKIPPED_BAND);
                let text = format!(
                    "⋯ {lines} unchanged line{}",
                    if *lines == 1 { "" } else { "s" }
                );
                let x = MARGIN + (width - text.chars().count() as f32 * flow.char_width()) / 2.0;
                flow.text(x, top, flow.size, false, GREY, text);
            }
            Row::Lines { left, right } => {
                let pieces = [left, right].map(|line| {
                    line.as_ref()
                        .map(|line| wrap(&line.text, columns.text))
                        .unwrap_or_default()
                });
                let height = pieces.iter().map(Vec::len).max().unwrap_or(1).max(1);
                if height <= flow.rows_per_page() {
                    flow.reserve(height);
                }
                for row in 0..height {
                    let top = flow.row();
                    for (side, line) in [left, right].into_iter().enumerate() {
                        if let Some(line) = line {
                            draw_line_row(flow, columns, side, line, &pieces[side], row, top);
                        }
                    }
                }
            }
        }
    }
}

/// Physical row `row` of `line` (wrapped into `pieces`) in column `side`.
fn draw_line_row(
    flow: &mut Flow,
    columns: &Columns,
    side: usize,
    line: &Line,
    pieces: &[String],
    row: usize,
    top: f32,
) {
    let cw = flow.char_width();
    let x = columns.x[side];
    let text_x = x + (columns.number + 3) as f32 * cw;
    let (background, highlight, ink, sign) = match line.status {
        LineStatus::Added => (Some(ADDED_LINE), ADDED_CHARS, ADDED_INK, "+"),
        LineStatus::Removed => (Some(REMOVED_LINE), REMOVED_CHARS, REMOVED_INK, "−"),
        LineStatus::Unchanged => (None, RULE, GREY, ""),
    };
    if let Some(background) = background {
        flow.rect(x, top, columns.width, flow.line, background);
        // Changed characters within this piece of the line.
        let start = row * columns.text;
        let end = start + pieces.get(row).map_or(0, |piece| piece.chars().count());
        for range in &line.changed {
            let (from, to) = (range.start.max(start), range.end.min(end));
            if from < to {
                let at = text_x + (from - start) as f32 * cw;
                flow.rect(
                    at,
                    top + 0.5,
                    (to - from) as f32 * cw,
                    flow.line - 1.0,
                    highlight,
                );
            }
        }
    }
    if row == 0 {
        let number = format!("{:>width$}", line.number, width = columns.number);
        flow.text(x, top, flow.size, false, GREY, number);
        let sign_x = x + (columns.number + 1) as f32 * cw;
        flow.text(sign_x, top, flow.size, true, ink, sign.to_owned());
    } else if row < pieces.len() {
        let sign_x = x + (columns.number + 1) as f32 * cw;
        flow.text(sign_x, top, flow.size, false, GREY, "↪".to_owned());
    }
    if let Some(piece) = pieces.get(row) {
        flow.text(text_x, top, flow.size, false, INK, piece.clone());
    }
}

/// "page N of M" and the producer on every page, once the count is known.
fn footers(layout: &mut Layout, report: &Report) {
    let total = layout.pages.len();
    for (index, page) in layout.pages.iter_mut().enumerate() {
        let y = page.height - MARGIN + 4.0;
        let size = 7.0;
        let label = format!("page {} of {total}", index + 1);
        let x = page.width - MARGIN - label.chars().count() as f32 * size * ADVANCE;
        page.items.push(Item::Text {
            x,
            y,
            size,
            bold: false,
            color: GREY,
            text: label,
        });
        page.items.push(Item::Text {
            x: MARGIN,
            y,
            size,
            bold: false,
            color: GREY,
            text: report.producer.clone(),
        });
    }
}

/// `text` cut to `width` characters, ending in "…" when cut.
fn shorten(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        let kept: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

/// `text` cut into pieces of at most `width` characters; at least one.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() || width == 0 {
        return vec![text.to_owned()];
    }
    chars
        .chunks(width)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{self, Source};
    use json_diff_core::DiffOptions;
    use std::time::UNIX_EPOCH;

    fn sample_layout(paper: Paper) -> Layout {
        let state = crate::report::tests::sample();
        lay_out(&report::build(&state, Some(3), UNIX_EPOCH), paper)
    }

    fn texts(page: &Page) -> Vec<&str> {
        page.items
            .iter()
            .filter_map(|item| match item {
                Item::Text { text, .. } => Some(text.as_str()),
                Item::Rect { .. } => None,
            })
            .collect()
    }

    #[test]
    fn summary_is_portrait_and_side_by_side_landscape() {
        let layout = sample_layout(Paper::Letter);
        assert!(layout.pages.len() >= 2);
        assert_eq!(
            (layout.pages[0].width, layout.pages[0].height),
            (612.0, 792.0)
        );
        let last = layout.pages.last().unwrap();
        assert_eq!((last.width, last.height), (792.0, 612.0));
        assert!(texts(&layout.pages[0]).contains(&"json-diff report"));
        assert!(
            texts(&layout.pages[0])
                .iter()
                .any(|text| text.contains("/runtime/timeout"))
        );

        let a4 = sample_layout(Paper::A4);
        assert_eq!((a4.pages[0].width, a4.pages[0].height), (595.28, 841.89));
    }

    #[test]
    fn nothing_leaves_the_page() {
        for paper in [Paper::Letter, Paper::A4] {
            for page in sample_layout(paper).pages {
                for item in &page.items {
                    let (left, right, top, bottom) = match item {
                        Item::Text {
                            x, y, size, text, ..
                        } => (
                            *x,
                            x + text.chars().count() as f32 * size * ADVANCE,
                            y - size,
                            *y,
                        ),
                        Item::Rect {
                            x,
                            y,
                            width,
                            height,
                            ..
                        } => (*x, x + width, *y, y + height),
                    };
                    let slack = 0.01;
                    assert!(
                        left >= MARGIN - slack && right <= page.width - MARGIN + slack,
                        "{item:?}"
                    );
                    assert!(
                        top >= MARGIN - 8.0 && bottom <= page.height - MARGIN + 6.0,
                        "{item:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_page_is_numbered_and_every_block_bookmarked() {
        let layout = sample_layout(Paper::Letter);
        let total = layout.pages.len();
        for (index, page) in layout.pages.iter().enumerate() {
            let label = format!("page {} of {total}", index + 1);
            assert!(texts(page).contains(&label.as_str()), "{label}");
        }
        let report = report::build(&crate::report::tests::sample(), Some(3), UNIX_EPOCH);
        assert_eq!(layout.bookmarks[0].title, "Summary");
        assert_eq!(layout.bookmarks.len(), 1 + report.blocks.len());
    }

    #[test]
    fn long_lines_wrap_with_a_marker_and_keep_together() {
        let long = "x".repeat(400);
        let line = |status| Line {
            number: 7,
            text: long.clone(),
            status,
            changed: std::iter::once(395..400).collect(),
        };
        let source = Source {
            label: "a.json".into(),
            path: None,
            error: None,
        };
        let report = Report {
            title: "a.json ↔ b.json".into(),
            sources: [source.clone(), source],
            options: DiffOptions::default(),
            ignored: Vec::new(),
            ignored_changes: 0,
            producer: "json-diff test".into(),
            generated: "now".into(),
            changes: Changes::TextOnly,
            totals: Default::default(),
            blocks: vec![Block {
                title: "Change 1 of 1".into(),
                rows: vec![Row::Lines {
                    left: Some(line(LineStatus::Removed)),
                    right: Some(line(LineStatus::Added)),
                }],
            }],
        };
        let layout = lay_out(&report, Paper::Letter);
        let page = layout.pages.last().unwrap();
        let pieces: Vec<&str> = texts(page)
            .into_iter()
            .filter(|text| text.starts_with('x'))
            .collect();
        assert!(
            pieces.len() >= 8,
            "400 characters wrap into several rows per side"
        );
        assert_eq!(pieces.iter().map(|piece| piece.len()).sum::<usize>(), 800);
        assert!(texts(page).contains(&"↪"));
        // The last characters are highlighted on the last row of each side.
        let highlights = page
            .items
            .iter()
            .filter(|item| matches!(item, Item::Rect { color, .. } if *color == ADDED_CHARS || *color == REMOVED_CHARS))
            .count();
        assert_eq!(highlights, 2);
    }

    #[test]
    fn wrap_splits_by_characters() {
        assert_eq!(wrap("abcdef", 4), vec!["abcd", "ef"]);
        assert_eq!(wrap("", 4), vec![""]);
        assert_eq!(wrap("ünïcødé", 3), vec!["ünï", "cød", "é"]);
    }
}
