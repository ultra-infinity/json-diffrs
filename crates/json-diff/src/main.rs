use anyhow::Error;
use eframe::egui::{self, Button, CollapsingHeader, RichText, TextEdit, WidgetText};
use egui::{Color32, Ui, Widget};
use json_diff_core::*;
use serde_json::Value;
use std::collections::BTreeMap;

// Map the structural status to your exact requested colors
fn status_color(status: &NodeStatus) -> Color32 {
    match status {
        NodeStatus::Unchanged => Color32::GRAY,
        NodeStatus::Modified => Color32::from_rgb(255, 140, 0), // Orange
        NodeStatus::Added => Color32::from_rgb(50, 205, 50),    // Green
        NodeStatus::Deleted => Color32::from_rgb(220, 20, 60),  // Red
    }
}

pub struct DiffTab<'a> {
    diff: &'a BTreeMap<String, WrappedValue<'a>>,
}

impl<'a> DiffTab<'a> {
    pub fn new(diff: &'a BTreeMap<String, WrappedValue<'a>>) -> Self {
        Self { diff }
    }
}

impl<'a> Widget for DiffTab<'a> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        ui.vertical(|ui| {
            for (key, value) in self.diff {
                render_wrapped_value(ui, key, value);
            }
        })
        .response
    }
}

/// Recursive helper function to render the diff tree
fn render_wrapped_value(ui: &mut Ui, key: &String, wrapped_val: &WrappedValue) {
    let overall_status = wrapped_val.status();

    // Generate styled text for the key based on its status
    let key = format!("\"{}\"", key);
    let header_text = match overall_status {
        NodeStatus::Unchanged => RichText::new(key),
        status => RichText::new(key).color(status_color(&status)),
    };

    match wrapped_val {
        WrappedValue::Object(object) => {
            CollapsingHeader::new(header_text).show(ui, |ui| {
                for (child_key, child_val) in object {
                    render_wrapped_value(ui, child_key, child_val);
                }
            });
        }
        WrappedValue::Array(array) => {
            CollapsingHeader::new(header_text).show(ui, |ui| {
                for (index, child_val) in array.into_iter().enumerate() {
                    render_wrapped_value(ui, &format!("[{}]", index), child_val);
                }
            });
        }
        WrappedValue::Value(diff_node) => {
            let DiffNode { value, status } = diff_node;
            ui.horizontal(|ui| {
                // Render the label/key name
                ui.label(header_text);
                ui.label(":");

                // Render the values side-by-side inside this single row depending on status
                let val: WidgetText = match status {
                    NodeStatus::Unchanged => get_node_val(value).into(),
                    NodeStatus::Added => {
                        let text_string = get_node_val(value);
                        let text =
                            RichText::new(text_string).color(status_color(&NodeStatus::Added));
                        text.into()
                    }
                    NodeStatus::Deleted => {
                        let text_string = get_node_val(value);
                        let text =
                            RichText::new(text_string).color(status_color(&NodeStatus::Deleted));
                        text.into()
                    }
                    NodeStatus::Modified => {
                        let text_string = get_node_val(value);
                        let text =
                            RichText::new(text_string).color(status_color(&NodeStatus::Modified));
                        text.into()
                    }
                };

                ui.label(val);
            });
        }
    }
}

fn get_node_val(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(bool) => bool.to_string(),
        Value::String(string) => format!("\"{}\"", string),
        Value::Number(number) => number.to_string(),
        _ => unreachable!("Only Leaf Nodes"),
    }
}

pub enum Input {
    String(String),
    Json(BTreeMap<String, Value>),
    Error(Error),
}

impl Default for Input {
    fn default() -> Self {
        Input::String(String::default())
    }
}

enum Mode {
    Input,
    Compare,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Input
    }
}

#[derive(Default)]
struct MyEguiApp {
    old_input: String,
    new_input: String,
    old_json: BTreeMap<String, Value>,
    new_json: BTreeMap<String, Value>,
    err_left: Option<Error>,
    err_right: Option<Error>,
    mode: Mode,
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
        if let Mode::Compare = self.mode {
            match serde_json::from_str::<BTreeMap<String, Value>>(&self.old_input) {
                Ok(json) => self.old_json = json,
                Err(err) => self.err_left = Some(err.into()),
            };

            match serde_json::from_str::<BTreeMap<String, Value>>(&self.new_input) {
                Ok(json) => self.new_json = json,
                Err(err) => self.err_right = Some(err.into()),
            };
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.columns(2, |columns| {
                columns[0].add(TextEdit::multiline(&mut self.old_input));
                match self.mode {
                    Mode::Input => columns[1].add(TextEdit::multiline(&mut self.new_input)),
                    Mode::Compare => {
                        let diff = gen_diff_json(&self.old_json, &self.new_json);
                        columns[1].add(DiffTab::new(&diff))
                    }
                }
            });

            let text = match self.mode {
                Mode::Input => "Compare",
                Mode::Compare => "New",
            };

            if ui.add(Button::new(text)).clicked() {
                self.mode = match self.mode {
                    Mode::Input => Mode::Compare,
                    Mode::Compare => Mode::Input,
                }
            }
        });
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
