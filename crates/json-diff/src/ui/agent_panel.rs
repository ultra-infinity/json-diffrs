//! The assistant panel: model settings, the conversation, and a live log of
//! every tool call the agent makes.

use super::Palette;
use crate::agent::{AgentHandle, AgentRequest, api_key_env, known_providers};
use eframe::egui::{self, RichText, Ui};

pub enum ChatEntry {
    User(String),
    Assistant(String),
    Tool {
        name: String,
        arguments: String,
        result: Result<String, String>,
    },
    Error(String),
    Info(String),
}

#[derive(Default)]
pub struct AgentPanelState {
    pub model: String,
    pub api_key: String,
    pub connected: Option<String>,
    pub busy: bool,
    pub input: String,
    pub entries: Vec<ChatEntry>,
}

impl AgentPanelState {
    pub fn connect(&mut self, agent: &AgentHandle) {
        let key = self.api_key.trim();
        agent.send(AgentRequest::Connect {
            model: self.model.trim().to_owned(),
            api_key: (!key.is_empty()).then(|| key.to_owned()),
        });
    }
}

pub fn agent_panel(
    ui: &mut Ui,
    panel: &mut AgentPanelState,
    agent: &AgentHandle,
    palette: &Palette,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("ASSISTANT").small().strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("🗑")
                .on_hover_text("New conversation")
                .clicked()
            {
                panel.entries.clear();
                agent.send(AgentRequest::Reset);
            }
        });
    });

    egui::CollapsingHeader::new(match &panel.connected {
        Some(model) => format!("Model: {model}"),
        None => "Model: not connected".to_owned(),
    })
    .id_salt("agent_settings")
    .default_open(panel.connected.is_none())
    .show(ui, |ui| {
        let model = ui.add(
            egui::TextEdit::singleline(&mut panel.model)
                .hint_text("provider:model, e.g. openai:gpt-5.5")
                .desired_width(f32::INFINITY),
        );
        model.on_hover_text(format!("Providers: {}", known_providers().join(", ")));
        let env = api_key_env(&panel.model);
        ui.add(
            egui::TextEdit::singleline(&mut panel.api_key)
                .password(true)
                .hint_text(match env {
                    Some(env) => format!("API key (default: ${env})"),
                    None => "API key (optional)".to_owned(),
                })
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                !panel.model.trim().is_empty() && !panel.busy,
                egui::Button::new("Connect"),
            )
            .clicked()
        {
            panel.connect(agent);
        }
    });
    ui.separator();

    let input_height = 64.0;
    egui::ScrollArea::vertical()
        .id_salt("agent_chat")
        .auto_shrink(false)
        .stick_to_bottom(true)
        .max_height((ui.available_height() - input_height - 12.0).max(40.0))
        .show(ui, |ui| {
            if panel.entries.is_empty() {
                ui.label(
                    RichText::new(
                        "Ask about the diff, or to drive the app: \"open ~/a and ~/b and \
                         summarise what changed\", \"explain change 3\", \"sort keys\".",
                    )
                    .weak(),
                );
            }
            for (index, entry) in panel.entries.iter().enumerate() {
                chat_entry(ui, index, entry, palette);
                ui.add_space(6.0);
            }
            if panel.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("working…").weak());
                });
            }
        });

    ui.separator();
    let response = ui.add_enabled(
        panel.connected.is_some() && !panel.busy,
        egui::TextEdit::multiline(&mut panel.input)
            .hint_text(if panel.connected.is_some() {
                "Message (Enter to send)"
            } else {
                "Connect a model first"
            })
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    let enter = response.has_focus()
        && ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
    ui.horizontal(|ui| {
        if panel.busy {
            if ui.button("Stop").clicked() {
                agent.cancel();
            }
        } else if (ui
            .add_enabled(panel.connected.is_some(), egui::Button::new("Send"))
            .clicked()
            || enter)
            && !panel.input.trim().is_empty()
        {
            let prompt = panel.input.trim().to_owned();
            panel.input.clear();
            panel.entries.push(ChatEntry::User(prompt.clone()));
            panel.busy = true;
            agent.send(AgentRequest::Prompt(prompt));
        }
    });
}

fn chat_entry(ui: &mut Ui, index: usize, entry: &ChatEntry, palette: &Palette) {
    match entry {
        ChatEntry::User(text) => {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(6.0)
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("You").small().strong());
                    ui.label(text);
                });
        }
        ChatEntry::Assistant(text) => {
            ui.label(RichText::new("Assistant").small().strong());
            ui.label(text);
        }
        ChatEntry::Tool {
            name,
            arguments,
            result,
        } => {
            let (mark, color) = match result {
                Ok(_) => ("✔", palette.added_border),
                Err(_) => ("✖", palette.removed_border),
            };
            let header = RichText::new(format!("{mark} {name} {arguments}"))
                .monospace()
                .small()
                .color(color);
            egui::CollapsingHeader::new(header)
                .id_salt(("tool_call", index))
                .show(ui, |ui| {
                    let text = match result {
                        Ok(text) | Err(text) => text,
                    };
                    ui.label(RichText::new(text).monospace().small());
                });
        }
        ChatEntry::Error(text) => {
            ui.label(RichText::new(text).color(palette.removed_border));
        }
        ChatEntry::Info(text) => {
            ui.label(RichText::new(text).weak().small());
        }
    }
}
