use crate::keystore;
use crate::platform;
use crate::settings::{Settings, Theme};
use crate::surfaces::{onboarding, settings_surface};
use crate::theme;
use egui::RichText;

pub fn validate_shortcuts(dictation: &str, writing: &str) -> Result<(), String> {
    let dictation = dictation.trim();
    let writing = writing.trim();
    if dictation.is_empty() {
        return Err("Enter a dictation shortcut.".to_string());
    }
    if writing.is_empty() {
        return Err("Enter a writing shortcut.".to_string());
    }
    if dictation.chars().count() > 64 || writing.chars().count() > 64 {
        return Err("Shortcuts must be 64 characters or fewer.".to_string());
    }
    if dictation.chars().any(char::is_control) || writing.chars().any(char::is_control) {
        return Err("Shortcuts contain an invalid character.".to_string());
    }
    if dictation == writing {
        return Err("Dictation and writing shortcuts must be different.".to_string());
    }
    Ok(())
}

pub fn validate_settings(settings: &Settings) -> Result<(), String> {
    validate_shortcuts(&settings.dictation_shortcut, &settings.writing_shortcut)?;
    let model = settings.ai_model.trim();
    if model.is_empty() {
        return Err("Set a Gemini model in Settings first.".to_string());
    }
    if model.chars().count() > 128 {
        return Err("Model must be 128 characters or fewer.".to_string());
    }
    if !crate::ai::is_usable_model(model) {
        return Err("That model ID looks invalid. Pick one from the list.".to_string());
    }
    Ok(())
}

/// Background key test channel: the UI polls this each frame.
pub fn start_key_test(state: &mut settings_surface::State, key: String, ctx: &egui::Context) {
    if state.testing {
        return;
    }
    state.testing = true;
    state.test_result = None;
    let (tx, rx) = std::sync::mpsc::channel();
    // Store the receiver as an opaque pending flag via test_result None + testing.
    // We stash the receiver in a thread-local slot keyed by pointer: simpler is
    // to leak it into the state through a one-shot poll using a background thread
    // that sends back through a channel we poll in `poll_key_test`.
    state.test_pending = Some(rx);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result =
            crate::ai::test_connection(&key).map(|n| format!("Key works — {n} models available."));
        let _ = tx.send(result);
        ctx.request_repaint();
    });
}

pub fn poll_key_test(state: &mut settings_surface::State) {
    if !state.testing {
        return;
    }
    if let Some(rx) = state.test_pending.as_ref() {
        match rx.try_recv() {
            Ok(result) => {
                state.test_result = Some(result);
                state.testing = false;
                state.test_pending = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.test_result = Some(Err("The key test worker stopped.".to_string()));
                state.testing = false;
                state.test_pending = None;
            }
        }
    }
}

pub fn show_settings(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut settings_surface::State,
    save_error: &mut Option<String>,
) {
    poll_key_test(state);
    let ctx = ui.ctx().clone();
    ui.heading("Settings");
    theme::helper(
        ui,
        "Shortcuts and microphone wait for the OS integration; key, model, and theme work now.",
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
    theme::helper(
        ui,
        "Login + background need the OS integration; the toggles are saved now.",
    );

    ui.separator();
    theme::section_label(ui, "Dictation");
    ui.horizontal(|ui| {
        ui.label("Dictation shortcut:");
        ui.add(
            egui::TextEdit::singleline(&mut settings.dictation_shortcut)
                .font(egui::TextStyle::Monospace)
                .hint_text("Ctrl+Meta")
                .desired_width(f32::INFINITY),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Writing shortcut:");
        ui.add(
            egui::TextEdit::singleline(&mut settings.writing_shortcut)
                .font(egui::TextStyle::Monospace)
                .hint_text("Ctrl+Space")
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
                ui.selectable_value(&mut settings.microphone_id, None, "System default");
                for (id, name) in platform::microphones() {
                    if id != "default" {
                        ui.selectable_value(&mut settings.microphone_id, Some(id), name);
                    }
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
    ui.label("Stored in the OS vault (Keychain / Credential Manager / Secret Service).");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.api_key)
                .password(!state.show_key)
                .hint_text("AIza…")
                .desired_width(f32::INFINITY),
        );
        ui.checkbox(&mut state.show_key, "Show");
    });
    ui.horizontal(|ui| {
        if ui.button("Clear key").clicked() {
            state.api_key.clear();
            let _ = keystore::clear_api_key(keystore::GEMINI_ACCOUNT);
            state.status = Some("Key cleared.".to_string());
            state.test_result = None;
        }
        let testing = state.testing;
        if ui
            .add_enabled(
                !testing && !state.api_key.trim().is_empty(),
                egui::Button::new("Test connection"),
            )
            .on_hover_text("Calls Gemini ListModels with this key.")
            .clicked()
        {
            start_key_test(state, state.api_key.clone(), &ctx);
        }
        if testing {
            ui.spinner();
        }
        if theme::primary_button(ui, "Save settings").clicked() {
            settings.dictation_shortcut = settings.dictation_shortcut.trim().to_owned();
            settings.writing_shortcut = settings.writing_shortcut.trim().to_owned();
            settings.ai_model = settings.ai_model.trim().to_owned();
            match validate_settings(settings) {
                Err(e) => *save_error = Some(e),
                Ok(()) => {
                    let key_result = if state.api_key.trim().is_empty() {
                        let _ = keystore::clear_api_key(keystore::GEMINI_ACCOUNT);
                        Ok(())
                    } else {
                        keystore::save_api_key(keystore::GEMINI_ACCOUNT, &state.api_key)
                    };
                    match key_result {
                        Err(e) => *save_error = Some(e.to_string()),
                        Ok(()) => match settings.save() {
                            Err(e) => *save_error = Some(e),
                            Ok(()) => {
                                *save_error = None;
                                state.status = Some("Settings saved.".to_string());
                            }
                        },
                    }
                }
            }
        }
    });
    if let Some(Ok(msg)) = state.test_result.clone() {
        theme::success_label(ui, &msg);
    } else if let Some(Err(err)) = state.test_result.clone() {
        theme::error_label(ui, &err);
    }
    ui.horizontal(|ui| {
        ui.label("Gemini model:");
        egui::ComboBox::from_id_salt("model")
            .selected_text(settings.ai_model.clone())
            .show_ui(ui, |ui| {
                for m in crate::ai::SUPPORTED_MODELS {
                    ui.selectable_value(&mut settings.ai_model, m.id.to_string(), m.label)
                        .on_hover_text(m.description);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut settings.ai_model)
                .font(egui::TextStyle::Monospace)
                .hint_text("gemini-3.8-flash")
                .desired_width(f32::INFINITY),
        );
    });
    theme::helper(
        ui,
        "Pick from the list or paste a newer ID — retired defaults never block you.",
    );
    if let Some(s) = state.status.clone() {
        ui.label(s);
    }
    if let Some(e) = save_error.clone() {
        theme::error_label(ui, &e);
    }
}

pub fn show_onboarding(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut onboarding::State,
    settings_ui: &mut settings_surface::State,
    save_error: &mut Option<String>,
) {
    poll_key_test(settings_ui);
    let ctx = ui.ctx().clone();
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
            ui.add_space(4.0);
            theme::helper(
                ui,
                "Shortcuts: Ctrl+1 Dictate · Ctrl+2 Write · Ctrl+3 Settings.",
            );
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
            theme::helper(
                ui,
                "These keys are saved and waiting for the OS integration.",
            );
        }
        _ => {
            ui.label(
                "Create a Gemini key in AI Studio and paste it here. It is stored in the OS vault.",
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut settings_ui.api_key)
                        .password(!settings_ui.show_key)
                        .hint_text("AIza…")
                        .desired_width(f32::INFINITY),
                );
                ui.checkbox(&mut settings_ui.show_key, "Show");
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !settings_ui.testing && !settings_ui.api_key.trim().is_empty(),
                        egui::Button::new("Test connection"),
                    )
                    .clicked()
                {
                    start_key_test(settings_ui, settings_ui.api_key.clone(), &ctx);
                }
                if settings_ui.testing {
                    ui.spinner();
                }
            });
            if let Some(Ok(msg)) = settings_ui.test_result.clone() {
                theme::success_label(ui, &msg);
            } else if let Some(Err(err)) = settings_ui.test_result.clone() {
                theme::error_label(ui, &err);
            }
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
            *save_error = None;
        }
        let last = step + 1 >= onboarding::STEPS.len();
        if theme::primary_button(ui, if last { "Finish" } else { "Next" }).clicked() {
            if last {
                settings.dictation_shortcut = settings.dictation_shortcut.trim().to_owned();
                settings.writing_shortcut = settings.writing_shortcut.trim().to_owned();
                settings.ai_model = settings.ai_model.trim().to_owned();
                match validate_settings(settings) {
                    Err(e) => *save_error = Some(e),
                    Ok(()) => {
                        let key_result = if settings_ui.api_key.trim().is_empty() {
                            Ok(())
                        } else {
                            keystore::save_api_key(keystore::GEMINI_ACCOUNT, &settings_ui.api_key)
                        };
                        match key_result {
                            Err(e) => *save_error = Some(e.to_string()),
                            Ok(()) => {
                                settings.onboarding_complete = true;
                                match settings.save() {
                                    Ok(()) => *save_error = None,
                                    Err(e) => {
                                        settings.onboarding_complete = false;
                                        *save_error = Some(e);
                                    }
                                }
                            }
                        }
                    }
                }
            } else if step == 1 {
                settings.dictation_shortcut = settings.dictation_shortcut.trim().to_owned();
                settings.writing_shortcut = settings.writing_shortcut.trim().to_owned();
                match validate_shortcuts(&settings.dictation_shortcut, &settings.writing_shortcut) {
                    Err(e) => *save_error = Some(e),
                    Ok(()) => {
                        *save_error = None;
                        state.step = step + 1;
                    }
                }
            } else {
                *save_error = None;
                state.step = step + 1;
            }
        }
    });
    if let Some(e) = save_error.clone() {
        theme::error_label(ui, &e);
    }
}
