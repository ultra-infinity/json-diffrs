//! The Open Folder / Open File dialog, drawn inside the app with egui so it
//! behaves the same on every platform and needs no native dialog service.

use crate::state::{AppState, Command, Side};
use eframe::egui;
use egui_file_dialog::{DialogState, FileDialog};

const JSON_FILTER: &str = "JSON files";
const MARKDOWN_FILTER: &str = "Markdown files";

#[derive(Clone, Copy)]
pub enum PickKind {
    Folder,
    File,
}

/// What the open dialog was opened for.
#[derive(Clone, Copy)]
enum Target {
    Side(Side, PickKind),
    /// An instructions file for the assistant.
    AgentsMd,
}

pub struct FilePicker {
    dialog: FileDialog,
    pending: Option<Target>,
}

impl Default for FilePicker {
    fn default() -> Self {
        Self {
            dialog: FileDialog::new()
                .as_modal(true)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .default_size([760.0, 500.0])
                .add_file_filter_extensions(JSON_FILTER, vec!["json"])
                .add_file_filter_extensions(MARKDOWN_FILTER, vec!["md", "markdown"]),
            pending: None,
        }
    }
}

impl FilePicker {
    /// Opens the dialog for `side`, starting in that side's folder (or the
    /// other side's) when one is open.
    pub fn open(&mut self, state: &AppState, side: Side, kind: PickKind) {
        let config = self.dialog.config_mut();
        config.title = Some(match kind {
            PickKind::Folder => format!("Open {} folder", side.title().to_lowercase()),
            PickKind::File => format!("Open {} file", side.title().to_lowercase()),
        });
        config.default_file_filter = Some(JSON_FILTER.to_owned());
        if let Some(start) = start_folder(state, side) {
            config.initial_directory = start;
        }
        match kind {
            PickKind::Folder => self.dialog.pick_directory(),
            PickKind::File => self.dialog.pick_file(),
        }
        self.pending = Some(Target::Side(side, kind));
    }

    /// Opens the dialog to choose an AGENTS.md (or any Markdown file) for
    /// the assistant to follow.
    pub fn open_agents_md(&mut self, state: &AppState) {
        let config = self.dialog.config_mut();
        config.title = Some("Add instructions for the assistant (AGENTS.md)".to_owned());
        config.default_file_filter = Some(MARKDOWN_FILTER.to_owned());
        if let Some(start) = start_folder(state, Side::Left) {
            config.initial_directory = start;
        }
        self.dialog.pick_file();
        self.pending = Some(Target::AgentsMd);
    }

    pub fn is_open(&self) -> bool {
        *self.dialog.state() == DialogState::Open
    }

    /// Draws the dialog while it is open and turns a pick into a command.
    pub fn show(&mut self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        self.dialog.update(ctx);
        if let Some(path) = self.dialog.take_picked()
            && let Some(target) = self.pending.take()
        {
            commands.push(match target {
                Target::Side(side, PickKind::Folder) => Command::OpenFolder { side, path },
                Target::Side(side, PickKind::File) => Command::SelectFile { side, path },
                Target::AgentsMd => Command::AddAgentsMd { path },
            });
        }
    }
}

/// `side`'s open folder, or else the other side's.
fn start_folder(state: &AppState, side: Side) -> Option<std::path::PathBuf> {
    state.folders[side.index()]
        .as_ref()
        .or(state.folders[1 - side.index()].as_ref())
        .map(|folder| folder.root.clone())
}
