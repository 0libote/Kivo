use crate::flow_writing;
use crate::settings::Settings;
use crate::settings_onboarding;
use crate::surfaces::{flow_bar, onboarding, settings_surface, writing_tools};

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
            Surface::FlowBar => "Flow bar",
            Surface::WritingTools => "Writing tools",
            Surface::Settings => "Settings",
            Surface::Onboarding => "Onboarding",
        }
    }
}

pub struct KivoApp {
    surface: Surface,
    settings: Settings,
    flow: flow_bar::State,
    writing: writing_tools::State,
    settings_ui: settings_surface::State,
    onboarding: onboarding::State,
    save_error: Option<String>,
}

impl KivoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        apply_theme(&settings, &cc.egui_ctx);
        let surface = if settings.onboarding_complete {
            Surface::FlowBar
        } else {
            Surface::Onboarding
        };
        Self {
            surface,
            settings,
            flow: flow_bar::State::default(),
            writing: writing_tools::State::default(),
            settings_ui: settings_surface::State::default(),
            onboarding: onboarding::State::default(),
            save_error: None,
        }
    }
}

fn apply_theme(settings: &Settings, ctx: &egui::Context) {
    use crate::settings::Theme;
    match settings.theme {
        Theme::Light => ctx.set_visuals(egui::Visuals::light()),
        Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
        Theme::System => ctx.set_visuals(egui::Visuals::default()),
    }
}

impl eframe::App for KivoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("surface-tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for surface in Surface::ALL {
                    ui.selectable_value(&mut self.surface, surface, surface.title());
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.surface {
                Surface::FlowBar => flow_writing::show_flow_bar(ui, &mut self.flow),
                Surface::WritingTools => flow_writing::show_writing_tools(
                    ui,
                    ctx,
                    &mut self.writing,
                    &self.settings_ui.api_key,
                ),
                Surface::Settings => {
                    settings_onboarding::show_settings(
                        ui,
                        &mut self.settings,
                        &mut self.settings_ui,
                        &mut self.save_error,
                    );
                    apply_theme(&self.settings, ctx);
                }
                Surface::Onboarding => {
                    settings_onboarding::show_onboarding(
                        ui,
                        &mut self.settings,
                        &mut self.onboarding,
                    );
                    apply_theme(&self.settings, ctx);
                }
            });
        });
    }
}
