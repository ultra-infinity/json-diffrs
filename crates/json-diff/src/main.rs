// A GUI app: no console window on Windows (debug builds keep it for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! json-diff: the Tauri backend. The React + Monaco frontend in `ui/` draws
//! what [`view::snapshot`] describes and sends every button press back as a
//! [`run`] call naming one of the assistant's tools, so the UI and the
//! assistant share one set of commands.

mod agent;
mod licenses;
mod secrets;
mod update;

// The app itself, shared with the web version; imported under the same
// names so `crate::state` and the like read as before.
use json_diff_app::{changelog, layout, pdf, samples, settings, state, tools, view};

use agent::{AgentEvent, AgentHandle, AgentRequest, Host, ModelChoice};
use layout::Paper;
use serde::Serialize;
use serde_json::Value;
use settings::Settings;
use state::{AppState, Command, Side, WindowAction};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tools::{ToolSpec, tool_specs};
use view::View;

/// Built once: generating every tool's JSON schema is not free.
static TOOLS: LazyLock<Vec<ToolSpec>> = LazyLock::new(tool_specs);

/// The app state, shared by the frontend's commands and the assistant.
struct Core {
    state: Mutex<AppState>,
    /// The zoom last applied to the webview, as `f32` bits.
    applied_zoom: AtomicU32,
    applied_title: Mutex<String>,
    /// Where settings are remembered; `None` if the OS has no config
    /// directory, in which case nothing is remembered.
    settings_path: Option<PathBuf>,
    /// The settings as last written, so unchanged ones are not rewritten.
    saved: Mutex<Settings>,
    /// Releases since the version that last ran, until the frontend shows
    /// them.
    news: Mutex<Vec<changelog::Release>>,
}

impl Core {
    fn new(state: AppState, settings_path: Option<PathBuf>, saved: Settings) -> Self {
        Self {
            state: Mutex::new(state),
            applied_zoom: AtomicU32::new(1.0f32.to_bits()),
            applied_title: Mutex::new(String::new()),
            settings_path,
            saved: Mutex::new(saved),
            news: Mutex::new(Vec::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, AppState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes the settings if `state` (or the frontend's part, when given)
    /// changed them.
    fn remember(&self, state: &AppState, ui: Option<Value>) {
        let mut saved = self.saved.lock().unwrap_or_else(PoisonError::into_inner);
        let ui = ui.unwrap_or_else(|| saved.ui.clone());
        let mut settings = Settings::of(state, ui);
        settings.last_version.clone_from(&saved.last_version);
        self.save(&mut saved, settings);
    }

    /// Notes the releases since the version that last ran (for the
    /// frontend's What's New) and records `version` as the last to run.
    fn start_version(&self, version: &str, upgraded: bool) {
        let mut saved = self.saved.lock().unwrap_or_else(PoisonError::into_inner);
        *self.news.lock().unwrap_or_else(PoisonError::into_inner) =
            changelog::news(saved.last_version.as_deref(), upgraded, version);
        let settings = Settings {
            last_version: Some(version.to_string()),
            ..saved.clone()
        };
        self.save(&mut saved, settings);
    }

    fn save(&self, saved: &mut Settings, settings: Settings) {
        if settings == *saved {
            return;
        }
        if let Some(path) = &self.settings_path
            && let Err(error) = settings.save(path)
        {
            eprintln!("could not save settings to {}: {error}", path.display());
        }
        *saved = settings;
    }

    /// Runs `change` on the state, carries out what it asked of the window,
    /// remembers changed settings, and snapshots the result for the
    /// frontend.
    fn update<R>(&self, app: &AppHandle, change: impl FnOnce(&mut AppState) -> R) -> (R, View) {
        let mut state = self.lock();
        let result = change(&mut state);
        self.remember(&state, None);
        let window = app.get_webview_window("main");
        for action in std::mem::take(&mut state.window_requests) {
            let Some(window) = &window else { break };
            let _ = match action {
                WindowAction::Minimize => window.minimize(),
                WindowAction::Maximize => window.maximize(),
                WindowAction::Restore => window.unmaximize(),
                WindowAction::Close => window.close(),
            };
        }
        let zoom = state.zoom.to_bits();
        if self.applied_zoom.swap(zoom, Ordering::Relaxed) != zoom
            && let Some(window) = &window
        {
            let _ = window.set_zoom(f64::from(state.zoom));
        }
        let view = view::snapshot(&mut state);
        let mut title = self
            .applied_title
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *title != view.title
            && let Some(window) = &window
        {
            let _ = window.set_title(&view.title);
            title.clone_from(&view.title);
        }
        (result, view)
    }
}

/// The assistant's way into the app: its tool calls change the shared state
/// and the frontend hears about it through events.
struct TauriHost {
    core: Arc<Core>,
    app: AppHandle,
}

impl Host for TauriHost {
    fn apply(&self, command: Command) -> Result<Value, String> {
        // The assistant never chooses where a PDF goes: the user does, in the
        // save dialog, and cancelling writes nothing.
        let command = match command {
            Command::ExportPdf {
                path: None,
                context,
                paper,
            } => {
                let suggested = pdf::suggested_path(&self.core.lock());
                let path = save_dialog(&self.app, &suggested)
                    .ok_or("the user cancelled the export; nothing was written")?;
                Command::ExportPdf {
                    path: Some(path),
                    context,
                    paper,
                }
            }
            command => command,
        };
        let (result, view) = self.core.update(&self.app, |state| state.apply(command));
        let _ = self.app.emit("view", view);
        result
    }

    fn emit(&self, event: AgentEvent) {
        let _ = self.app.emit("agent", event);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunOutput {
    result: Option<Value>,
    /// Also shown in the status bar.
    error: Option<String>,
    view: View,
}

#[tauri::command]
fn get_view(app: AppHandle, core: State<Arc<Core>>) -> View {
    core.update(&app, |_| ()).1
}

/// Runs the command behind a button: `name` and `args` are one of the
/// assistant's tools and its arguments.
#[tauri::command]
fn run(
    app: AppHandle,
    core: State<Arc<Core>>,
    name: String,
    args: Value,
) -> Result<RunOutput, String> {
    let spec = TOOLS
        .iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| format!("unknown command {name}"))?;
    let command = (spec.parse)(args).map_err(|error| format!("{name}: {error}"))?;
    let (result, view) = core.update(&app, |state| state.apply(command));
    Ok(RunOutput {
        error: result.as_ref().err().cloned(),
        result: result.ok(),
        view,
    })
}

/// File > Export as PDF, once the user picked `path` in the save dialog.
#[tauri::command]
fn export_pdf(
    app: AppHandle,
    core: State<Arc<Core>>,
    path: PathBuf,
    context: Option<usize>,
    paper: Paper,
) -> RunOutput {
    let (result, view) = core.update(&app, |state| {
        state.apply(Command::ExportPdf {
            path: Some(path),
            context,
            paper,
        })
    });
    RunOutput {
        error: result.as_ref().err().cloned(),
        result: result.ok(),
        view,
    }
}

/// The key under 1-based `line` of `side`, for the diff's right-click menu.
#[tauri::command]
fn key_at(core: State<Arc<Core>>, side: Side, line: usize) -> Option<view::KeyAt> {
    view::key_at(&core.lock(), side, line)
}

/// Where the export's save dialog starts.
#[tauri::command]
fn pdf_suggested_path(core: State<Arc<Core>>) -> String {
    pdf::suggested_path(&core.lock()).display().to_string()
}

/// Opens an exported PDF in the system's viewer.
#[tauri::command]
fn open_pdf(app: AppHandle, path: PathBuf) -> Result<(), String> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err(format!("{} is not a PDF", path.display()));
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|error| error.to_string())
}

/// Asks where to save a PDF, starting at `suggested`. Blocks, so it must not
/// run on the main thread (the assistant's tools don't).
fn save_dialog(app: &AppHandle, suggested: &std::path::Path) -> Option<PathBuf> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Save the PDF report")
        .add_filter("PDF", &["pdf"]);
    if let Some(name) = suggested.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    if let Some(folder) = suggested
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        dialog = dialog.set_directory(folder);
    }
    dialog.blocking_save_file()?.into_path().ok()
}

/// Text typed in edit mode.
#[tauri::command]
fn edit_text(app: AppHandle, core: State<Arc<Core>>, side: Side, text: String) -> View {
    core.update(&app, |state| state.edit_text(side, text)).1
}

/// Files or folders dropped on the window. Two paths fill both sides; one
/// goes to `side`.
#[tauri::command]
fn open_paths(app: AppHandle, core: State<Arc<Core>>, paths: Vec<PathBuf>, side: Side) -> View {
    core.update(&app, |state| {
        let targets: Vec<(Side, PathBuf)> = match paths.as_slice() {
            [left, right] => vec![(Side::Left, left.clone()), (Side::Right, right.clone())],
            [path, ..] => vec![(side, path.clone())],
            [] => Vec::new(),
        };
        for (side, path) in targets {
            let _ = state.apply(open_command(side, path));
        }
    })
    .1
}

#[tauri::command]
fn providers() -> Vec<String> {
    agent::known_providers()
}

#[tauri::command]
fn models(provider: String) -> Vec<ModelChoice> {
    agent::catalog_models(&provider)
}

#[tauri::command]
fn api_key_env(provider: String) -> Option<&'static str> {
    agent::api_key_env(&provider)
}

/// The `provider:model` to connect at startup, from `JSON_DIFF_MODEL`.
#[tauri::command]
fn startup_model() -> Option<String> {
    std::env::var(agent::MODEL_ENV)
        .ok()
        .filter(|model| !model.trim().is_empty())
}

/// Connects the assistant to `model` (`provider:model`). A key typed in is
/// used, and saved in the keychain when `remember` is set; without one, the
/// key saved for that provider is used, then its environment variable.
/// Fails only when a key could not be remembered; the connection itself
/// reports through the `agent` event.
#[tauri::command]
async fn agent_connect(
    agent: State<'_, AgentHandle>,
    model: String,
    api_key: Option<String>,
    remember: bool,
) -> Result<(), String> {
    let provider = secrets::provider_of(&model).to_owned();
    let typed = api_key.filter(|key| !key.trim().is_empty());
    let (api_key, saved) = blocking(move || match typed {
        Some(key) if remember => {
            let saved = secrets::save(&provider, &key);
            (Some(key), saved)
        }
        Some(key) => (Some(key), Ok(())),
        None => (secrets::load(&provider), Ok(())),
    })
    .await?;
    agent.send(AgentRequest::Connect { model, api_key });
    saved.map_err(|error| format!("the key was used but not remembered: {error}"))
}

/// Whether a key for `provider` is saved in the keychain.
#[tauri::command]
async fn saved_key(provider: String) -> Result<bool, String> {
    blocking(move || secrets::load(&provider).is_some()).await
}

#[tauri::command]
async fn forget_key(provider: String) -> Result<(), String> {
    blocking(move || secrets::forget(&provider)).await?
}

/// The frontend's remembered settings (panel widths, provider and model).
#[tauri::command]
fn ui_settings(core: State<Arc<Core>>) -> Value {
    core.saved
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .ui
        .clone()
}

#[tauri::command]
fn set_ui_settings(core: State<Arc<Core>>, settings: Value) {
    let state = core.lock();
    core.remember(&state, Some(settings));
}

/// What changed since the version that last ran; empty after the first
/// call, so it is shown once.
#[tauri::command]
fn whats_new(core: State<Arc<Core>>) -> Vec<changelog::Release> {
    std::mem::take(&mut *core.news.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Every release up to this one, newest first (Help > What's New).
#[tauri::command]
fn release_notes(app: AppHandle) -> Vec<changelog::Release> {
    changelog::releases(&app.package_info().version.to_string())
}

/// Runs keychain work, which blocks, off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn agent_prompt(agent: State<AgentHandle>, core: State<Arc<Core>>, text: String) {
    let instructions = {
        let state = core.lock();
        state.agents_md.prompt_section() + &state.ignored_prompt_section()
    };
    agent.send(AgentRequest::Prompt { text, instructions });
}

#[tauri::command]
fn agent_reset(agent: State<AgentHandle>) {
    agent.send(AgentRequest::Reset);
}

#[tauri::command]
fn agent_cancel(agent: State<AgentHandle>) {
    agent.cancel();
}

/// Opens `path` on `side`: a folder in the explorer, a file in the diff.
fn open_command(side: Side, path: PathBuf) -> Command {
    if path.is_dir() {
        Command::OpenFolder { side, path }
    } else {
        Command::SelectFile {
            side,
            path: std::path::absolute(&path).unwrap_or(path),
        }
    }
}

/// `json-diff <original> <modified>` opens two files or two folders;
/// without arguments a sample comparison is shown.
fn initial_state(settings: &Settings) -> AppState {
    let mut state = AppState::default();
    settings.apply_to(&mut state);
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if let [left, right] = args.as_slice() {
        for (side, path) in Side::BOTH.into_iter().zip([left, right]) {
            let _ = state.apply(open_command(side, path.clone()));
        }
        // Two folders: start on the first file that differs.
        if let Some(pair) = state
            .pairs
            .iter()
            .find(|pair| pair.status != json_diff_core::PairStatus::Identical)
        {
            let _ = state.apply(Command::OpenPair {
                path: pair.path.clone(),
            });
        }
    } else {
        samples::show(
            &mut state,
            "Showing a sample. Open folders in the explorer to compare your own files.",
        );
    }
    state
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::PendingUpdate::default())
        .setup(|app| {
            let settings_path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|directory| directory.join("settings.json"));
            let loaded = settings_path.as_deref().and_then(Settings::load);
            let upgraded = loaded.is_some();
            let settings = loaded.unwrap_or_default();
            let core = Arc::new(Core::new(initial_state(&settings), settings_path, settings));
            core.start_version(&app.package_info().version.to_string(), upgraded);
            // Taskbars and window switchers show the window's own icon (X11 and
            // Windows read it from the window); the launcher entry and the exe
            // carry it for app menus and search.
            if let (Some(window), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                let _ = window.set_icon(icon.clone());
            }
            let host = TauriHost {
                core: core.clone(),
                app: app.handle().clone(),
            };
            app.manage(agent::spawn(Arc::new(host)));
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_view,
            run,
            edit_text,
            export_pdf,
            pdf_suggested_path,
            key_at,
            open_pdf,
            open_paths,
            providers,
            models,
            api_key_env,
            startup_model,
            agent_connect,
            agent_prompt,
            agent_reset,
            agent_cancel,
            saved_key,
            forget_key,
            ui_settings,
            set_ui_settings,
            whats_new,
            release_notes,
            update::check_update,
            update::install_update,
            update::open_release,
            update::app_version,
            licenses::licenses,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start json-diff");
}
