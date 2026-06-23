mod diff_view;
mod json_input;
use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, RwLock},
};

use anyhow::Error;
pub use diff_view::CodeEditor;
use eframe::egui::{self, Key::Colon, TextEdit, WidgetText};
use egui::{Ui, Widget};
use egui_dock::{DockArea, DockState, TabViewer};
use json_diff_core::*;
use serde_json::{Value, json};

pub struct Entry<S> {
    pub old: S,
    pub new: S,
}

#[derive(Default)]
pub struct AppContext {
    pub input_strings: HashMap<String, Entry<String>>,
    pub json_values: HashMap<String, Entry<Value>>,
}

#[derive(Clone, Default)]
pub struct Ctx(Arc<RwLock<AppContext>>);

impl Ctx {
    pub fn new() -> Self {
        Ctx(Arc::new(RwLock::new(AppContext::default())))
    }
}

impl Deref for Ctx {
    type Target = Arc<RwLock<AppContext>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Ctx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub enum InputState<'a> {
    Input(&'a mut String, &'a mut String),
    Values(&'a Value, &'a Value),
}

pub struct MainView<'a> {
    dock_state: DockState<InputState<'a>>,
}

impl<'a> MainView<'a> {
    pub fn new(input_strings: &'a mut HashMap<String, Entry<String>>) -> Self {
        let dock_state = {
            let tabs = input_strings
                .iter_mut()
                .map(|(old, entry)| InputState::Input(&mut entry.old, &mut entry.new))
                .collect();
            let dock_state = DockState::new(tabs);

            dock_state
        };
        Self { dock_state }
    }
}

impl<'a> Widget for MainView<'a> {
    fn ui(mut self, ui: &mut Ui) -> egui::Response {
        let mut tab_viewer = JsonTabViewer(PhantomData);
        DockArea::new(&mut self.dock_state).show_inside(ui, &mut tab_viewer);
        ui.response()
    }
}

struct JsonTabViewer<'a>(PhantomData<&'a ()>);

impl<'a> TabViewer for JsonTabViewer<'a> {
    type Tab = InputState<'a>;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        match tab {
            InputState::Input(_, _) => "New Comparision".into(),
            InputState::Values(_, _) => "Diff".into(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            InputState::Input(old, new) => {
                ui.columns(2, |columns| {
                    columns[0].add(CodeEditor::new(old));
                    columns[1].add(CodeEditor::new(new));
                });
            }
            _ => {}
        }
    }
}
