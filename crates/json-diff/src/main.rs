mod ui;
use anyhow::Error;
use eframe::egui::{self, Layout, RichText, WidgetText, WidgetWithState};
use egui::{Color32, Ui, Widget};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use egui_json_tree::{DefaultExpand, JsonTree};
use json_diff_core::*;
use serde_json::{Value, json};
use std::{any::Any, marker::PhantomData};
use ui::*;

struct JsonDiffTab<'a, T>(PhantomData<&'a T>);

impl<'a, T> JsonDiffTab<'a, T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'a> TabViewer for JsonDiffTab<'a, JsonState<'a>> {
    type Tab = JsonState<'a>;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        match tab {
            JsonState::Json(_) => "Json Tab".into(),
            JsonState::Diff(_) => "Diff Tab".into(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            JsonState::Json(json) => JsonTree::new("Json Tree", *json)
                .default_expand(DefaultExpand::ToLevel(0))
                .show(ui),
            JsonState::Diff(diff) => JsonTree::new("Diff Tree", diff)
                .default_expand(DefaultExpand::ToLevel(0))
                .show(ui),
        };
    }
}

struct Tabs<'a, T> {
    dock_state: DockState<T>,
    phantom: PhantomData<&'a T>,
}

impl<'a> WidgetWithState for Tabs<'a, WrappedValue<'a>> {
    type State = DockState<WrappedValue<'a>>;
}

impl<'a> WidgetWithState for Tabs<'a, &'a Value> {
    type State = DockState<&'a Value>;
}
impl<'a, T> Tabs<'a, T> {
    pub fn new(dock_state: DockState<T>) -> Self {
        Self {
            dock_state,
            phantom: PhantomData,
        }
    }
}

impl<'a> Widget for Tabs<'a, JsonState<'a>> {
    fn ui(mut self, ui: &mut Ui) -> egui::Response {
        let mut diff_tab = JsonDiffTab::new();
        DockArea::new(&mut self.dock_state).show_inside(ui, &mut diff_tab);
        ui.response()
    }
}

// Map the structural status to your exact requested colors
fn status_color(status: &NodeStatus) -> Color32 {
    match status {
        NodeStatus::Unchanged => Color32::GRAY,
        NodeStatus::Modified => Color32::from_rgb(255, 140, 0), // Orange
        NodeStatus::Added => Color32::from_rgb(50, 205, 50),    // Green
        NodeStatus::Deleted => Color32::from_rgb(220, 20, 60),  // Red
    }
}

enum JsonDiffrsState {
    ExitClicked,
    New,
}

impl Default for JsonDiffrsState {
    fn default() -> Self {
        JsonDiffrsState::New
    }
}

struct JsonDiffrsApp {
    old_json: Value,
    new_json: Value,
    err: Option<Error>,
    zoom: f32,
    state: JsonDiffrsState,
}

impl Default for JsonDiffrsApp {
    fn default() -> Self {
        let old_json = Value::default();
        let new_json = Value::default();
        let err = Option::default();
        let zoom = 1.3;
        let state = JsonDiffrsState::default();

        Self {
            old_json,
            new_json,
            err,
            zoom,
            state,
        }
    }
}

impl JsonDiffrsApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_global_style.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        Self::default()
    }
}

impl eframe::App for JsonDiffrsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let JsonDiffrsState::ExitClicked = self.state {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let old_json = json!({ "foo": "bar", "fizz": [1, 2, 3]});
        let new_json = json!({ "foo": "baz", "fizz": [1, 2, 3]});

        self.new_json = serde_json::from_value(old_json).unwrap();
        self.old_json = serde_json::from_value(new_json).unwrap();
        ctx.set_zoom_factor(self.zoom);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::bottom("Footer").show_inside(ui, |ui| {
            ui.spacing();
            match self.err.take() {
                Some(e) => {
                    let text = RichText::new(e.to_string()).color(Color32::RED);
                    ui.label(text)
                }
                _ => ui.label("Everything is fine"),
            };
        });
        egui::Panel::top("Header").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {}
                    if ui.button("Exit").clicked() {
                        self.state = JsonDiffrsState::ExitClicked;
                    }
                });
                ui.menu_button("Zoom", |ui| {
                    if ui.button("+").clicked() {
                        self.zoom += 0.1;
                    }
                    if ui.button("-").clicked() {
                        if self.zoom - 0.1 > 0.0000 {
                            self.zoom -= 0.1;
                        }
                    }
                })
            });
        });
        egui::Panel::left("Left Panel").show_inside(ui, |ui| ui.label("AI Assist Incoming"));

        egui::CentralPanel::default().show_inside(ui, |ui| {
            use JsonDiffrsState::*;
            match self.state {
                New => {}
                _ => {}
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Ok(Box::new(JsonDiffrsApp::new(cc)))),
    )?;

    Ok(())
}
