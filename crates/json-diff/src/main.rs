mod ui;
use std::collections::HashMap;

use anyhow::Error;
use eframe::egui::{self, Layout, RichText, WidgetText, WidgetWithState};
use egui::{Color32, Ui, Widget};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use egui_json_tree::{DefaultExpand, JsonTree};
use json_diff_core::*;
use serde_json::{Value, json};
use ui::*;

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
struct JsonDiffrsApp {
    input_strings: HashMap<String, Entry<String>>,
    err: Option<Error>,
}

impl JsonDiffrsApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_global_style.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        let mut app = Self::default();
        let old = json!({
            "code": 200,
            "success": true,
            "payload": {
                "features": [
                    "serde",
                    "json"
                ],
                "homepage": null
            }
        })
        .to_string();

        let new = json!({
            "code": 200,
            "success": true,
            "payload": {
                "features": [
                    "serde",
                    "json"
                ],
                "homepage": null
            }
        })
        .to_string();

        app.input_strings
            .insert("Sample".into(), Entry { old, new });
        app
    }
}

impl eframe::App for JsonDiffrsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let old_json = json!({ "foo": "bar", "fizz": [1, 2, 3]});
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("Header").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| if ui.button("New").clicked() {});
                ui.menu_button("Zoom", |ui| {
                    if ui.button("+").clicked() {
                        let zoom_factor = ui.ctx().zoom_factor();
                        ui.ctx().set_zoom_factor(zoom_factor + 0.1);
                    }
                    if ui.button("-").clicked() {
                        let zoom_factor = ui.ctx().zoom_factor();
                        if zoom_factor - 0.1 > 0.0000 {
                            ui.ctx().set_zoom_factor(zoom_factor - 0.1);
                        }
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);

                    // Close Button (Borderless style)
                    if ui.add(egui::Button::new("❌")).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    // Maximize Button
                    if ui.add(egui::Button::new("🗖")).clicked() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                    }

                    // Minimize Button
                    if ui.add(egui::Button::new("🗕")).clicked() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                });
            });
        });
        egui::Panel::left("Left Panel").show_inside(ui, |ui| ui.label("AI Assist Incoming"));

        egui::Panel::bottom("Bottom Panel").show_inside(ui, |ui| {
            if let Some(err) = self.err.take() {
                let msg = err.to_string();
                ui.label(RichText::new(msg).color(Color32::RED));
            } else {
                ui.label("All Ok");
            }
        });

        egui::CentralPanel::default()
            .show_inside(ui, |ui| ui.add(MainView::new(&mut self.input_strings)));
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| Ok(Box::new(JsonDiffrsApp::new(cc)))),
    )?;

    Ok(())
}
