use eframe::egui::{self, WidgetText, WidgetWithState};
use egui::{Color32, Ui, Widget};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use egui_json_tree::{DefaultExpand, JsonTree};
use json_diff_core::*;
use serde_json::{Value, json};
use std::marker::PhantomData;

enum JsonState<'a> {
    Json(&'a Value),
    Diff(WrappedValue<'a>),
}

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

#[derive(Default)]
struct MyEguiApp {
    old_json: Value,
    new_json: Value,
}

impl MyEguiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_global_style.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let old_json = json!({ "foo": "bar", "fizz": [1, 2, 3]});
        let new_json = json!({ "foo": "baz", "fizz": [1, 2, 3]});

        self.new_json = serde_json::from_value(old_json).unwrap();
        self.old_json = serde_json::from_value(new_json).unwrap();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let diff = WrappedValue::from((&self.old_json, &self.new_json));

        let old = JsonState::Json(&self.old_json);
        let diff = JsonState::Diff(diff);

        let mut dock_state = DockState::new(vec![old]);
        let surface = dock_state.main_surface_mut();

        surface.split_right(NodeIndex::root(), 0.50, vec![diff]);

        ui.add(Tabs::new(dock_state));
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
    )?;

    Ok(())
}
