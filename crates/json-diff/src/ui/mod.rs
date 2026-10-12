mod agent_panel;
mod diff_view;
mod editor;
mod explorer;
mod file_picker;

pub use agent_panel::{AgentPanelState, ChatEntry, agent_panel};
pub use diff_view::diff_view;
pub use editor::CodeEditor;
pub use explorer::explorer;
pub use file_picker::{FilePicker, PickKind};

use eframe::egui::{Color32, FontId, text::LayoutJob, text::TextFormat};

/// Diff colours for the dark and light themes (VS Code shades): green on the
/// modified side, red on the original side.
#[derive(Clone, Copy)]
pub struct Palette {
    pub added_line: Color32,
    pub added_text: Color32,
    pub added_border: Color32,
    pub removed_line: Color32,
    pub removed_text: Color32,
    pub removed_border: Color32,
    /// Explorer badge for a file that differs between the two folders.
    pub modified_file: Color32,
    pub line_number: Color32,
    pub line_number_active: Color32,
    pub key: Color32,
    pub string: Color32,
    pub number: Color32,
    pub keyword: Color32,
    pub punctuation: Color32,
    pub status_bar: Color32,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                added_line: Color32::from_rgba_unmultiplied(155, 185, 85, 40),
                added_text: Color32::from_rgba_unmultiplied(156, 204, 44, 70),
                added_border: Color32::from_rgb(115, 201, 145),
                removed_line: Color32::from_rgba_unmultiplied(255, 0, 0, 40),
                removed_text: Color32::from_rgba_unmultiplied(255, 0, 0, 80),
                removed_border: Color32::from_rgb(241, 76, 76),
                modified_file: Color32::from_rgb(229, 192, 76),
                line_number: Color32::from_rgb(110, 118, 129),
                line_number_active: Color32::from_rgb(204, 204, 204),
                key: Color32::from_rgb(156, 220, 254),
                string: Color32::from_rgb(206, 145, 120),
                number: Color32::from_rgb(181, 206, 168),
                keyword: Color32::from_rgb(86, 156, 214),
                punctuation: Color32::from_rgb(212, 212, 212),
                status_bar: Color32::from_rgb(0, 122, 204),
            }
        } else {
            Self {
                added_line: Color32::from_rgba_unmultiplied(155, 185, 85, 45),
                added_text: Color32::from_rgba_unmultiplied(156, 204, 44, 90),
                added_border: Color32::from_rgb(88, 153, 59),
                removed_line: Color32::from_rgba_unmultiplied(255, 0, 0, 30),
                removed_text: Color32::from_rgba_unmultiplied(255, 0, 0, 60),
                removed_border: Color32::from_rgb(205, 49, 49),
                modified_file: Color32::from_rgb(176, 128, 0),
                line_number: Color32::from_rgb(130, 130, 130),
                line_number_active: Color32::from_rgb(11, 33, 108),
                key: Color32::from_rgb(4, 81, 165),
                string: Color32::from_rgb(163, 21, 21),
                number: Color32::from_rgb(9, 134, 88),
                keyword: Color32::from_rgb(0, 0, 255),
                punctuation: Color32::from_rgb(40, 40, 40),
                status_bar: Color32::from_rgb(0, 122, 204),
            }
        }
    }
}

/// Lays out one line of JSON with syntax colours. `highlights` are byte
/// ranges drawn with `highlight` as background (intra-line changes).
pub fn json_line_job(
    line: &str,
    font: &FontId,
    palette: &Palette,
    highlights: &[std::ops::Range<usize>],
    highlight: Color32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut push = |range: std::ops::Range<usize>, color: Color32| {
        // Split the token wherever a highlight starts or ends.
        let mut cuts = vec![range.start, range.end];
        for h in highlights {
            for cut in [h.start, h.end] {
                if range.start < cut && cut < range.end {
                    cuts.push(cut);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            let highlighted = highlights.iter().any(|h| h.start <= start && end <= h.end);
            job.append(
                &line[start..end],
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color,
                    background: if highlighted {
                        highlight
                    } else {
                        Color32::TRANSPARENT
                    },
                    ..Default::default()
                },
            );
        }
    };

    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let color = match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                // A string followed by a colon is an object key.
                let rest = line[i..].trim_start();
                if rest.starts_with(':') {
                    palette.key
                } else {
                    palette.string
                }
            }
            b'-' | b'0'..=b'9' => {
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                palette.number
            }
            b'a'..=b'z' | b'A'..=b'Z' => {
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                palette.keyword
            }
            _ => {
                // Punctuation, whitespace, or any non-ASCII character.
                let width = line[i..].chars().next().map_or(1, char::len_utf8);
                i += width;
                while i < bytes.len()
                    && !matches!(bytes[i], b'"' | b'-' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
                {
                    i += line[i..].chars().next().map_or(1, char::len_utf8);
                }
                palette.punctuation
            }
        };
        let end = i.min(bytes.len());
        // Never split a UTF-8 sequence (possible after an escaped quote).
        let end = if line.is_char_boundary(end) {
            end
        } else {
            line.ceil_char_boundary(end)
        };
        push(start..end, color);
        i = end;
    }
    job
}
