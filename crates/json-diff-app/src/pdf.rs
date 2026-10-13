//! Exporting a comparison as a PDF: [`crate::report`] decides what it says,
//! [`crate::layout`] where it goes, and this module draws it with krilla.

use crate::layout::{self, Item, Layout, Paper};
use crate::report;
use crate::state::AppState;
use krilla::Document;
use krilla::color::rgb;
use krilla::destination::XyzDestination;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::text::{Font, TextDirection};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// DejaVu Sans Mono; see `fonts/README.md` for where it comes from.
const REGULAR: &[u8] = include_bytes!("../fonts/DejaVuSansMono.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/DejaVuSansMono-Bold.ttf");

/// Unchanged lines kept around each change unless the user asks otherwise.
pub const DEFAULT_CONTEXT: usize = 3;

/// A finished export.
pub struct Pdf {
    pub bytes: Vec<u8>,
    pub pages: usize,
}

/// The report for what `state` compares, as a PDF. `context` is how many
/// unchanged lines to keep around each change; `None` keeps everything.
pub fn export(
    state: &AppState,
    context: Option<usize>,
    paper: Paper,
    generated: SystemTime,
) -> Result<Pdf, String> {
    let report = report::build(state, context, generated);
    let layout = layout::lay_out(&report, paper);
    let pages = layout.pages.len();
    let bytes = draw(&layout, &report.title, &report.producer)?;
    Ok(Pdf { bytes, pages })
}

fn draw(layout: &Layout, title: &str, producer: &str) -> Result<Vec<u8>, String> {
    let font = |data: &[u8]| {
        Font::new(data.to_vec().into(), 0)
            .ok_or_else(|| "the embedded font is unreadable".to_owned())
    };
    let (regular, bold) = (font(REGULAR)?, font(BOLD)?);

    let mut document = Document::new();
    for page in &layout.pages {
        let settings = PageSettings::from_wh(page.width, page.height)
            .ok_or_else(|| format!("invalid page size {}×{}", page.width, page.height))?;
        let mut pdf_page = document.start_page_with(settings);
        let mut surface = pdf_page.surface();
        for item in &page.items {
            match item {
                Item::Rect {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    let Some(rect) = Rect::from_xywh(*x, *y, *width, *height) else {
                        continue;
                    };
                    let mut path = PathBuilder::new();
                    path.push_rect(rect);
                    let Some(path) = path.finish() else { continue };
                    surface.set_fill(Some(fill(*color)));
                    surface.draw_path(&path);
                }
                Item::Text {
                    x,
                    y,
                    size,
                    bold: is_bold,
                    color,
                    text,
                } => {
                    surface.set_fill(Some(fill(*color)));
                    let font = if *is_bold {
                        bold.clone()
                    } else {
                        regular.clone()
                    };
                    surface.draw_text(
                        Point::from_xy(*x, *y),
                        font,
                        *size,
                        text,
                        false,
                        TextDirection::Auto,
                    );
                }
            }
        }
        surface.finish();
        pdf_page.finish();
    }

    let mut outline = Outline::new();
    for bookmark in &layout.bookmarks {
        let destination = XyzDestination::new(bookmark.page, Point::from_xy(0.0, bookmark.y));
        outline.push_child(OutlineNode::new(bookmark.title.clone(), destination));
    }
    document.set_outline(outline);
    // No creation date: the same comparison always gives the same file.
    document.set_metadata(
        Metadata::new()
            .title(format!("json-diff report: {title}"))
            .creator(producer.to_owned())
            .producer(producer.to_owned()),
    );
    document
        .finish()
        .map_err(|error| format!("could not write the PDF: {error:?}"))
}

fn fill((red, green, blue): layout::Color) -> Fill {
    Fill {
        paint: rgb::Color::new(red, green, blue).into(),
        opacity: NormalizedF32::ONE,
        rule: Default::default(),
    }
}

/// Where the save dialog starts: `original-vs-modified.pdf`, in the modified
/// file's folder when it has one.
pub fn suggested_path(state: &AppState) -> PathBuf {
    let stem = |index: usize| {
        let doc = &state.docs[index];
        doc.path.as_deref().and_then(Path::file_stem).map_or_else(
            || if index == 0 { "original" } else { "modified" }.to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        )
    };
    let name = format!("{}-vs-{}.pdf", stem(0), stem(1));
    let folder = state.docs[1]
        .path
        .as_deref()
        .or(state.docs[0].path.as_deref())
        .and_then(Path::parent);
    folder.map_or_else(|| PathBuf::from(&name), |folder| folder.join(&name))
}

/// `path`, ending in `.pdf`.
pub fn with_pdf_extension(path: PathBuf) -> PathBuf {
    match path.extension() {
        Some(extension) if extension.eq_ignore_ascii_case("pdf") => path,
        _ => {
            let mut name = path.into_os_string();
            name.push(".pdf");
            PathBuf::from(name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn the_sample_becomes_a_complete_pdf() {
        let state = crate::report::tests::sample();
        let pdf = export(&state, Some(DEFAULT_CONTEXT), Paper::Letter, UNIX_EPOCH).unwrap();
        assert!(pdf.bytes.starts_with(b"%PDF-"));
        assert!(pdf.bytes.trim_ascii_end().ends_with(b"%%EOF"));
        assert!(pdf.pages >= 2);
        // Same input, same bytes: nothing time-dependent is written.
        let again = export(&state, Some(DEFAULT_CONTEXT), Paper::Letter, UNIX_EPOCH).unwrap();
        assert_eq!(pdf.bytes, again.bytes);
        // Whole documents make a longer report.
        let whole = export(&state, None, Paper::Letter, UNIX_EPOCH).unwrap();
        assert!(whole.pages >= pdf.pages);
    }

    /// Writes the sample's report for a look at the layout:
    /// `JSON_DIFF_SAMPLE_PDF=/tmp/sample.pdf cargo test -p json-diff -- --ignored sample_pdf`
    #[test]
    #[ignore = "writes a file; run by hand"]
    fn sample_pdf() {
        let path = std::env::var("JSON_DIFF_SAMPLE_PDF").expect("set JSON_DIFF_SAMPLE_PDF");
        let state = crate::report::tests::sample();
        let pdf = export(
            &state,
            Some(DEFAULT_CONTEXT),
            Paper::Letter,
            SystemTime::now(),
        )
        .unwrap();
        std::fs::write(path, pdf.bytes).unwrap();
    }

    #[test]
    fn file_names_come_from_the_documents() {
        let state = crate::report::tests::sample();
        assert_eq!(
            suggested_path(&state),
            PathBuf::from("original-vs-modified.pdf")
        );
        assert_eq!(
            with_pdf_extension("a/report".into()),
            PathBuf::from("a/report.pdf")
        );
        assert_eq!(
            with_pdf_extension("a/report.PDF".into()),
            PathBuf::from("a/report.PDF")
        );
    }
}
