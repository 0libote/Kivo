use crate::settings::{Settings, Theme};
use crate::surfaces::{onboarding, settings_surface};

pub fn show_settings(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut settings_surface::State,
    save_error: &mut Option<String>,
) {
    ui.heading("Settings");
    ui.horizontal(|ui| {
        ui.label("Theme:");
        ui.selectable_value(&mut settings.theme, Theme::System, "System");
        ui.selectable_value(&mut settings.theme, Theme::Light, "Light");
        ui.selectable_value(&mut settings.theme, Theme::Dark, "Dark");
    });
    ui.checkbox(&mut settings.launch_at_login, "Launch at login");
    ui.checkbox(&mut settings.start_in_background, "Start in background");
    ui.checkbox(&mut settings.show_idle_flow_bar, "Show idle flow bar");
    ui.checkbox(&mut settings.improve_dictation_with_ai, "Improve dictation with AI");
    ui.checkbox(&mut settings.sound_feedback, "Sound feedback");
    ui.horizontal(|ui| {
        ui.label("Dictation shortcut:");
        ui.text_edit_singleline(&mut settings.dictation_shortcut);
    });
    ui.horizontal(|ui| {
        ui.label("Writing shortcut:");
        ui.text_edit_singleline(&mut settings.writing_shortcut);
    });
    ui.horizontal(|ui| {
        ui.label("Language:");
        ui.text_edit_singleline(&mut settings.dictation_language);
    });
    ui.separator();
    ui.heading("Gemini API key");
    ui.label("Stored for this native build in the OS config dir; never logged.");
    ui.add(egui::TextEdit::singleline(&mut state.api_key).password(true));
    ui.horizontal(|ui| {
        if ui.button("Save key").clicked() {
            if state.api_key.trim().len() > 8 {
                settings.api_key_configured = true;
                state.status = Some("Key saved.".to_string());
            } else {
                state.status = Some("Key looks too short.".to_string());
            }
        }
        if ui.button("Clear key").clicked() {
            state.api_key.clear();
            settings.api_key_configured = false;
            state.status = Some("Key cleared.".to_string());
        }
        if ui.button("Save settings").clicked() {
            *save_error = settings.save().err();
            if save_error.is_none() {
                state.status = Some("Settings saved.".to_string());
            }
        }
    });
    if let Some(s) = &state.status {
        ui.label(s);
    }
    if let Some(e) = &save_error {
        ui.colored_label(egui::Color32::RED, e);
    }
}

pub fn show_onboarding(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut onboarding::State,
) {
    ui.heading("Welcome to Kivo");
    ui.label(format!("Step {} of {}", state.step + 1, onboarding::STEPS.len()));
    ui.label(onboarding::STEPS[state.step.min(onboarding::STEPS.len() - 1)]);
    match state.step {
        0 => {
            ui.label("Quiet, system-wide dictation and writing help. No account, no telemetry.");
        }
        1 => {
            ui.label("Grant Accessibility, Input Monitoring, Microphone, Speech Recognition in OS Settings when asked.");
        }
        2 => {
            ui.horizontal(|ui| {
                ui.label("Dictation:");
                ui.text_edit_singleline(&mut settings.dictation_shortcut);
            });
            ui.horizontal(|ui| {
                ui.label("Writing:");
                ui.text_edit_singleline(&mut settings.writing_shortcut);
            });
        }
        _ => {
            ui.label("Create a Gemini key in AI Studio and paste it under Settings.");
            ui.checkbox(&mut settings.onboarding_complete, "Mark onboarding complete");
        }
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(state.step > 0, egui::Button::new("Back")).clicked() {
            state.step -= 1;
        }
        if ui.button(if state.step + 1 >= onboarding::STEPS.len() { "Finish" } else { "Next" }).clicked() {
            if state.step + 1 >= onboarding::STEPS.len() {
                settings.onboarding_complete = true;
                let _ = settings.save();
            } else {
                state.step += 1;
            }
        }
    });
}
