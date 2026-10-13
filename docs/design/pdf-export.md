# Design: export the differences as a PDF

Status: **implemented** in `report.rs`, `layout.rs` and `pdf.rs` (crates/json-diff/src). The decisions are at the end.

## Goal

Let someone hand a comparison to people who don't have json-diff, such as a
reviewer or a ticket. **File → Export as PDF…** writes one PDF with:

1. **A summary**: every change, by JSON pointer, with its old and new value.
2. **The side-by-side diff**: red and green, like the app, but trimmed to the
   changes and a little context around each one.

The PDF must stand on its own: readable in print and black-and-white, with no
dependence on the app's theme, zoom or what's folded on screen.

**Out of scope for the first version:** exporting a whole folder comparison
(one report covering every changed file pair), other formats (HTML,
Markdown), and choosing which changes to include.

## What the PDF contains

### Page 1 onwards: summary (portrait)

```
json-diff report                                   13 Oct 2026, 14:05
──────────────────────────────────────────────────────────────────────
Original   configs/v1/app.json   (/home/…/v1/configs/app.json)
Modified   configs/v2/app.json   (/home/…/v2/configs/app.json)
Options    Format JSON · Sort keys off · json-diff 0.4.0

16 changes: 4 added · 3 removed · 9 modified       11 changed blocks

 #  Change     JSON pointer               Original          New
 1  modified   /version                   "2.3.1"           "2.4.0"
 2  modified   /runtime/timeout           30                "30s"
 3  added      /endpoints/2               —                 {"path": "/checko…
 4  removed    /legacy_soap               {"enabled": true… —
 …
```

- **Changes** come from the structural diff, the same list the assistant's
  `get_structural_changes` returns, in document order.
- **Badges** are coloured and labelled in words (added, removed, modified), so
  the table still reads in black and white.
- **Long values** are cut to about 120 characters with "…"; the side-by-side
  section has them in full.
- **Invalid JSON:** if either side isn't valid JSON there is no structural
  list. The summary says so, shows the parse error, and the side-by-side
  section (a plain text diff) follows as usual.

### Then: side-by-side (landscape)

One section per changed block, headed by the block number and the pointer of
its first change. Inside a section:

- Two columns, original on the left and modified on the right, each with line
  numbers and the `−` / `+` sign column.
- Red and green line backgrounds, with darker highlights on the changed
  characters, as in the app.
- **Context:** 3 unchanged lines around each block (configurable). Longer
  unchanged runs collapse into a single "⋯ 120 unchanged lines" row.
- **Alignment:** where one side has fewer lines, its column is left blank
  opposite the other side's extra lines, so rows line up. On paper there are
  no ribbons to point at where an insertion belongs, so blank rows do that job.
- **Long lines** wrap inside their column, with a ↪ marker on each
  continuation.
- Every page has both file names in its header and "page N of M" in the footer.
  The PDF outline (bookmarks) lists the summary and each changed block.

**Folding is ignored.** The export always works from the fully expanded
documents, so nothing is hidden behind `{…}`. The context trimming above keeps
even a 10k-line document short. A **Full documents** option (no trimming)
prints everything, for small files.

## How the PDF is made

**Decision: [krilla](https://crates.io/crates/krilla), plus a small layout of
our own.** This changed after the spike (below). The first draft proposed
embedding Typst.

krilla is a PDF writer (MIT OR Apache-2.0, and the one Typst itself uses). It
draws text in an embedded font, fills shapes and writes pages. It does no
layout, but this report barely needs any:

- **Monospaced text:** everything is set in one monospaced font (DejaVu Sans
  Mono, regular and bold), so every character has the same width.
  - **Wrapping** means splitting a line every N characters.
  - **Columns** are fixed numbers of characters.
- **Fixed-height rows:** a page holds a known number of rows, so pagination
  is counting rows. A wrapped line is just several rows, and a table row never
  splits across pages.
- **Colour:** red and green backgrounds and the changed-character highlights
  are filled rectangles behind the text, at character positions we already
  know from `view.rs`.

The work splits into three pure, testable steps:

1. **`report.rs`** builds the report from the app state:
   - file names, options and the structural changes;
   - the side-by-side rows, trimmed to context, with their colours and
     changed-character ranges.
2. **`layout.rs`** turns the report into pages of positioned rows: wrapping,
   page breaks, headers and footers. It needs no PDF; tests check the page
   count, the row placement and where lines wrap.
3. **`pdf.rs`** draws the laid-out pages with krilla. It's thin; a test checks
   that the sample becomes a valid multi-page PDF whose text contains the
   changed pointers.

File contents are drawn as text, never interpreted, so a value containing any
character can't break the document.

### The spike: Typst versus krilla

| | Typst (`typst` + `typst-pdf`) | krilla (text only) |
| --- | --- | --- |
| Crates added to json-diff | **138** (15% more than today's 905) | **22** |
| What they bring | Layout engine, plus bibliographies, a WebAssembly plugin runtime, Unicode collation data, image and SVG decoders, syntax highlighting | PDF writing, font parsing and subsetting, text shaping |
| Layout | Done for us, via a template | Ours, but simple: monospaced text in fixed rows |

The 138 crates come from Typst's standard library, which can't be trimmed with
feature flags. The decisions below ask to keep dependencies manageable, so
krilla it is.

**What the spike proved with krilla:**
- **The PDF:** a 3-page US Letter landscape test file of side-by-side rows,
  with red and green backgrounds, `−`/`+` signs and Unicode text. It comes out
  at 16.5 KB, because only the characters used are embedded.
- **Validity:** `qpdf --check` passes. The text can be selected and searched
  (`pdftotext` extracts it in layout), and the page renders as intended.
- **Cost:** a 92 s clean release build of the spike, and a 4.8 MB binary for
  the whole spike including Rust's standard library and the font.

### Alternatives considered

| Approach | Why not |
| --- | --- |
| Typst | 138 extra crates, see above |
| `printpdf` | Lower level than krilla, with weaker font handling; no gain for this report |
| The web view's print dialog (`window.print()`) | The user must pick "Save as PDF" in a system dialog, which differs between WebKitGTK and WebView2; the assistant can't use it; layout depends on the platform's print engine |

### Fonts

The PDF embeds DejaVu Sans Mono, regular and bold (about 0.66 MB inside the
app; each PDF carries only the characters it uses). Its license is the
Bitstream Vera license plus public-domain changes, both permissive. The font
files go in `crates/json-diff/fonts/` with their license, and
`scripts/notices.sh` gains a section that appends them to
`THIRD_PARTY_NOTICES.txt`, since `cargo-about` only sees crates.

## How it's used

1. **File → Export as PDF…** opens a small dialog:
   - Context: 3 lines (default), 10 lines, or full documents.
   - Paper: US Letter (default) or A4.
   - **Export…**
2. A save dialog follows, suggesting a name like
   `app.json-vs-app.json.pdf` in the modified file's folder.
3. The PDF is generated in the background. The status bar shows "Exporting…",
   then "Exported to …/report.pdf", with an **Open** button that opens it in the
   system's PDF viewer.

The options are remembered with the other settings.

### The assistant

Every action is also an assistant tool, so there is an
`export_pdf { context, paper }` tool, with two limits:

- **Only on request.** The assistant uses it only when you ask for a PDF
  ("export this as a PDF with full documents"). Its tool description and
  instructions say so.
- **You pick the file.** The tool has no path argument: it opens the same save
  dialog, so you choose whether and where the file is written. The assistant
  still never writes to disk on its own, as the README promises.

## Implementation plan

1. **Spike: done.** It compared Typst with krilla (above).
2. **Report data (`report.rs`):** build the report from `AppState`:
   - re-run the diff fully expanded, without touching what's on screen;
   - trim the side-by-side rows to 3 lines of context;
   - carry the changed-character ranges.
   Unit tests on the sample: counts, pointers, context trimming, the "unchanged
   lines" rows.
3. **Layout (`layout.rs`):**
   - a portrait summary table and the landscape side-by-side pages;
   - wrapping, page breaks, headers and footers.
   Unit tests on page count, wrapping and rows staying whole.
4. **Drawing (`pdf.rs`)** with krilla and the embedded font, plus bookmarks.
   A test checks that the sample makes a valid multi-page PDF whose text
   contains the changed pointers.
5. **Command and tool:**
   - `Command::ExportPdf { path, context, paper }` for the UI;
   - the `export_pdf` assistant tool. It has no path argument, opens the save
     dialog through the Tauri layer, and its description says to use it only
     when the user asks for a PDF.
6. **UI:** File → Export as PDF…, the options dialog, progress, and an "Open"
   link when done.
7. **Packaging:** the font and its license, `THIRD_PARTY_NOTICES.txt`, the
   README and the assistant's instructions.

## Decisions

Agreed on 13 Oct 2026:

1. **Paper:** US Letter by default; A4 stays an option.
2. **Context:** 3 lines by default.
3. **The assistant** exports only when the user explicitly asks for a PDF. Its
   tool description and instructions say so, and the save dialog still decides
   whether and where the file is written.
4. **Scope:** two files only. Folder comparisons are not exported.
5. **Size:** the app may grow, but the dependencies must stay manageable. The
   spike therefore compares the number of crates each approach adds, not just
   the binary size.
