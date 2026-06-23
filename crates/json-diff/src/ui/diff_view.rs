use eframe::egui::{self, Ui, Widget};

pub struct CodeEditor<'a> {
    language: String,
    code: &'a mut String,
}

impl<'a> CodeEditor<'a> {
    pub fn new(code: &'a mut String) -> Self {
        let language = "json".into();
        Self { code, language }
    }
}

impl<'a> Widget for CodeEditor<'a> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let Self { mut language, code } = self;

        let mut theme =
            egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
        ui.collapsing("Theme", |ui| {
            ui.group(|ui| {
                theme.ui(ui);
                theme.clone().store_in_memory(ui.ctx());
            });
        });

        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut layout_job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(),
                ui.style(),
                &theme,
                buf.as_str(),
                &mut language,
            );
            layout_job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(layout_job))
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            let editor = egui::TextEdit::multiline(code)
                .font(egui::TextStyle::Monospace) // for cursor height
                .code_editor()
                .desired_rows(10)
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter);

            ui.add(editor);
        });

        ui.response()
    }
}
