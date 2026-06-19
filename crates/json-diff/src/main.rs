use anyhow::Error;
use eframe::egui::{self, Button, CollapsingHeader, RichText, TextEdit, WidgetText};
use egui::{Color32, Ui, Widget};
use egui_json_tree::{
    JsonTree,
    value::{JsonTreeValue, ToJsonTreeValue},
};
use json_diff_core::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

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
    old_json: BTreeMap<String, Value>,
    new_json: BTreeMap<String, Value>,
    err_left: Option<Error>,
    err_right: Option<Error>,
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
        let diff = gen_diff_json(&self.old_json, &self.new_json);
        JsonTree::new("Diff Tree", &diff).show(ui);
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
