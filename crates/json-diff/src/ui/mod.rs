mod diff_view;
mod json_input;
use anyhow::Error;
use eframe::{
    egui::{self, TextEdit, WidgetText, WidgetWithState},
    wgpu::naga::back::hlsl::Error::Unimplemented,
};
use egui::{Color32, Ui, Widget};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use egui_json_tree::{DefaultExpand, JsonTree};
use json_diff_core::*;
use serde_json::{Value, json};
use std::marker::PhantomData;

pub struct AppContext {
    pub err: Option<Error>,
}

pub trait AppWidget: Widget {
    fn update(&mut self, ctx: &mut AppContext);
}

pub enum TabState<'a> {
    Input(InputState<'a>),
    Output(JsonState<'a>, JsonState<'a>),
}

pub enum JsonState<'a> {
    Json(&'a Value),
    Diff(WrappedValue<'a>),
}

pub struct JsonEntry<'a> {
    name: String,
    value: JsonState<'a>,
}

pub enum InputState<'a> {
    Input((JsonEntry<'a>, JsonEntry<'a>)),
    Error(Error),
}

pub struct InputTabView;

impl TabViewer for InputTabView {
    type Tab = (String, String);
    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        "New Json Comparison".into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let (old_json_string, new_json_string) = tab;
        ui.columns(2, |columns| {
            columns[0].add(TextEdit::multiline(old_json_string));
            columns[1].add(TextEdit::multiline(new_json_string));
        });
    }
}

pub struct Tabs;

impl Widget for Tabs {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        unimplemented!()
    }
}
