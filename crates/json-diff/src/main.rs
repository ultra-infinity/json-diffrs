// A GUI app: no console window on Windows (debug builds keep it for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! json-diff: the Tauri backend. The React + Monaco frontend in `ui/` draws
//! what [`view::snapshot`] describes and sends every button press back as a
//! [`run`] call naming one of the assistant's tools, so the UI and the
//! assistant share one set of commands.

mod agent;
mod agents_md;
mod samples;
mod state;
mod tools;
mod update;
mod view;

use agent::{AgentEvent, AgentHandle, AgentRequest, Host, ModelChoice};
use serde::Serialize;
use serde_json::Value;
use state::{AppState, Command, Side, WindowAction};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter, Manager, State};
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
}

impl Core {
    fn new(state: AppState) -> Self {
        Self {
            state: Mutex::new(state),
            applied_zoom: AtomicU32::new(1.0f32.to_bits()),
            applied_title: Mutex::new(String::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, AppState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `change` on the state, carries out what it asked of the window,
    /// and snapshots the result for the frontend.
    fn update<R>(&self, app: &AppHandle, change: impl FnOnce(&mut AppState) -> R) -> (R, View) {
        let mut state = self.lock();
        let result = change(&mut state);
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

#[tauri::command]
fn agent_connect(agent: State<AgentHandle>, model: String, api_key: Option<String>) {
    agent.send(AgentRequest::Connect {
        model,
        api_key: api_key.filter(|key| !key.trim().is_empty()),
    });
}

#[tauri::command]
fn agent_prompt(agent: State<AgentHandle>, core: State<Arc<Core>>, text: String) {
    let instructions = core.lock().agents_md.prompt_section();
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
fn initial_state() -> AppState {
    let mut state = AppState::default();
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
        for (side, text) in [
            (Side::Left, samples::ORIGINAL),
            (Side::Right, samples::MODIFIED),
        ] {
            let _ = state.apply(Command::SetText {
                side,
                text: text.into(),
            });
        }
        state.set_status(
            "Showing a sample. Open folders in the explorer to compare your own files.",
            false,
        );
    }
    state
}

fn main() {
    let core = Arc::new(Core::new(initial_state()));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::PendingUpdate::default())
        .setup(move |app| {
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
            open_paths,
            providers,
            models,
            api_key_env,
            startup_model,
            agent_connect,
            agent_prompt,
            agent_reset,
            agent_cancel,
            update::check_update,
            update::install_update,
            update::open_release,
            update::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start json-diff");
}
