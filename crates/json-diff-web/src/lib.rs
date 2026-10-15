//! json-diff in the browser. The same app state as the desktop app, behind
//! a small JavaScript interface: the frontend calls these methods where the
//! desktop app makes Tauri calls (see `ui/src/backend/web.ts`).
//!
//! Values cross the boundary as JSON strings. The web app has no assistant
//! or updater: files and folders arrive as text from the browser, settings
//! are kept in its storage, and PDFs are handed back as bytes to download.

use json_diff_app::changelog::{self, Release};
use json_diff_app::layout::Paper;
use json_diff_app::settings::Settings;
use json_diff_app::state::{AppState, Side};
use json_diff_app::tools::{ToolSpec, tool_specs};
use json_diff_app::view::{self, View};
use json_diff_app::{pdf, samples};
use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, UNIX_EPOCH};
use wasm_bindgen::prelude::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[wasm_bindgen]
pub struct App {
    state: AppState,
    /// The frontend's own settings (panel widths, PDF options).
    ui: Value,
    tools: Vec<ToolSpec>,
    /// Releases since the version that last ran here, until shown.
    news: Vec<Release>,
}

/// An exported PDF report.
#[wasm_bindgen]
pub struct PdfFile {
    bytes: Vec<u8>,
    pages: usize,
}

#[wasm_bindgen]
impl PdfFile {
    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pages(&self) -> usize {
        self.pages
    }
}

/// What `run` returns, as the desktop app's `run` command does.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunOutput {
    result: Option<Value>,
    error: Option<String>,
    view: View,
}

#[wasm_bindgen]
impl App {
    /// Starts on the sample comparison. `settings` is what [`App::settings`]
    /// returned last time, from the browser's storage, if anything.
    #[wasm_bindgen(constructor)]
    pub fn new(settings: Option<String>) -> App {
        let saved: Option<Settings> = settings.and_then(|text| serde_json::from_str(&text).ok());
        let news = changelog::news(
            saved
                .as_ref()
                .and_then(|saved| saved.last_version.as_deref()),
            saved.is_some(),
            VERSION,
        );
        let settings = saved.unwrap_or_default();
        let mut state = AppState::default();
        settings.apply_to(&mut state);
        // There is no assistant here.
        state.show_agent = false;
        samples::show(
            &mut state,
            "Showing a sample. Open or drop JSON files or folders to compare your own: they stay on your computer.",
        );
        App {
            state,
            ui: settings.ui,
            tools: tool_specs(),
            news,
        }
    }

    /// The snapshot to draw, as JSON.
    pub fn view(&mut self) -> String {
        to_json(&view::snapshot(&mut self.state))
    }

    /// Runs the command behind a button: `name` and `args` (JSON) are one of
    /// the assistant's tools and its arguments, as on the desktop.
    pub fn run(&mut self, name: &str, args: &str) -> Result<String, String> {
        let spec = self
            .tools
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| format!("unknown command {name}"))?;
        let args = serde_json::from_str(args).map_err(|error| format!("{name}: {error}"))?;
        let command = (spec.parse)(args).map_err(|error| format!("{name}: {error}"))?;
        let result = self.state.apply(command);
        Ok(to_json(&RunOutput {
            error: result.as_ref().err().cloned(),
            result: result.ok(),
            view: view::snapshot(&mut self.state),
        }))
    }

    /// Text typed in edit mode.
    pub fn edit_text(&mut self, side: &str, text: String) -> Result<String, String> {
        self.state.edit_text(parse_side(side)?, text);
        Ok(self.view())
    }

    /// A file picked or dropped: `name` labels the side.
    pub fn open_text(&mut self, side: &str, name: &str, text: String) -> Result<String, String> {
        self.state.open_text(parse_side(side)?, name, text);
        Ok(self.view())
    }

    /// A folder picked or dropped: `files` is JSON, `[[relative path, text],
    /// …]`, and `name` stands for the folder.
    pub fn open_folder(&mut self, side: &str, name: &str, files: &str) -> Result<String, String> {
        let files: Vec<(std::path::PathBuf, String)> =
            serde_json::from_str(files).map_err(|error| error.to_string())?;
        self.state
            .open_folder_contents(parse_side(side)?, name, files);
        Ok(self.view())
    }

    /// Shows `message` as an error in the status bar, e.g. a folder the
    /// browser could not read.
    pub fn report_error(&mut self, message: &str) -> String {
        self.state.set_status(message, true);
        self.view()
    }

    /// The key under 1-based `line` of `side`, as JSON (`null` if none).
    pub fn key_at(&self, side: &str, line: usize) -> Result<String, String> {
        Ok(to_json(&view::key_at(&self.state, parse_side(side)?, line)))
    }

    /// The PDF report. `context`: unchanged lines around each change (none:
    /// whole documents); `paper`: `letter` or `a4`; `now`: the time to print,
    /// in milliseconds since 1970 (the browser's `Date.now()`).
    pub fn export_pdf(
        &mut self,
        context: Option<usize>,
        paper: &str,
        now: f64,
    ) -> Result<PdfFile, String> {
        let paper: Paper = serde_json::from_value(Value::String(paper.into()))
            .map_err(|error| error.to_string())?;
        let generated = UNIX_EPOCH + Duration::from_millis(now.max(0.0) as u64);
        let report = pdf::export(&self.state, context, paper, generated)?;
        self.state
            .set_status(format!("Exported a {}-page PDF", report.pages), false);
        Ok(PdfFile {
            bytes: report.bytes,
            pages: report.pages,
        })
    }

    /// The name to save the PDF under, e.g. `app.json-vs-app.json.pdf`.
    pub fn pdf_name(&self) -> String {
        let path = pdf::suggested_path(&self.state);
        path.file_name().map_or_else(
            || "json-diff.pdf".into(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// Everything to remember, as JSON for the browser's storage.
    pub fn settings(&self) -> String {
        let mut settings = Settings::of(&self.state, self.ui.clone());
        settings.last_version = Some(VERSION.to_owned());
        to_json(&settings)
    }

    /// The frontend's part of the settings, as JSON.
    pub fn ui_settings(&self) -> String {
        to_json(&self.ui)
    }

    pub fn set_ui_settings(&mut self, ui: &str) -> Result<(), String> {
        self.ui = serde_json::from_str(ui).map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Releases since the version that last ran in this browser, as JSON;
    /// empty after the first call.
    pub fn whats_new(&mut self) -> String {
        to_json(&std::mem::take(&mut self.news))
    }

    /// Every release up to this one, newest first, as JSON.
    pub fn release_notes(&self) -> String {
        to_json(&changelog::releases(VERSION))
    }

    pub fn version() -> String {
        VERSION.to_owned()
    }
}

fn parse_side(side: &str) -> Result<Side, String> {
    serde_json::from_value(Value::String(side.into())).map_err(|error| error.to_string())
}

fn to_json(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_fresh_browser_sees_the_sample_and_no_news() {
        let mut app = App::new(None);
        let snapshot = view(&app.view());
        assert_eq!(snapshot["showExplorer"], true);
        assert_eq!(snapshot["showAgent"], false);
        assert!(snapshot["diff"]["hunks"].as_array().unwrap().len() > 1);
        assert_eq!(app.whats_new(), "[]");
    }

    #[test]
    fn folders_arrive_as_lists_of_files() {
        let mut app = App::new(None);
        app.open_folder("left", "v1", r#"[["a/x.json", "{\"v\": 1}"]]"#)
            .unwrap();
        let opened = view(
            &app.open_folder("right", "v2", r#"[["a/x.json", "{\"v\": 2}"]]"#)
                .unwrap(),
        );
        assert_eq!(opened["folders"][1]["name"], "v2");
        assert_eq!(opened["folders"][1]["files"][0]["status"], "modified");
        let output = view(
            &app.run("select_file", r#"{"side": "left", "path": "a/x.json"}"#)
                .unwrap(),
        );
        assert_eq!(output["error"], Value::Null);
        assert_eq!(output["view"]["docs"][0]["label"], "x.json");
    }

    #[test]
    fn files_commands_settings_and_pdfs_work_without_a_file_system() {
        let mut app = App::new(None);
        app.open_text("left", "a.json", r#"{"v": 1, "at": "mon"}"#.into())
            .unwrap();
        let opened = view(
            &app.open_text("right", "b.json", r#"{"v": 2, "at": "tue"}"#.into())
                .unwrap(),
        );
        assert_eq!(opened["docs"][0]["label"], "a.json");
        assert_eq!(opened["semanticChanges"], 2);

        let output = view(&app.run("ignore_changes", r#"{"key": "at"}"#).unwrap());
        assert_eq!(output["error"], Value::Null);
        assert_eq!(output["view"]["semanticChanges"], 1);
        assert!(app.run("no_such_command", "{}").is_err());

        let pdf = app.export_pdf(Some(3), "a4", 1.79e12).unwrap();
        assert!(pdf.bytes().starts_with(b"%PDF-"));
        assert!(pdf.pages() >= 2);
        assert_eq!(app.pdf_name(), "a-vs-b.pdf");

        // The settings come back in a new session, which hears what's new
        // only once a version has run before.
        let settings = app.settings();
        assert_eq!(view(&settings)["ignored"][0]["key"], "at");
        let mut again = App::new(Some(settings));
        assert_eq!(again.state.ignored.len(), 1);
        assert_eq!(again.whats_new(), "[]", "same version");
        let older = settings_with_version("0.1.0", &app);
        assert_ne!(App::new(Some(older)).whats_new(), "[]");
    }

    fn settings_with_version(version: &str, app: &App) -> String {
        let mut settings = view(&app.settings());
        settings["lastVersion"] = Value::String(version.into());
        settings.to_string()
    }
}
