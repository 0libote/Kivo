use crate::platform;
use crate::settings::{Settings, Theme};
use crate::surfaces::{onboarding, settings_surface};
use crate::theme;
use egui::RichText;

pub fn show_settings(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut settings_surface::State,
    save_error: &mut Option<String>,
) {
    ui.heading("Settings");
    theme::helper(
        ui,
        "Set your shortcuts and microphone now; they activate with the OS integration.",
    );

    theme::section_label(ui, "Appearance");
    ui.horizontal(|ui| {
        theme::section_label(ui, "Theme");
        ui.selectable_value(&mut settings.theme, Theme::System, "System");
        ui.selectable_value(&mut settings.theme, Theme::Light, "Light");
        ui.selectable_value(&mut settings.theme, Theme::Dark, "Dark");
    });
    theme::helper(
        ui,
        "System follows the OS theme and keeps the native title bar in sync.",
    );

    ui.separator();
    theme::section_label(ui, "Startup");
    ui.checkbox(&mut settings.launch_at_login, "Launch at login");
    ui.checkbox(&mut settings.start_in_background, "Start in background");
    ui.checkbox(&mut settings.show_idle_flow_bar, "Show idle flow bar");

    ui.separator();
    theme::section_label(ui, "Dictation");
    ui.horizontal(|ui| {
        ui.label("Dictation shortcut:");
        ui.add(
            egui::TextEdit::singleline(&mut settings.dictation_shortcut)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Writing shortcut:");
        ui.add(
            egui::TextEdit::singleline(&mut settings.writing_shortcut)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Microphone:");
        let current = platform::microphones()
            .iter()
            .find(|(id, _)| Some(id) == settings.microphone_id.as_ref())
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "System default".to_string());
        egui::ComboBox::from_id_salt("mic")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in platform::microphones() {
                    ui.selectable_value(&mut settings.microphone_id, Some(id), name);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Language:");
        egui::ComboBox::from_id_salt("lang")
            .selected_text(settings.dictation_language.clone())
            .show_ui(ui, |ui| {
                for (code, name) in platform::speech_languages() {
                    ui.selectable_value(&mut settings.dictation_language, code, name);
                }
            });
    });
    ui.checkbox(
        &mut settings.improve_dictation_with_ai,
        "Improve dictation with AI",
    );
    ui.checkbox(&mut settings.sound_feedback, "Sound feedback");

    ui.separator();
    theme::section_label(ui, "Gemini API key");
    ui.label("Kept in memory for this session. Enter it again after restarting Kivo.");
    ui.add(
        egui::TextEdit::singleline(&mut state.api_key)
            .password(true)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        if ui.button("Clear key").clicked() {
            state.api_key.clear();
            state.status = Some("Key cleared.".to_string());
        }
        if theme::primary_button(ui, "Save settings").clicked() {
            *save_error = settings.save().err();
            if save_error.is_none() {
                state.status = Some("Settings saved.".to_string());
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Gemini model:");
        ui.add(
            egui::TextEdit::singleline(&mut settings.ai_model)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
    });
    theme::helper(
        ui,
        "The model ID stays editable so a retired default never blocks you.",
    );
    if let Some(s) = state.status.clone() {
        ui.label(s);
    }
    if let Some(e) = save_error.clone() {
        theme::error_label(ui, &e);
    }
}

pub fn show_onboarding(ui: &mut egui::Ui, settings: &mut Settings, state: &mut onboarding::State) {
    ui.heading("Welcome to Kivo");
    ui.add_space(4.0);
    let step = state.step.min(onboarding::STEPS.len() - 1);
    ui.horizontal(|ui| {
        ui.label(format!("Step {} of {}", step + 1, onboarding::STEPS.len()));
        ui.label(RichText::new(onboarding::STEPS[step]).strong());
    });
    ui.add(
        egui::ProgressBar::new((step + 1) as f32 / onboarding::STEPS.len() as f32)
            .desired_width(f32::INFINITY)
            .show_percentage(),
    );
    ui.add_space(4.0);

    match step {
        0 => {
            ui.label("Paste text into Writing tools and send it to Gemini. Dictation, shortcuts, and selection capture activate with the OS integration.");
        }
        1 => {
            ui.horizontal(|ui| {
                ui.label("Dictation:");
                ui.add(
                    egui::TextEdit::singleline(&mut settings.dictation_shortcut)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Writing:");
                ui.add(
                    egui::TextEdit::singleline(&mut settings.writing_shortcut)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            });
            theme::helper(ui, "These keys are saved and waiting.");
        }
        _ => {
            ui.label("Create a Gemini key in AI Studio and paste it under Settings.");
            ui.checkbox(
                &mut settings.onboarding_complete,
                "Mark onboarding complete",
            );
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(step > 0, egui::Button::new("Back"))
            .clicked()
        {
            state.step = step.saturating_sub(1);
        }
        let last = step + 1 >= onboarding::STEPS.len();
        if theme::primary_button(ui, if last { "Finish" } else { "Next" }).clicked() {
            if last {
                settings.onboarding_complete = true;
                let _ = settings.save();
            } else {
                state.step = step + 1;
            }
        }
    });
}
