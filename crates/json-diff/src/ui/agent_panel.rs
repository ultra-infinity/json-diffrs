//! The assistant panel: model settings, the conversation, and a live log of
//! every tool call the agent makes.

use super::FilePicker;
use super::Palette;
use crate::agent::{
    AgentHandle, AgentRequest, ModelChoice, api_key_env, catalog_models, known_providers,
    split_reference,
};
use crate::agents_md::Source;
use crate::state::{AppState, Command};
use eframe::egui::{self, PopupCloseBehavior, RichText, Ui};
use std::path::Path;

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
    /// Provider selection, e.g. `openai` or `minimax/openai`.
    pub provider: String,
    /// Model id at that provider, e.g. `gpt-5.5`.
    pub model: String,
    /// The model id is typed rather than picked from the catalog.
    pub custom_model: bool,
    /// Narrows the model dropdown.
    model_filter: String,
    /// `provider`'s catalog models, computed when it changes.
    models: Option<(String, Vec<ModelChoice>)>,
    pub api_key: String,
    pub connected: Option<String>,
    pub busy: bool,
    pub input: String,
    pub entries: Vec<ChatEntry>,
}

impl AgentPanelState {
    /// Preselects a `provider:model` reference, e.g. from `JSON_DIFF_MODEL`.
    pub fn select(&mut self, reference: &str) {
        let (provider, model) = split_reference(reference);
        self.custom_model = !model.is_empty()
            && !catalog_models(&provider)
                .iter()
                .any(|choice| choice.id == model);
        self.provider = provider;
        self.model = model;
    }

    /// The `provider:model` reference to connect to, once both are chosen.
    pub fn reference(&self) -> Option<String> {
        let model = self.model.trim();
        (!self.provider.is_empty() && !model.is_empty())
            .then(|| format!("{}:{model}", self.provider))
    }

    pub fn connect(&mut self, agent: &AgentHandle) {
        let Some(model) = self.reference() else {
            return;
        };
        let key = self.api_key.trim();
        agent.send(AgentRequest::Connect {
            model,
            api_key: (!key.is_empty()).then(|| key.to_owned()),
        });
    }

    fn models(&mut self) -> &[ModelChoice] {
        if self
            .models
            .as_ref()
            .is_none_or(|(provider, _)| *provider != self.provider)
        {
            self.models = Some((self.provider.clone(), catalog_models(&self.provider)));
        }
        &self.models.as_ref().expect("just filled").1
    }
}

pub fn agent_panel(
    ui: &mut Ui,
    panel: &mut AgentPanelState,
    agent: &AgentHandle,
    state: &AppState,
    picker: &mut FilePicker,
    commands: &mut Vec<Command>,
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
        model_settings(ui, panel);
        let env = api_key_env(&panel.provider);
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
                panel.reference().is_some() && !panel.busy,
                egui::Button::new("Connect"),
            )
            .clicked()
        {
            panel.connect(agent);
        }
    });
    instructions(ui, state, picker, commands);
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
            agent.send(AgentRequest::Prompt {
                text: prompt,
                instructions: state.agents_md.prompt_section(),
            });
        }
    });
}

/// The AGENTS.md files the assistant follows: those found next to the
/// opened folders and files, and any the user added. Each can be switched
/// off; added ones can be removed.
fn instructions(
    ui: &mut Ui,
    state: &AppState,
    picker: &mut FilePicker,
    commands: &mut Vec<Command>,
) {
    let files = state.agents_md.files();
    let enabled = files.iter().filter(|file| file.enabled).count();
    let title = match files.len() {
        0 => "Instructions (AGENTS.md)".to_owned(),
        total if enabled == total => format!("Instructions (AGENTS.md): {total}"),
        total => format!("Instructions (AGENTS.md): {enabled} of {total}"),
    };
    egui::CollapsingHeader::new(title)
        .id_salt("agents_md")
        .show(ui, |ui| {
            if files.is_empty() {
                ui.label(
                    RichText::new(
                        "An AGENTS.md next to the opened folders or files is used \
                         automatically. You can also add one yourself.",
                    )
                    .weak(),
                );
            }
            for file in &files {
                ui.horizontal(|ui| {
                    let mut on = file.enabled;
                    let label = short_path(&file.path);
                    if ui
                        .checkbox(&mut on, label)
                        .on_hover_text(file.path.display().to_string())
                        .changed()
                    {
                        commands.push(Command::SetAgentsMdEnabled {
                            path: file.path.clone(),
                            enabled: on,
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if file.source == Source::Added
                            && ui.small_button("×").on_hover_text("Remove").clicked()
                        {
                            commands.push(Command::RemoveAgentsMd {
                                path: file.path.clone(),
                            });
                        }
                        ui.label(RichText::new(file.source.as_str()).small().weak());
                    });
                });
            }
            if ui
                .button("Add AGENTS.md…")
                .on_hover_text("Pick an instructions file for the assistant to follow")
                .clicked()
            {
                picker.open_agents_md(state);
            }
        });
}

/// `parent/AGENTS.md`: enough to tell instruction files apart.
fn short_path(path: &Path) -> String {
    let name = path.file_name().map(|name| name.to_string_lossy());
    let parent = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy());
    match (parent, name) {
        (Some(parent), Some(name)) => format!("{parent}/{name}"),
        (None, Some(name)) => name.into_owned(),
        _ => path.display().to_string(),
    }
}

/// Width of the "Provider"/"Model" label column.
const LABEL_WIDTH: f32 = 56.0;

/// Provider and model dropdowns. Models come from rig's catalog and can be
/// searched; "Other…" switches to typing an id the catalog does not list
/// (e.g. a model pulled into a local ollama).
fn model_settings(ui: &mut Ui, panel: &mut AgentPanelState) {
    let field_width = (ui.available_width() - LABEL_WIDTH - 8.0).max(80.0);

    ui.horizontal(|ui| {
        ui.add_sized([LABEL_WIDTH, 18.0], egui::Label::new("Provider"));
        egui::ComboBox::from_id_salt("provider")
            .selected_text(if panel.provider.is_empty() {
                "Choose…"
            } else {
                panel.provider.as_str()
            })
            .width(field_width)
            .height(320.0)
            .show_ui(ui, |ui| {
                for provider in known_providers() {
                    let selected = panel.provider == provider;
                    if ui.selectable_label(selected, &provider).clicked() && !selected {
                        panel.provider = provider;
                        panel.model.clear();
                        panel.custom_model = false;
                        panel.model_filter.clear();
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        ui.add_sized([LABEL_WIDTH, 18.0], egui::Label::new("Model"));
        if panel.provider.is_empty() {
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("model_disabled")
                    .selected_text("Choose a provider first")
                    .width(field_width)
                    .show_ui(ui, |_| {});
            });
            return;
        }
        let listed = !panel.models().is_empty();
        if panel.custom_model || !listed {
            let list_button = if listed { 44.0 } else { 0.0 };
            ui.add(
                egui::TextEdit::singleline(&mut panel.model)
                    .hint_text("model id, e.g. llama3.2")
                    .desired_width(field_width - list_button),
            );
            if listed
                && ui
                    .small_button("List")
                    .on_hover_text("Pick from the provider's models")
                    .clicked()
            {
                panel.custom_model = false;
                panel.model.clear();
            }
            return;
        }
        model_dropdown(ui, panel, field_width);
    });
}

/// The searchable model list: everything at first, then only the models
/// whose name or id contains every word typed (case-insensitive).
fn model_dropdown(ui: &mut Ui, panel: &mut AgentPanelState, width: f32) {
    panel.models();
    let AgentPanelState {
        models,
        model,
        model_filter,
        custom_model,
        ..
    } = panel;
    let models = &models.as_ref().expect("filled by models()").1;
    let selected = models
        .iter()
        .find(|choice| choice.id == *model)
        .map_or("Choose…", |choice| choice.name.as_str());

    egui::ComboBox::from_id_salt("model")
        .selected_text(selected)
        .width(width)
        .height(360.0)
        // Clicks inside go to the search box; picking a model closes it.
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let search = ui.add(
                egui::TextEdit::singleline(model_filter)
                    .hint_text("Search models, e.g. gpt")
                    .desired_width(f32::INFINITY),
            );
            if ui.memory(|memory| memory.focused().is_none()) {
                search.request_focus();
            }
            let query = model_filter.to_lowercase();
            let terms: Vec<&str> = query.split_whitespace().collect();
            let matches: Vec<&ModelChoice> = models
                .iter()
                .filter(|choice| {
                    let haystack = format!("{} {}", choice.name, choice.id).to_lowercase();
                    terms.iter().all(|term| haystack.contains(term))
                })
                .collect();
            let enter =
                search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

            ui.separator();
            let mut picked = enter
                .then(|| matches.first().map(|choice| choice.id.clone()))
                .flatten();
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if matches.is_empty() {
                        ui.label(RichText::new("No model matches").weak());
                    }
                    for choice in &matches {
                        let text = if choice.name == choice.id {
                            choice.id.clone()
                        } else {
                            format!("{}  ({})", choice.name, choice.id)
                        };
                        let item =
                            ui.add(egui::Button::selectable(*model == choice.id, text).truncate());
                        if item.clicked() {
                            picked = Some(choice.id.clone());
                        }
                    }
                });
            ui.separator();
            if ui
                .selectable_label(false, "Other (type a model id)…")
                .on_hover_text("For models this list does not include")
                .clicked()
            {
                *custom_model = true;
                *model = model_filter.trim().to_owned();
                model_filter.clear();
                ui.close();
            }
            if let Some(id) = picked {
                *model = id;
                model_filter.clear();
                ui.close();
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
