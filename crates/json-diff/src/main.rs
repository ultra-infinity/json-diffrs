mod agent;
mod samples;
mod state;
mod tools;
mod ui;

use agent::{AgentEvent, AgentHandle};
use eframe::egui::{self, Color32, Rect, RichText};
use state::{AppState, Command, PanelKind, Side, Theme, ViewMode, WindowAction};
use std::path::PathBuf;
use ui::{AgentPanelState, ChatEntry, CodeEditor, FilePicker, Palette, PickKind};

struct JsonDiffApp {
    state: AppState,
    agent: AgentHandle,
    chat: AgentPanelState,
    /// Typed folder paths in the empty explorers.
    path_inputs: [String; 2],
    /// Screen rects of the two explorer halves, for routing dropped folders.
    explorer_rects: [Rect; 2],
    applied_theme: Option<Theme>,
    /// The in-app Open Folder / Open File dialog.
    picker: FilePicker,
}

impl JsonDiffApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            state: AppState::default(),
            agent: agent::spawn(cc.egui_ctx.clone()),
            chat: AgentPanelState::default(),
            path_inputs: Default::default(),
            explorer_rects: [Rect::NOTHING; 2],
            applied_theme: None,
            picker: FilePicker::default(),
        };

        // `json-diff <original> <modified>` opens two files or two folders.
        let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
        if let [left, right] = args.as_slice() {
            for (side, path) in Side::BOTH.into_iter().zip([left, right]) {
                let command = if path.is_dir() {
                    Command::OpenFolder {
                        side,
                        path: path.clone(),
                    }
                } else {
                    Command::SelectFile {
                        side,
                        path: std::path::absolute(path).unwrap_or(path.clone()),
                    }
                };
                let _ = app.state.apply(command);
            }
            // Two folders: start on the first file that differs.
            if let Some(pair) = app
                .state
                .pairs
                .iter()
                .find(|pair| pair.status != json_diff_core::PairStatus::Identical)
            {
                let _ = app.state.apply(Command::OpenPair {
                    path: pair.path.clone(),
                });
            }
        } else {
            let _ = app.state.apply(Command::SetText {
                side: Side::Left,
                text: samples::ORIGINAL.into(),
            });
            let _ = app.state.apply(Command::SetText {
                side: Side::Right,
                text: samples::MODIFIED.into(),
            });
            app.state.set_status(
                "Showing a sample. Open folders in the explorer to compare your own files.",
                false,
            );
        }

        if let Ok(model) = std::env::var(agent::MODEL_ENV) {
            app.chat.model = model;
            app.chat.connect(&app.agent);
        }
        app
    }

    /// Applies tool calls coming from the agent, exactly like UI commands.
    fn handle_agent_events(&mut self) {
        while let Ok(event) = self.agent.events.try_recv() {
            match event {
                AgentEvent::ToolCall {
                    tool,
                    arguments,
                    command,
                    reply,
                } => {
                    let result = self.state.apply(command);
                    self.chat.entries.push(ChatEntry::Tool {
                        name: tool.to_owned(),
                        arguments: abbreviate(&arguments.to_string(), 80),
                        result: result
                            .as_ref()
                            .map(|value| {
                                abbreviate(
                                    &serde_json::to_string_pretty(value).unwrap_or_default(),
                                    2_000,
                                )
                            })
                            .map_err(Clone::clone),
                    });
                    let _ = reply.send(result);
                }
                AgentEvent::Connected { model } => {
                    self.chat
                        .entries
                        .push(ChatEntry::Info(format!("Connected to {model}.")));
                    self.chat.connected = Some(model);
                }
                AgentEvent::Reply(text) => {
                    self.chat.busy = false;
                    self.chat.entries.push(ChatEntry::Assistant(text));
                }
                AgentEvent::Failed(error) => {
                    self.chat.busy = false;
                    self.chat.entries.push(ChatEntry::Error(error));
                }
                AgentEvent::Cancelled => {
                    self.chat.busy = false;
                    self.chat.entries.push(ChatEntry::Info("Stopped.".into()));
                }
            }
        }
    }

    fn shortcuts(&self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        // The modal dialog owns the keyboard while it is open.
        if self.picker.is_open() {
            return;
        }
        ctx.input(|input| {
            if input.key_pressed(egui::Key::F7) {
                commands.push(if input.modifiers.shift {
                    Command::PreviousChange
                } else {
                    Command::NextChange
                });
            }
            if input.modifiers.alt && input.key_pressed(egui::Key::ArrowDown) {
                commands.push(Command::NextChange);
            }
            if input.modifiers.alt && input.key_pressed(egui::Key::ArrowUp) {
                commands.push(Command::PreviousChange);
            }
            if input.key_pressed(egui::Key::F5) {
                commands.push(Command::RefreshFolders);
            }
        });
    }

    /// Dropped folders open in the explorer half they land on; dropped files
    /// open on the side of the window they land on.
    fn handle_drops(&self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        let (files, pointer) =
            ctx.input(|input| (input.raw.dropped_files.clone(), input.pointer.latest_pos()));
        let center = ctx.content_rect().center().x;
        for file in files {
            let Some(path) = file.path else { continue };
            let side = match pointer {
                Some(pos) if self.explorer_rects[1].contains(pos) => Side::Right,
                Some(pos) if self.explorer_rects[0].contains(pos) => Side::Left,
                Some(pos) if pos.x > center => Side::Right,
                _ => Side::Left,
            };
            commands.push(if path.is_dir() {
                Command::OpenFolder { side, path }
            } else {
                Command::SelectFile { side, path }
            });
        }
    }

    fn apply_ui_settings(&mut self, ctx: &egui::Context) {
        if (ctx.zoom_factor() - self.state.zoom).abs() > f32::EPSILON {
            ctx.set_zoom_factor(self.state.zoom);
        }
        if self.applied_theme != Some(self.state.theme) {
            ctx.set_theme(match self.state.theme {
                Theme::Dark => egui::Theme::Dark,
                Theme::Light => egui::Theme::Light,
            });
            self.applied_theme = Some(self.state.theme);
        }
        for action in self.state.window_requests.drain(..) {
            ctx.send_viewport_cmd(match action {
                WindowAction::Minimize => egui::ViewportCommand::Minimized(true),
                WindowAction::Maximize => egui::ViewportCommand::Maximized(true),
                WindowAction::Restore => egui::ViewportCommand::Maximized(false),
                WindowAction::Close => egui::ViewportCommand::Close,
            });
        }
    }
}

impl eframe::App for JsonDiffApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.state.zoom = ctx.zoom_factor();
        self.handle_agent_events();

        let palette = Palette::new(self.state.theme == Theme::Dark);
        let mut commands = Vec::new();
        self.shortcuts(&ctx, &mut commands);

        egui::Panel::top("title_bar").show_inside(ui, |ui| {
            title_bar(ui, &self.state, &mut commands, &mut self.picker)
        });
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(palette.status_bar)
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show_inside(ui, |ui| status_bar(ui, &self.state, &self.chat));

        if self.state.show_explorer {
            egui::Panel::left("explorer")
                .resizable(true)
                .default_size(260.0)
                .size_range(160.0..=600.0)
                .show_inside(ui, |ui| {
                    self.explorer_rects = ui::explorer(
                        ui,
                        &self.state,
                        &mut commands,
                        &mut self.picker,
                        &palette,
                        &mut self.path_inputs,
                    );
                });
        } else {
            self.explorer_rects = [Rect::NOTHING; 2];
        }
        if self.state.show_agent {
            egui::Panel::right("agent")
                .resizable(true)
                .default_size(340.0)
                .size_range(240.0..=700.0)
                .show_inside(ui, |ui| {
                    ui::agent_panel(ui, &mut self.chat, &self.agent, &palette)
                });
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            toolbar(ui, &self.state, &mut commands);
            ui.separator();
            match self.state.view_mode {
                ViewMode::Diff => ui::diff_view(ui, &mut self.state, &palette, &mut commands),
                ViewMode::Edit => editors(ui, &mut self.state),
            }
        });

        self.picker.show(&ctx, &mut commands);
        self.handle_drops(&ctx, &mut commands);
        for command in commands {
            // Failures are shown in the status bar by `apply`.
            let _ = self.state.apply(command);
        }
        self.apply_ui_settings(&ctx);
    }
}

/// Menus, title and window buttons; also the drag handle of the borderless
/// window.
fn title_bar(
    ui: &mut egui::Ui,
    state: &AppState,
    commands: &mut Vec<Command>,
    picker: &mut FilePicker,
) {
    // Registered first so the menus drawn on top keep their clicks.
    let drag = ui.interact(
        ui.max_rect(),
        ui.id().with("title_drag"),
        egui::Sense::click_and_drag(),
    );
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let maximized = ui
        .ctx()
        .input(|input| input.viewport().maximized.unwrap_or(false));
    if drag.double_clicked() {
        commands.push(Command::Window {
            action: if maximized {
                WindowAction::Restore
            } else {
                WindowAction::Maximize
            },
        });
    }

    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            for side in Side::BOTH {
                if ui
                    .button(format!("Open {} Folder…", side.title()))
                    .clicked()
                {
                    picker.open(state, side, PickKind::Folder);
                }
            }
            for side in Side::BOTH {
                if ui.button(format!("Open {} File…", side.title())).clicked() {
                    picker.open(state, side, PickKind::File);
                }
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Refresh").shortcut_text("F5"))
                .clicked()
            {
                commands.push(Command::RefreshFolders);
            }
            if ui.button("Swap Sides").clicked() {
                commands.push(Command::SwapSides);
            }
            for side in Side::BOTH {
                if ui.button(format!("Clear {}", side.title())).clicked() {
                    commands.push(Command::ClearSide { side });
                }
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                commands.push(Command::Window {
                    action: WindowAction::Close,
                });
            }
        });
        ui.menu_button("View", |ui| {
            for (mode, label) in [
                (ViewMode::Diff, "Side-by-side Diff"),
                (ViewMode::Edit, "Edit"),
            ] {
                if ui.radio(state.view_mode == mode, label).clicked() {
                    commands.push(Command::SetViewMode { mode });
                }
            }
            ui.separator();
            for (panel, label, shown) in [
                (PanelKind::Explorer, "Explorer", state.show_explorer),
                (PanelKind::Agent, "Assistant", state.show_agent),
            ] {
                if ui.checkbox(&mut shown.clone(), label).clicked() {
                    commands.push(Command::SetPanelVisible {
                        panel,
                        visible: Some(!shown),
                    });
                }
            }
            ui.separator();
            fold_buttons(ui, state, commands);
            ui.separator();
            for (theme, label) in [(Theme::Dark, "Dark Theme"), (Theme::Light, "Light Theme")] {
                if ui.radio(state.theme == theme, label).clicked() {
                    commands.push(Command::SetTheme { theme });
                }
            }
            ui.separator();
            if ui.button("Zoom In").clicked() {
                commands.push(Command::ZoomIn);
            }
            if ui.button("Zoom Out").clicked() {
                commands.push(Command::ZoomOut);
            }
            if ui.button("Reset Zoom").clicked() {
                commands.push(Command::SetZoom { zoom: 1.0 });
            }
        });
        ui.menu_button("Go", |ui| {
            if ui
                .add(egui::Button::new("Next Change").shortcut_text("F7"))
                .clicked()
            {
                commands.push(Command::NextChange);
            }
            if ui
                .add(egui::Button::new("Previous Change").shortcut_text("Shift+F7"))
                .clicked()
            {
                commands.push(Command::PreviousChange);
            }
        });
        ui.menu_button("Options", |ui| option_toggles(ui, state, commands));

        let title = format!(
            "{} ↔ {} — json-diff",
            state.docs[0].label(),
            state.docs[1].label()
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if ui
                .add(egui::Button::new("×").frame(false))
                .on_hover_text("Close")
                .clicked()
            {
                commands.push(Command::Window {
                    action: WindowAction::Close,
                });
            }
            let (icon, action) = if maximized {
                ("🗗", WindowAction::Restore)
            } else {
                ("🗖", WindowAction::Maximize)
            };
            if ui.add(egui::Button::new(icon).frame(false)).clicked() {
                commands.push(Command::Window { action });
            }
            if ui.add(egui::Button::new("🗕").frame(false)).clicked() {
                commands.push(Command::Window {
                    action: WindowAction::Minimize,
                });
            }
            ui.add_space(16.0);
            ui.centered_and_justified(|ui| ui.label(RichText::new(title).weak()));
        });
    });
}

/// Expand all, collapse all, and reveal changes.
fn fold_buttons(ui: &mut egui::Ui, state: &AppState, commands: &mut Vec<Command>) {
    let foldable = state.diff.is_foldable();
    for (label, tip, command) in [
        (
            "Expand all",
            "Expand every object and array",
            Command::ExpandAll,
        ),
        (
            "Collapse all",
            "Show only the top-level keys",
            Command::CollapseAll,
        ),
        (
            "Reveal changes",
            "Expand only what leads to a change",
            Command::RevealChanges,
        ),
    ] {
        if ui
            .add_enabled(foldable, egui::Button::new(label))
            .on_hover_text(tip)
            .on_disabled_hover_text("Folding needs Format JSON on and valid JSON")
            .clicked()
        {
            commands.push(command);
        }
    }
}

fn option_toggles(ui: &mut egui::Ui, state: &AppState, commands: &mut Vec<Command>) {
    let options = state.options;
    if ui
        .checkbox(&mut options.format.clone(), "Format JSON")
        .on_hover_text("Pretty print both sides and align lines by JSON structure")
        .clicked()
    {
        commands.push(Command::SetOptions {
            format: Some(!options.format),
            sort_keys: None,
            ignore_whitespace: None,
        });
    }
    if ui
        .add_enabled(
            options.format,
            egui::Checkbox::new(&mut options.sort_keys.clone(), "Sort keys"),
        )
        .on_hover_text("Ignore differences in key order")
        .clicked()
    {
        commands.push(Command::SetOptions {
            format: None,
            sort_keys: Some(!options.sort_keys),
            ignore_whitespace: None,
        });
    }
    if ui
        .add_enabled(
            !options.format,
            egui::Checkbox::new(&mut options.ignore_whitespace.clone(), "Ignore whitespace"),
        )
        .on_hover_text("Only applies when Format JSON is off")
        .clicked()
    {
        commands.push(Command::SetOptions {
            format: None,
            sort_keys: None,
            ignore_whitespace: Some(!options.ignore_whitespace),
        });
    }
}

fn toolbar(ui: &mut egui::Ui, state: &AppState, commands: &mut Vec<Command>) {
    ui.horizontal_wrapped(|ui| {
        for (mode, label) in [(ViewMode::Diff, "Diff"), (ViewMode::Edit, "Edit")] {
            if ui
                .selectable_label(state.view_mode == mode, label)
                .clicked()
            {
                commands.push(Command::SetViewMode { mode });
            }
        }
        ui.separator();

        let count = state.diff.view.hunks.len();
        let has_changes = count > 0;
        if ui
            .add_enabled(has_changes, egui::Button::new("Previous"))
            .on_hover_text("Previous change (Shift+F7)")
            .clicked()
        {
            commands.push(Command::PreviousChange);
        }
        if ui
            .add_enabled(has_changes, egui::Button::new("Next"))
            .on_hover_text("Next change (F7)")
            .clicked()
        {
            commands.push(Command::NextChange);
        }
        ui.label(match (count, state.current_hunk) {
            (0, _) => "No changes".to_owned(),
            (count, Some(index)) => format!("Change {} of {count}", index + 1),
            (count, None) => format!("{count} changes"),
        });
        ui.separator();
        option_toggles(ui, state, commands);
        ui.separator();
        if ui
            .button("Swap sides")
            .on_hover_text("Swap original and modified")
            .clicked()
        {
            commands.push(Command::SwapSides);
        }
        ui.separator();
        fold_buttons(ui, state, commands);
        ui.separator();
        for (panel, label, tip, shown) in [
            (
                PanelKind::Explorer,
                "Explorer",
                "Toggle the folder explorer",
                state.show_explorer,
            ),
            (
                PanelKind::Agent,
                "Assistant",
                "Toggle the assistant panel",
                state.show_agent,
            ),
        ] {
            if ui
                .selectable_label(shown, label)
                .on_hover_text(tip)
                .clicked()
            {
                commands.push(Command::SetPanelVisible {
                    panel,
                    visible: None,
                });
            }
        }
    });
}

fn editors(ui: &mut egui::Ui, state: &mut AppState) {
    let mut changed = false;
    ui.columns(2, |columns| {
        for side in Side::BOTH {
            let ui = &mut columns[side.index()];
            ui.label(
                RichText::new(format!(
                    "{} — {}",
                    side.title(),
                    state.docs[side.index()].label()
                ))
                .strong(),
            );
            let salt = match side {
                Side::Left => "editor_left",
                Side::Right => "editor_right",
            };
            changed |= ui
                .add(CodeEditor::new(&mut state.docs[side.index()].text, salt))
                .changed();
        }
    });
    if changed {
        state.recompute(false);
    }
}

fn status_bar(ui: &mut egui::Ui, state: &AppState, chat: &AgentPanelState) {
    let text = Color32::WHITE;
    ui.horizontal(|ui| {
        if let Some(status) = &state.status {
            let label = if status.is_error {
                format!("⚠ {}", status.text)
            } else {
                status.text.clone()
            };
            ui.label(RichText::new(label).color(if status.is_error {
                Color32::from_rgb(255, 210, 210)
            } else {
                text
            }));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let assistant = match (&chat.connected, chat.busy) {
                (Some(_), true) => "Assistant: working…".to_owned(),
                (Some(model), false) => format!("Assistant: {model}"),
                (None, _) => "Assistant: not connected".to_owned(),
            };
            ui.label(RichText::new(assistant).color(text));
            ui.label(RichText::new(format!("{:.0}%", state.zoom * 100.0)).color(text));
            let view = &state.diff.view;
            let semantic = match &state.diff.changes {
                Some(changes) => format!("{} semantic changes  ·  ", changes.len()),
                None => String::new(),
            };
            ui.label(
                RichText::new(format!(
                    "{semantic}{} changed blocks  +{} −{} ~{}",
                    view.hunks.len(),
                    view.added_lines(),
                    view.removed_lines(),
                    view.modified_lines()
                ))
                .color(text),
            );
        });
    });
}

fn abbreviate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        text.to_owned()
    } else {
        format!("{}…", &text[..text.floor_char_boundary(limit)])
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("json-diff")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([720.0, 420.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "json-diff",
        options,
        Box::new(|cc| Ok(Box::new(JsonDiffApp::new(cc)))),
    )
}
