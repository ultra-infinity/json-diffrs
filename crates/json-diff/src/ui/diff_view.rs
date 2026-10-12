//! The side-by-side diff view.
//!
//! Each pane shows its document without filler lines. Changed blocks are boxed
//! (green on the right for additions, red on the left for removals), and a
//! ribbon in the gutter between the panes connects every block to the place
//! it corresponds to on the other side; an insertion's ribbon narrows to the
//! exact line on the left where the new lines were added.
//!
//! Both panes scroll through one shared "aligned" space in which each block
//! is as tall as its taller side. An anchor slides from the top of the view
//! (scrolled to the top) to the bottom (scrolled to the end); each pane is
//! placed so that the line it shows at the anchor's aligned row sits at the
//! anchor, so both documents start and end together and drift apart only
//! around blocks of different height.

use super::{Palette, json_line_job};
use crate::state::{AppState, Command, Side};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Mesh, Pos2, Rangef, Rect, Sense,
    Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, pos2, vec2,
};
use json_diff_core::{Hunk, LineStatus, SideBySide};

const CONNECTOR_WIDTH: f32 = 56.0;
const RULER_WIDTH: f32 = 14.0;
const HEADER_HEIGHT: f32 = 26.0;
const SIGN_WIDTH: f32 = 16.0;
/// The fold chevron column, between line numbers and the change sign.
const FOLD_WIDTH: f32 = 16.0;
/// Thickness of the horizontal scrollbar.
const SCROLLBAR_SIZE: f32 = 10.0;

/// Where each pane's lines land on screen for the current scroll position.
struct Placement {
    /// Screen y of the anchor.
    anchor_y: f32,
    /// Fractional line of each side at the anchor.
    anchor_line: [f32; 2],
    row_height: f32,
}

impl Placement {
    fn y(&self, side: Side, line: usize) -> f32 {
        self.anchor_y + (line as f32 - self.anchor_line[side.index()]) * self.row_height
    }

    fn line_at(&self, side: Side, y: f32) -> f32 {
        (y - self.anchor_y) / self.row_height + self.anchor_line[side.index()]
    }
}

pub fn diff_view(
    ui: &mut Ui,
    state: &mut AppState,
    palette: &Palette,
    commands: &mut Vec<Command>,
) {
    if state.docs.iter().all(|doc| doc.text.is_empty()) {
        empty_state(ui);
        return;
    }

    let font = TextStyle::Monospace.resolve(ui.style());
    let row_height = (ui.fonts_mut(|fonts| fonts.row_height(&font)) * 1.4).round();
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let painter = ui.painter_at(rect);
    let view = &state.diff.view;

    // Geometry.
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_HEIGHT));
    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    let ruler = Rect::from_min_max(pos2(body.right() - RULER_WIDTH, body.top()), body.max);
    let pane_width = ((body.width() - RULER_WIDTH - CONNECTOR_WIDTH) / 2.0).max(40.0);
    let panes = [
        Rect::from_min_size(body.min, vec2(pane_width, body.height())),
        Rect::from_min_size(
            pos2(body.left() + pane_width + CONNECTOR_WIDTH, body.top()),
            vec2(pane_width, body.height()),
        ),
    ];
    let connector = Rect::from_min_max(
        pos2(panes[0].right(), body.top()),
        pos2(panes[1].left(), body.bottom()),
    );
    let digit_width = ui.fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
    let digits = view
        .left
        .len()
        .max(view.right.len())
        .max(1)
        .to_string()
        .len() as f32;
    let gutter = digits * digit_width + 12.0 + FOLD_WIDTH + SIGN_WIDTH;

    // Scrolling, VS Code style: the wheel or trackpad (Shift for horizontal)
    // over the view; keys while the view is hovered or focused; a draggable
    // thumb over the overview ruler; a horizontal scrollbar for long lines.
    let scroll_id = ui.id().with("diff_scroll");
    let mut scroll: Vec2 = ui.data(|data| data.get_temp(scroll_id)).unwrap_or_default();
    let total_height = view.aligned_rows() as f32 * row_height;
    let max_scroll_y = (total_height - body.height() + row_height).max(0.0);
    let longest = view
        .left
        .iter()
        .chain(&view.right)
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let text_width = (pane_width - gutter).max(1.0);
    let max_scroll_x = (longest as f32 * digit_width - text_width + 24.0).max(0.0);

    if response.clicked() {
        response.request_focus();
    }
    let hovered = ui.rect_contains_pointer(body);
    // Keys belong to a text field being typed in (e.g. the assistant input).
    let typing_elsewhere = ui.ctx().text_edit_focused();
    if hovered {
        scroll -= ui.input(|input| input.smooth_scroll_delta);
    }
    if (hovered || response.has_focus()) && !typing_elsewhere {
        let page = (body.height() - 2.0 * row_height).max(row_height);
        ui.input(|input| {
            for (key, delta) in [
                (egui::Key::ArrowDown, vec2(0.0, row_height)),
                (egui::Key::ArrowUp, vec2(0.0, -row_height)),
                (egui::Key::PageDown, vec2(0.0, page)),
                (egui::Key::PageUp, vec2(0.0, -page)),
                (egui::Key::ArrowRight, vec2(digit_width * 4.0, 0.0)),
                (egui::Key::ArrowLeft, vec2(-digit_width * 4.0, 0.0)),
            ] {
                if input.key_pressed(key) {
                    scroll += delta;
                }
            }
            if input.key_pressed(egui::Key::Home) {
                scroll = Vec2::ZERO;
            }
            if input.key_pressed(egui::Key::End) {
                scroll.y = max_scroll_y;
            }
        });
    }
    if state.scroll_to_current {
        state.scroll_to_current = false;
        if let Some(row) = state.current_hunk.and_then(|index| view.hunk_row(index)) {
            scroll.y = row as f32 * row_height - body.height() / 3.0;
        }
    }

    // The vertical scrollbar: a thumb over the overview ruler.
    let vertical = Scrollbar::new(
        ruler.y_range(),
        body.height(),
        total_height + row_height,
        max_scroll_y,
    );
    let ruler_response = ui.interact(ruler, scroll_id.with("ruler"), Sense::click_and_drag());
    if let Some(pointer) = ruler_response.interact_pointer_pos() {
        scroll.y = vertical.drag(ui, scroll_id.with("ruler_grab"), pointer.y, scroll.y);
    } else {
        Scrollbar::release(ui, scroll_id.with("ruler_grab"));
    }

    // The horizontal scrollbar, along the bottom of both panes.
    let hbar = Rect::from_min_max(
        pos2(panes[0].left() + gutter, body.bottom() - SCROLLBAR_SIZE),
        pos2(panes[1].right(), body.bottom()),
    );
    let horizontal = Scrollbar::new(
        hbar.x_range(),
        text_width,
        text_width + max_scroll_x,
        max_scroll_x,
    );
    let hbar_response = (max_scroll_x > 0.0)
        .then(|| ui.interact(hbar, scroll_id.with("hbar"), Sense::click_and_drag()));
    if let Some(pointer) = hbar_response
        .as_ref()
        .and_then(|r| r.interact_pointer_pos())
    {
        scroll.x = horizontal.drag(ui, scroll_id.with("hbar_grab"), pointer.x, scroll.x);
    } else {
        Scrollbar::release(ui, scroll_id.with("hbar_grab"));
    }

    scroll.y = scroll.y.clamp(0.0, max_scroll_y);
    scroll.x = scroll.x.clamp(0.0, max_scroll_x);
    ui.data_mut(|data| data.insert_temp(scroll_id, scroll));

    let anchor_fraction = if max_scroll_y > 0.0 {
        scroll.y / max_scroll_y
    } else {
        0.0
    };
    let anchor_offset = anchor_fraction * (body.height() - row_height).max(0.0);
    let (left_anchor, right_anchor) = view.line_at((scroll.y + anchor_offset) / row_height);
    let placement = Placement {
        anchor_y: body.top() + anchor_offset,
        anchor_line: [left_anchor, right_anchor],
        row_height,
    };

    // Clicking a fold chevron (or double-clicking a foldable line) folds or
    // unfolds it; clicking a changed block makes it current.
    if let Some(pointer) = response.hover_pos() {
        for side in Side::BOTH {
            let pane = panes[side.index()];
            let line = placement.line_at(side, pointer.y);
            if !pane.contains(pointer) || line < 0.0 {
                continue;
            }
            let line = line as usize;
            let left = side == Side::Left;
            let chevron = pane.left() + gutter - SIGN_WIDTH - FOLD_WIDTH;
            let on_chevron = (chevron..=chevron + FOLD_WIDTH).contains(&pointer.x);
            if let Some(mark) = view.folds(left).get(line).and_then(Option::as_ref) {
                if on_chevron {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if (response.clicked() && on_chevron) || response.double_clicked() {
                    commands.push(Command::SetExpanded {
                        side,
                        pointer: mark.pointer.clone(),
                        expanded: mark.collapsed,
                    });
                    continue;
                }
            }
            if response.clicked()
                && let Some(index) = view.hunk_at_line(left, line)
            {
                state.current_hunk = Some(index);
            }
        }
    }

    // Painting.
    let visuals = ui.visuals();
    painter.rect_filled(body, CornerRadius::ZERO, visuals.extreme_bg_color);
    painter.rect_filled(connector, CornerRadius::ZERO, visuals.panel_fill);

    let current = state.current_hunk;
    for side in Side::BOTH {
        let pane = panes[side.index()];
        let pane_painter = painter.with_clip_rect(pane);
        paint_boxes(
            &pane_painter,
            view,
            side,
            pane,
            gutter,
            &placement,
            current,
            palette,
        );
        paint_lines(
            ui,
            &pane_painter,
            view,
            side,
            pane,
            gutter,
            scroll.x,
            &placement,
            &font,
            palette,
        );
    }
    paint_ribbons(
        &painter.with_clip_rect(connector),
        view,
        connector,
        &placement,
        current,
        palette,
    );
    paint_ruler(
        &painter,
        view,
        ruler,
        total_height,
        body.height(),
        palette,
        visuals,
    );
    if max_scroll_y > 0.0 {
        let thumb = Rect::from_x_y_ranges(ruler.x_range(), vertical.thumb(scroll.y));
        paint_thumb(&painter, thumb, &ruler_response, visuals);
    }
    if let Some(hbar_response) = &hbar_response {
        let thumb = Rect::from_x_y_ranges(horizontal.thumb(scroll.x), hbar.y_range()).shrink(2.0);
        paint_thumb(&painter, thumb, hbar_response, visuals);
    }
    paint_header(&painter, state, header, panes, palette, visuals);
    painter.vline(
        connector.left(),
        body.y_range(),
        visuals.widgets.noninteractive.bg_stroke,
    );
    painter.vline(
        connector.right(),
        body.y_range(),
        visuals.widgets.noninteractive.bg_stroke,
    );
}

fn empty_state(ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() / 3.0);
        ui.heading("Nothing to compare yet");
        ui.add_space(8.0);
        ui.label("Open an original folder (top) and a modified folder (bottom) in the explorer,");
        ui.label("pick a file on each side, or switch to Edit to paste JSON.");
        ui.label("The assistant panel can do all of this for you.");
    });
}

fn side_range(hunk: &Hunk, side: Side) -> std::ops::Range<usize> {
    match side {
        Side::Left => hunk.left.clone(),
        Side::Right => hunk.right.clone(),
    }
}

/// Line fill, box border and intra-line highlight colours for a status:
/// green on the modified side, red on the original side.
fn status_colors(status: LineStatus, palette: &Palette) -> Option<(Color32, Color32, Color32)> {
    match status {
        LineStatus::Unchanged => None,
        LineStatus::Added => Some((palette.added_line, palette.added_border, palette.added_text)),
        LineStatus::Removed => Some((
            palette.removed_line,
            palette.removed_border,
            palette.removed_text,
        )),
    }
}

/// Fill and border colour of each end of hunk `index`'s ribbon. A side
/// without coloured lines (an insertion point, or lines that were only
/// reformatted) takes the other side's colour; a block that changed nothing
/// semantically (e.g. reordered keys) is neutral.
fn ribbon_colors(view: &SideBySide, index: usize, palette: &Palette) -> [(Color32, Color32); 2] {
    let neutral = (
        palette.line_number.gamma_multiply(0.2),
        palette.line_number.gamma_multiply(0.6),
    );
    let left = status_colors(view.hunk_status(index, true), palette);
    let right = status_colors(view.hunk_status(index, false), palette);
    let end = |own: Option<(Color32, Color32, Color32)>, other| {
        own.or(other)
            .map_or(neutral, |(fill, border, _)| (fill, border))
    };
    [end(left, right), end(right, left)]
}

/// Boxes around each run of same-coloured lines, and a marker where a block
/// that exists only on the other side belongs.
#[allow(clippy::too_many_arguments)]
fn paint_boxes(
    painter: &egui::Painter,
    view: &SideBySide,
    side: Side,
    pane: Rect,
    gutter: f32,
    placement: &Placement,
    current: Option<usize>,
    palette: &Palette,
) {
    let statuses = view.status(side == Side::Left);
    let current_range = current
        .and_then(|index| view.hunks.get(index))
        .map(|hunk| side_range(hunk, side));
    let first = placement.line_at(side, pane.top()).floor().max(0.0) as usize;
    let last =
        (placement.line_at(side, pane.bottom()).ceil().max(0.0) as usize + 1).min(statuses.len());

    // Start at the beginning of a run that began above the view, so its box
    // has no false top edge.
    let mut line = first;
    while line > 0 && line < statuses.len() && statuses[line - 1] == statuses[line] {
        line -= 1;
    }
    while line < last {
        let status = statuses[line];
        let mut end = line + 1;
        while end < statuses.len() && statuses[end] == status {
            end += 1;
        }
        if let Some((fill, border, _)) = status_colors(status, palette) {
            let (top, bottom) = (placement.y(side, line), placement.y(side, end));
            let block = Rect::from_min_max(pos2(pane.left(), top), pos2(pane.right(), bottom));
            painter.rect_filled(block, CornerRadius::ZERO, fill);
            let is_current = current_range
                .as_ref()
                .is_some_and(|range| range.start < end && line < range.end);
            let width: f32 = if is_current { 2.0 } else { 1.0 };
            let boxed = Rect::from_min_max(
                pos2(pane.left() + gutter - SIGN_WIDTH, top),
                pos2(pane.right() - 1.0, bottom),
            );
            painter.rect_stroke(
                boxed,
                CornerRadius::same(2),
                Stroke::new(width, border),
                StrokeKind::Inside,
            );
        }
        line = end;
    }

    for (index, hunk) in view.hunks.iter().enumerate() {
        let range = side_range(hunk, side);
        let y = placement.y(side, range.start);
        if !range.is_empty() || y < pane.top() || y > pane.bottom() {
            continue;
        }
        let other = view.hunk_status(index, side != Side::Left);
        if let Some((_, border, _)) = status_colors(other, palette) {
            // The ribbon in the gutter points at this spot.
            let width: f32 = if current == Some(index) { 3.0 } else { 2.0 };
            painter.hline(
                pane.left() + gutter - SIGN_WIDTH..=pane.right(),
                y,
                Stroke::new(width, border),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_lines(
    ui: &Ui,
    painter: &egui::Painter,
    view: &SideBySide,
    side: Side,
    pane: Rect,
    gutter: f32,
    scroll_x: f32,
    placement: &Placement,
    font: &FontId,
    palette: &Palette,
) {
    let lines = match side {
        Side::Left => &view.left,
        Side::Right => &view.right,
    };
    let statuses = view.status(side == Side::Left);
    let first = placement.line_at(side, pane.top()).floor().max(0.0) as usize;
    let last =
        (placement.line_at(side, pane.bottom()).ceil().max(0.0) as usize + 1).min(lines.len());
    let text_clip = Rect::from_min_max(pos2(pane.left() + gutter, pane.top()), pane.max);
    let text_painter = painter.with_clip_rect(text_clip);
    let number_font = FontId::monospace(font.size * 0.9);

    // Hunks are sorted, so walk them alongside the lines.
    let mut hunks = view.hunks.iter().peekable();
    for (line, text) in lines.iter().enumerate().take(last).skip(first) {
        while hunks
            .peek()
            .is_some_and(|hunk| side_range(hunk, side).end <= line)
        {
            hunks.next();
        }
        let hunk = hunks
            .peek()
            .filter(|hunk| side_range(hunk, side).contains(&line));
        let top = placement.y(side, line);
        let center = top + placement.row_height / 2.0;

        let status = statuses[line];
        let colors = status_colors(status, palette);
        let sign = match status {
            LineStatus::Unchanged => "",
            LineStatus::Added => "+",
            LineStatus::Removed => "−",
        };
        painter.text(
            pos2(pane.left() + gutter - SIGN_WIDTH - FOLD_WIDTH - 2.0, center),
            Align2::RIGHT_CENTER,
            (line + 1).to_string(),
            number_font.clone(),
            if colors.is_some() {
                palette.line_number_active
            } else {
                palette.line_number
            },
        );
        let fold = view.folds(side == Side::Left)[line].as_ref();
        if let Some(fold) = fold {
            let x = pane.left() + gutter - SIGN_WIDTH - FOLD_WIDTH / 2.0;
            paint_chevron(
                painter,
                pos2(x, center),
                fold.collapsed,
                palette.line_number_active,
            );
        }
        if let Some((_, border, _)) = colors {
            painter.text(
                pos2(pane.left() + gutter - SIGN_WIDTH / 2.0, center),
                Align2::CENTER_CENTER,
                sign,
                font.clone(),
                border,
            );
        }

        // Word-level highlights, only on lines whose value changed; a folded
        // line reads the same on both sides, so it gets none.
        let folded = fold.is_some_and(|fold| fold.collapsed);
        let (inline, highlight) = match (hunk, colors) {
            (Some(hunk), Some((_, _, highlight))) if !folded => {
                let inline = match side {
                    Side::Left => hunk.left_inline.get(line - hunk.left.start),
                    Side::Right => hunk.right_inline.get(line - hunk.right.start),
                };
                (inline.map(Vec::as_slice).unwrap_or_default(), highlight)
            }
            _ => (&[][..], Color32::TRANSPARENT),
        };
        let galley = ui.fonts_mut(|fonts| {
            fonts.layout_job(json_line_job(text, font, palette, inline, highlight))
        });
        let y = center - galley.size().y / 2.0;
        let text_end = text_clip.left() + 4.0 - scroll_x + galley.size().x;
        text_painter.galley(
            pos2(text_clip.left() + 4.0 - scroll_x, y),
            galley,
            palette.punctuation,
        );

        // What a folded line hides.
        if let Some(fold) = fold.filter(|fold| fold.collapsed) {
            let noun = match (text.contains("{…}"), fold.children) {
                (true, 1) => "key",
                (true, _) => "keys",
                (false, 1) => "item",
                (false, _) => "items",
            };
            text_painter.text(
                pos2(text_end + 12.0, center),
                Align2::LEFT_CENTER,
                format!("{} {noun} · {} lines", fold.children, fold.hidden_lines + 1),
                FontId::proportional(font.size * 0.85),
                palette.line_number,
            );
        }
    }
}

/// A small triangle: pointing right when folded, down when expanded.
fn paint_chevron(painter: &egui::Painter, center: Pos2, collapsed: bool, color: Color32) {
    let points = if collapsed {
        vec![
            center + vec2(-2.5, -4.5),
            center + vec2(3.5, 0.0),
            center + vec2(-2.5, 4.5),
        ]
    } else {
        vec![
            center + vec2(-4.5, -2.5),
            center + vec2(4.5, -2.5),
            center + vec2(0.0, 3.5),
        ]
    };
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Ribbons joining each block's two sides across the gutter.
fn paint_ribbons(
    painter: &egui::Painter,
    view: &SideBySide,
    connector: Rect,
    placement: &Placement,
    current: Option<usize>,
    palette: &Palette,
) {
    const STEPS: usize = 24;
    let (x0, x1) = (connector.left(), connector.right());
    let curve = |y0: f32, y1: f32, t: f32| -> Pos2 {
        // Cubic Bézier with horizontal tangents at both ends.
        let s = 1.0 - t;
        let x = s * s * s * x0
            + 3.0 * s * s * t * (x0 + x1) / 2.0
            + 3.0 * s * t * t * (x0 + x1) / 2.0
            + t * t * t * x1;
        let y = (s * s * s + 3.0 * s * s * t) * y0 + (3.0 * s * t * t + t * t * t) * y1;
        pos2(x, y)
    };

    for (index, hunk) in view.hunks.iter().enumerate() {
        let left = (
            placement.y(Side::Left, hunk.left.start),
            placement.y(Side::Left, hunk.left.end),
        );
        let right = (
            placement.y(Side::Right, hunk.right.start),
            placement.y(Side::Right, hunk.right.end),
        );
        if left.1.max(right.1) < connector.top() || left.0.min(right.0) > connector.bottom() {
            continue;
        }
        let [(left_fill, left_border), (right_fill, right_border)] =
            ribbon_colors(view, index, palette);
        let boost = |color: Color32| {
            let [r, g, b, a] = color.to_srgba_unmultiplied();
            Color32::from_rgba_unmultiplied(r, g, b, a.saturating_mul(2))
        };

        let mut mesh = Mesh::default();
        for step in 0..=STEPS {
            let t = step as f32 / STEPS as f32;
            let color = lerp_color(boost(left_fill), boost(right_fill), t);
            mesh.colored_vertex(curve(left.0, right.0, t), color);
            mesh.colored_vertex(curve(left.1, right.1, t), color);
            if step > 0 {
                let i = (step * 2) as u32;
                mesh.add_triangle(i - 2, i - 1, i);
                mesh.add_triangle(i - 1, i + 1, i);
            }
        }
        painter.add(Shape::mesh(mesh));

        let width: f32 = if current == Some(index) { 2.0 } else { 1.0 };
        for (y0, y1) in [(left.0, right.0), (left.1, right.1)] {
            let points: Vec<Pos2> = (0..=STEPS)
                .map(|step| curve(y0, y1, step as f32 / STEPS as f32))
                .collect();
            for (segment, pair) in points.windows(2).enumerate() {
                let t = segment as f32 / STEPS as f32;
                painter.line_segment(
                    [pair[0], pair[1]],
                    Stroke::new(width, lerp_color(left_border, right_border, t)),
                );
            }
        }
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
    let mix = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(0), mix(1), mix(2), mix(3))
}

/// The overview ruler: every block's position in the whole document, plus
/// the visible region.
#[allow(clippy::too_many_arguments)]
fn paint_ruler(
    painter: &egui::Painter,
    view: &SideBySide,
    ruler: Rect,
    total_height: f32,
    view_height: f32,
    palette: &Palette,
    visuals: &egui::Visuals,
) {
    painter.rect_filled(ruler, CornerRadius::ZERO, visuals.faint_bg_color);
    if total_height <= 0.0 {
        return;
    }
    let scale = ruler.height() / total_height.max(view_height);
    let row_height = total_height / view.aligned_rows().max(1) as f32;
    for segment in &view.segments {
        let Some(index) = segment.hunk else { continue };
        let top = ruler.top() + segment.aligned_start as f32 * row_height * scale;
        let height = (segment.aligned_len() as f32 * row_height * scale).max(2.0);
        let half = ruler.width() / 2.0;
        let left = Rect::from_min_size(pos2(ruler.left(), top), vec2(half, height));
        let right = Rect::from_min_size(pos2(ruler.left() + half, top), vec2(half, height));
        if view.hunk_status(index, true) == LineStatus::Unchanged
            && view.hunk_status(index, false) == LineStatus::Unchanged
        {
            continue;
        }
        let [(_, left_color), (_, right_color)] = ribbon_colors(view, index, palette);
        painter.rect_filled(left, CornerRadius::ZERO, left_color);
        painter.rect_filled(right, CornerRadius::ZERO, right_color);
    }
}

fn paint_header(
    painter: &egui::Painter,
    state: &AppState,
    header: Rect,
    panes: [Rect; 2],
    palette: &Palette,
    visuals: &egui::Visuals,
) {
    painter.rect_filled(header, CornerRadius::ZERO, visuals.panel_fill);
    let font = FontId::proportional(13.0);
    let view = &state.diff.view;
    for side in Side::BOTH {
        let doc = &state.docs[side.index()];
        let error = match side {
            Side::Left => &state.diff.left_error,
            Side::Right => &state.diff.right_error,
        };
        let x = panes[side.index()].left() + 8.0;
        let title = match error {
            Some(_) => format!("{}  ⚠ not valid JSON, compared as text", doc.label()),
            None => doc.label(),
        };
        let color = if error.is_some() {
            palette.removed_border
        } else {
            visuals.strong_text_color()
        };
        painter.text(
            pos2(x, header.center().y),
            Align2::LEFT_CENTER,
            title,
            font.clone(),
            color,
        );

        // Coloured line count, right-aligned: "−3" or "+5".
        let left = side == Side::Left;
        let (sign, status, color) = if left {
            ("−", LineStatus::Removed, palette.removed_border)
        } else {
            ("+", LineStatus::Added, palette.added_border)
        };
        painter.text(
            pos2(panes[side.index()].right() - 8.0, header.center().y),
            Align2::RIGHT_CENTER,
            format!("{sign}{}", view.count(left, status)),
            font.clone(),
            color,
        );
    }
    painter.hline(
        header.x_range(),
        header.bottom(),
        visuals.widgets.noninteractive.bg_stroke,
    );
}

/// One scrollbar: a track along one axis and a thumb as long as the visible
/// share of the content.
struct Scrollbar {
    track: Rangef,
    thumb_len: f32,
    max_scroll: f32,
}

impl Scrollbar {
    fn new(track: Rangef, visible: f32, content: f32, max_scroll: f32) -> Self {
        let length = track.span();
        let thumb_len = (length * visible / content.max(visible)).clamp(length.min(24.0), length);
        Self {
            track,
            thumb_len,
            max_scroll,
        }
    }

    fn thumb(&self, scroll: f32) -> Rangef {
        let free = self.track.span() - self.thumb_len;
        let fraction = if self.max_scroll > 0.0 {
            scroll / self.max_scroll
        } else {
            0.0
        };
        let start = self.track.min + fraction.clamp(0.0, 1.0) * free;
        Rangef::new(start, start + self.thumb_len)
    }

    /// The scroll position while the track is pressed at `pointer`.
    /// Dragging the thumb keeps the grabbed point under the pointer; pressing
    /// elsewhere on the track centres the thumb there first, as in VS Code.
    fn drag(&self, ui: &Ui, grab_id: Id, pointer: f32, scroll: f32) -> f32 {
        let thumb = self.thumb(scroll);
        let grab = ui.data_mut(|data| {
            *data.get_temp_mut_or_insert_with(grab_id, || {
                if thumb.contains(pointer) {
                    pointer - thumb.min
                } else {
                    self.thumb_len / 2.0
                }
            })
        });
        let free = self.track.span() - self.thumb_len;
        if free <= 0.0 {
            return scroll;
        }
        ((pointer - grab - self.track.min) / free).clamp(0.0, 1.0) * self.max_scroll
    }

    fn release(ui: &Ui, grab_id: Id) {
        ui.data_mut(|data| data.remove::<f32>(grab_id));
    }
}

/// A scrollbar thumb, brighter while hovered or dragged.
fn paint_thumb(
    painter: &egui::Painter,
    thumb: Rect,
    response: &egui::Response,
    visuals: &egui::Visuals,
) {
    let strength = if response.is_pointer_button_down_on() {
        0.5
    } else if response.hovered() {
        0.35
    } else {
        0.2
    };
    painter.rect_filled(
        thumb,
        CornerRadius::same(2),
        visuals.text_color().gamma_multiply(strength),
    );
}
