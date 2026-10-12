//! The Open Folder / Open File dialog, drawn inside the app with egui so it
//! behaves the same on every platform and needs no native dialog service.

use crate::state::{AppState, Command, Side};
use eframe::egui;
use egui_file_dialog::{DialogState, FileDialog};

#[derive(Clone, Copy)]
pub enum PickKind {
    Folder,
    File,
}

pub struct FilePicker {
    dialog: FileDialog,
    /// What the open dialog was opened for.
    pending: Option<(Side, PickKind)>,
}

impl Default for FilePicker {
    fn default() -> Self {
        Self {
            dialog: FileDialog::new()
                .as_modal(true)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .default_size([760.0, 500.0])
                .add_file_filter_extensions("JSON files", vec!["json"])
                .default_file_filter("JSON files"),
            pending: None,
        }
    }
}

impl FilePicker {
    /// Opens the dialog for `side`, starting in that side's folder (or the
    /// other side's) when one is open.
    pub fn open(&mut self, state: &AppState, side: Side, kind: PickKind) {
        let start = state.folders[side.index()]
            .as_ref()
            .or(state.folders[1 - side.index()].as_ref())
            .map(|folder| folder.root.clone());
        let config = self.dialog.config_mut();
        config.title = Some(match kind {
            PickKind::Folder => format!("Open {} folder", side.title().to_lowercase()),
            PickKind::File => format!("Open {} file", side.title().to_lowercase()),
        });
        if let Some(start) = start {
            config.initial_directory = start;
        }
        match kind {
            PickKind::Folder => self.dialog.pick_directory(),
            PickKind::File => self.dialog.pick_file(),
        }
        self.pending = Some((side, kind));
    }

    pub fn is_open(&self) -> bool {
        *self.dialog.state() == DialogState::Open
    }

    /// Draws the dialog while it is open and turns a pick into a command.
    pub fn show(&mut self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        self.dialog.update(ctx);
        if let Some(path) = self.dialog.take_picked()
            && let Some((side, kind)) = self.pending.take()
        {
            commands.push(match kind {
                PickKind::Folder => Command::OpenFolder { side, path },
                PickKind::File => Command::SelectFile { side, path },
            });
        }
    }
}
