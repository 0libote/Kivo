use crate::flow_writing;
use crate::settings::Settings;
use crate::settings_onboarding;
use crate::surfaces::{flow_bar, onboarding, settings_surface, writing_tools};
use crate::theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Surface {
    #[default]
    FlowBar,
    WritingTools,
    Settings,
    Onboarding,
}

impl Surface {
    pub const ALL: [Surface; 4] = [
        Surface::FlowBar,
        Surface::WritingTools,
        Surface::Settings,
        Surface::Onboarding,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Surface::FlowBar => "Dictate",
            Surface::WritingTools => "Write",
            Surface::Settings => "Settings",
            Surface::Onboarding => "Welcome",
        }
    }
}

pub struct KivoApp {
    surface: Surface,
    settings: Settings,
    applied_theme: crate::settings::Theme,
    flow: flow_bar::State,
    writing: writing_tools::State,
    settings_ui: settings_surface::State,
    onboarding: onboarding::State,
    save_error: Option<String>,
}

impl KivoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        theme::apply_theme(&settings, &cc.egui_ctx);
        let surface = if settings.onboarding_complete {
            Surface::FlowBar
        } else {
            Surface::Onboarding
        };
        let applied_theme = settings.theme;
        Self {
            surface,
            settings,
            applied_theme,
            flow: flow_bar::State::default(),
            writing: writing_tools::State::default(),
            settings_ui: settings_surface::State::default(),
            onboarding: onboarding::State::default(),
            save_error: None,
        }
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui) {
        let mut surface = self.surface;
        let model = self.settings.ai_model.clone();
        let key_set = !self.settings_ui.api_key.trim().is_empty();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Kivo").size(20.0).strong());
            ui.add_space(4.0);
            ui.separator();
            for tab in Surface::ALL {
                ui.selectable_value(&mut surface, tab, tab.title());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::mono_small(ui, &model);
                ui.label(
                    egui::RichText::new(if key_set { "Key set" } else { "No key" })
                        .small()
                        .weak(),
                );
            });
        });
        self.surface = surface;
    }

    fn show_status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Typed text stays on this computer until sent to Gemini")
                    .small()
                    .weak(),
            );
            ui.add_space(16.0);
            ui.label(egui::RichText::new("Native preview build").small().weak());
        });
    }

    fn show_surface(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // One centered working column; narrow windows take the full width.
        let outer = ui.available_width();
        let width = outer.min(theme::MAX_CONTENT_WIDTH);
        ui.horizontal(|ui| {
            let gutter = (outer - width).max(0.0) / 2.0;
            if gutter > 1.0 {
                ui.add_space(gutter);
            }
            ui.vertical(|ui| {
                ui.set_max_width(width);
                ui.add_space(8.0);
                match self.surface {
                    Surface::FlowBar => flow_writing::show_flow_bar(ui, &mut self.flow),
                    Surface::WritingTools => flow_writing::show_writing_tools(
                        ui,
                        ctx,
                        &mut self.writing,
                        &self.settings_ui.api_key,
                        &self.settings.ai_model,
                    ),
                    Surface::Settings => {
                        settings_onboarding::show_settings(
                            ui,
                            &mut self.settings,
                            &mut self.settings_ui,
                            &mut self.save_error,
                        );
                    }
                    Surface::Onboarding => {
                        settings_onboarding::show_onboarding(
                            ui,
                            &mut self.settings,
                            &mut self.onboarding,
                        );
                    }
                }
                ui.add_space(16.0);
            });
        });
    }
}

impl eframe::App for KivoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::apply_if_changed(&self.settings, &ctx, &mut self.applied_theme);
        egui::Panel::top("kivo-topbar").show(ui, |ui| {
            self.show_top_bar(ui);
        });
        egui::Panel::bottom("kivo-status").show(ui, |ui| {
            self.show_status_bar(ui);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| self.show_surface(ui, &ctx));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::App;

    #[test]
    fn every_surface_renders_its_content() {
        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();
        let mut app = KivoApp {
            surface: Surface::FlowBar,
            settings: Settings::default(),
            applied_theme: Settings::default().theme,
            flow: flow_bar::State::default(),
            writing: writing_tools::State::default(),
            settings_ui: settings_surface::State::default(),
            onboarding: onboarding::State::default(),
            save_error: None,
        };
        for (surface, content) in [
            (Surface::FlowBar, "Dictation"),
            (Surface::WritingTools, "Text to process:"),
            (Surface::Settings, "Gemini API key"),
            (Surface::Onboarding, "Welcome to Kivo"),
        ] {
            app.surface = surface;
            let output = ctx.run_ui(egui::RawInput::default(), |ui| app.ui(ui, &mut frame));
            let has_content = output.shapes.iter().any(|shape| {
                matches!(
                    &shape.shape,
                    egui::epaint::Shape::Text(text) if text.galley.text() == content
                )
            });
            output.drop_without_applying_deltas();
            assert!(has_content, "Missing content for {surface:?}");
        }
    }

    #[test]
    fn shell_names_each_destination_once() {
        let titles: Vec<_> = Surface::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(titles, vec!["Dictate", "Write", "Settings", "Welcome"]);
    }
}
