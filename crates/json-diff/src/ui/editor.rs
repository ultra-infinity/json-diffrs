use eframe::egui::{self, Ui, Widget};

/// A syntax-highlighted multiline JSON editor.
pub struct CodeEditor<'a> {
    code: &'a mut String,
    id_salt: &'a str,
}

impl<'a> CodeEditor<'a> {
    pub fn new(code: &'a mut String, id_salt: &'a str) -> Self {
        Self { code, id_salt }
    }
}

impl Widget for CodeEditor<'_> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let Self { code, id_salt } = self;
        let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());

        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut layout_job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(),
                ui.style(),
                &theme,
                buf.as_str(),
                "json",
            );
            layout_job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(layout_job))
        };

        egui::ScrollArea::both()
            .id_salt(id_salt)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(code)
                        .id_salt(id_salt)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(30)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter),
                )
            })
            .inner
    }
}
