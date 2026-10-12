//! The left side bar: two stacked folder explorers, original on top and
//! modified below.

use super::Palette;
use super::file_picker::{FilePicker, PickKind};
use crate::state::{AppState, Command, Side};
use eframe::egui::{self, Color32, Rect, RichText, Ui};
use json_diff_core::PairStatus;
use std::path::{Component, Path, PathBuf};

/// Draws both explorers and returns the screen rect of each half, so dropped
/// folders can be routed to the half they were dropped on.
pub fn explorer(
    ui: &mut Ui,
    state: &AppState,
    commands: &mut Vec<Command>,
    picker: &mut FilePicker,
    palette: &Palette,
    path_inputs: &mut [String; 2],
) -> [Rect; 2] {
    let half = ui.available_height() / 2.0;
    let top = egui::Panel::top("explorer_original")
        .resizable(true)
        .default_size(half)
        .size_range(80.0..=(ui.available_height() - 80.0).max(80.0))
        .show_inside(ui, |ui| {
            section(
                ui,
                state,
                Side::Left,
                commands,
                picker,
                palette,
                &mut path_inputs[0],
            )
        })
        .response
        .rect;
    let bottom = egui::CentralPanel::default()
        .show_inside(ui, |ui| {
            section(
                ui,
                state,
                Side::Right,
                commands,
                picker,
                palette,
                &mut path_inputs[1],
            )
        })
        .response
        .rect;
    [top, bottom]
}

fn section(
    ui: &mut Ui,
    state: &AppState,
    side: Side,
    commands: &mut Vec<Command>,
    picker: &mut FilePicker,
    palette: &Palette,
    path_input: &mut String,
) {
    let folder = state.folders[side.index()].as_ref();

    ui.horizontal(|ui| {
        ui.label(RichText::new(side.title().to_uppercase()).small().strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if folder.is_some() && ui.small_button("×").on_hover_text("Close folder").clicked() {
                commands.push(Command::CloseFolder { side });
            }
            if folder.is_some()
                && ui
                    .small_button("Refresh")
                    .on_hover_text("Rescan folders (F5)")
                    .clicked()
            {
                commands.push(Command::RefreshFolders);
            }
            if ui
                .small_button("File…")
                .on_hover_text("Open a single file")
                .clicked()
            {
                picker.open(state, side, PickKind::File);
            }
            if ui
                .small_button("Folder…")
                .on_hover_text("Open a folder")
                .clicked()
            {
                picker.open(state, side, PickKind::Folder);
            }
        });
    });

    let Some(folder) = folder else {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No folder opened").weak());
            ui.add_space(4.0);
            if ui.button("Open Folder").clicked() {
                picker.open(state, side, PickKind::Folder);
            }
        });
        ui.add_space(6.0);
        let response = ui.add(
            egui::TextEdit::singleline(path_input)
                .hint_text("or type a path and press Enter")
                .desired_width(f32::INFINITY),
        );
        if response.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !path_input.trim().is_empty()
        {
            commands.push(Command::OpenFolder {
                side,
                path: PathBuf::from(path_input.trim()),
            });
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("You can also drop a folder here.")
                .small()
                .weak(),
        );
        fill_half(ui);
        return;
    };

    let name = folder.root.file_name().map_or_else(
        || folder.root.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    ui.label(RichText::new(name).strong())
        .on_hover_text(folder.root.display().to_string());
    ui.separator();

    if folder.files.is_empty() {
        ui.label(RichText::new("No .json files in this folder").weak());
        fill_half(ui);
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt(("explorer_scroll", side))
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            tree(
                ui,
                state,
                side,
                &folder.files,
                0,
                Path::new(""),
                commands,
                palette,
            );
        });
}

/// Renders `files` (sorted, all sharing the first `depth` components) as a
/// tree. Sorted paths keep every directory's files contiguous, so each level
/// is a linear scan and closed directories cost nothing.
#[allow(clippy::too_many_arguments)]
fn tree(
    ui: &mut Ui,
    state: &AppState,
    side: Side,
    files: &[PathBuf],
    depth: usize,
    prefix: &Path,
    commands: &mut Vec<Command>,
    palette: &Palette,
) {
    let component = |path: &Path| -> Option<String> {
        path.components()
            .nth(depth)
            .and_then(|component| match component {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
    };

    let mut start = 0;
    while start < files.len() {
        let Some(name) = component(&files[start]) else {
            start += 1;
            continue;
        };
        let is_dir = files[start].components().count() > depth + 1;
        let mut end = start + 1;
        if is_dir {
            while end < files.len()
                && component(&files[end]).as_deref() == Some(name.as_str())
                && files[end].components().count() > depth + 1
            {
                end += 1;
            }
            let dir = prefix.join(&name);
            let changed = files[start..end]
                .iter()
                .any(|path| matches!(state.pair_status(path), Some(status) if status != PairStatus::Identical));
            let mut title = RichText::new(format!("📁 {name}"));
            if changed {
                title = title.color(palette.modified_file);
            }
            egui::CollapsingHeader::new(title)
                .id_salt(("dir", side, &dir))
                .default_open(depth == 0 && files.len() < 200)
                .show(ui, |ui| {
                    tree(
                        ui,
                        state,
                        side,
                        &files[start..end],
                        depth + 1,
                        &dir,
                        commands,
                        palette,
                    );
                });
        } else {
            file_row(ui, state, side, &files[start], &name, commands, palette);
        }
        start = end;
    }
}

fn file_row(
    ui: &mut Ui,
    state: &AppState,
    side: Side,
    path: &Path,
    name: &str,
    commands: &mut Vec<Command>,
    palette: &Palette,
) {
    let status = state.pair_status(path);
    let (badge, color) = match status {
        Some(PairStatus::Modified) => ("M", palette.modified_file),
        Some(PairStatus::OnlyLeft) => ("D", palette.removed_border),
        Some(PairStatus::OnlyRight) => ("A", palette.added_border),
        _ => ("", Color32::TRANSPARENT),
    };
    let selected = state.is_selected(side, path);
    let mut text = RichText::new(name);
    if !badge.is_empty() {
        text = text.color(color);
    }

    let response = ui
        .horizontal(|ui| {
            let response = ui.selectable_label(selected, text);
            if !badge.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(badge).color(color).strong().monospace());
                });
            }
            response
        })
        .inner;

    let paired = !state.pairs.is_empty();
    let response = response.on_hover_text(if paired {
        format!(
            "{}\nClick: compare with the other folder\nRight-click: more",
            path.display()
        )
    } else {
        path.display().to_string()
    });
    if response.clicked() {
        commands.push(if paired {
            Command::OpenPair {
                path: path.to_path_buf(),
            }
        } else {
            Command::SelectFile {
                side,
                path: path.to_path_buf(),
            }
        });
    }
    response.context_menu(|ui| {
        if paired && ui.button("Compare with other folder").clicked() {
            commands.push(Command::OpenPair {
                path: path.to_path_buf(),
            });
        }
        if ui
            .button(format!("Show as {} only", side.title().to_lowercase()))
            .clicked()
        {
            commands.push(Command::SelectFile {
                side,
                path: path.to_path_buf(),
            });
        }
    });
}

/// Claims the rest of the section. A resizable panel takes the size of its
/// content, so a short empty state would otherwise shrink the top half.
fn fill_half(ui: &mut Ui) {
    ui.allocate_space(ui.available_size());
}
