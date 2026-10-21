use crate::ai::{self, Action};
use crate::platform;
use crate::surfaces::{flow_bar, writing_tools};

pub fn show_flow_bar(ui: &mut egui::Ui, state: &mut flow_bar::State) {
    ui.heading("Dictation");
    ui.horizontal(|ui| {
        ui.label(format!("Status: {:?}", state.status));
        ui.add(egui::Slider::new(&mut state.level, 0.0..=1.0).text("level"));
    });
    ui.horizontal(|ui| {
        if ui.button("Start").clicked() {
            state.session += 1;
            state.status = flow_bar::Status::Listening;
            state.message = None;
        }
        if ui.button("Stop").clicked() {
            if state.transcript.trim().is_empty() {
                state.status = flow_bar::Status::Error;
                state.message = Some("Nothing captured yet.".to_string());
            } else {
                state.status = flow_bar::Status::Processing;
            }
        }
        if ui.button("Mark success").clicked() {
            state.status = flow_bar::Status::Success;
        }
        if ui.button("Cancel").clicked() {
            state.status = flow_bar::Status::Hidden;
            state.transcript.clear();
        }
    });
    ui.label("Transcript (kept in memory only, never logged):");
    ui.text_edit_multiline(&mut state.transcript);
    if let Some(msg) = &state.message {
        ui.colored_label(egui::Color32::RED, msg);
    }
    ui.small("Native speech engines plug into platform.rs; this build captures typed/pasted text.");
}

pub fn show_writing_tools(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut writing_tools::State,
    api_key: &str,
) {
    ui.heading("Writing tools");
    if ui.button("Capture system selection").clicked() {
        match platform::capture_selection() {
            Ok(sel) => {
                state.input = sel.text;
                state.can_replace = sel.can_replace;
                state.captured_app = Some(sel.app_name);
                state.error = None;
                state.mode = writing_tools::Mode::Menu;
            }
            Err(e) => {
                state.error = Some(e.to_string());
                state.mode = writing_tools::Mode::Error;
            }
        }
    }
    if let Some(app) = &state.captured_app {
        ui.small(format!("Captured from {app}"));
    }
    ui.label("Text to process:");
    ui.text_edit_multiline(&mut state.input);
    ui.horizontal_wrapped(|ui| {
        for (label, action) in Action::ALL {
            if ui.button(*label).clicked() {
                run_action(state, *action, api_key, None);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Custom instruction:");
        ui.text_edit_singleline(&mut state.custom_instruction);
        if ui.button("Run custom").clicked() {
            let custom = state.custom_instruction.clone();
            run_action(state, Action::Custom, api_key, Some(custom));
        }
    });
    match state.mode {
        writing_tools::Mode::Processing => {
            ui.spinner();
            ui.label("Working…");
        }
        writing_tools::Mode::Result | writing_tools::Mode::Chat => {
            ui.label("Result:");
            ui.text_edit_multiline(&mut state.result);
            ui.horizontal(|ui| {
                if ui.button("Copy").clicked() {
                    platform::copy_to_clipboard(ctx, &state.result);
                }
                let can_replace = state.can_replace;
                if ui
                    .add_enabled(can_replace, egui::Button::new("Replace selection"))
                    .clicked()
                {
                    match platform::replace_selection(&state.result) {
                        Ok(_) => state.error = None,
                        Err(e) => {
                            state.error = Some(e.to_string());
                            state.mode = writing_tools::Mode::Error;
                        }
                    }
                }
                if ui.button("Back").clicked() {
                    state.mode = writing_tools::Mode::Menu;
                }
            });
        }
        writing_tools::Mode::Error => {
            if let Some(err) = state.error.clone() {
                ui.colored_label(egui::Color32::RED, &err);
                if ui.button("Back").clicked() {
                    state.mode = writing_tools::Mode::Menu;
                }
            }
        }
        _ => {}
    }
}

fn run_action(
    state: &mut writing_tools::State,
    action: Action,
    api_key: &str,
    custom: Option<String>,
) {
    let prompt = match ai::prompt_for(action, &state.input, custom.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            state.error = Some(e);
            state.mode = writing_tools::Mode::Error;
            return;
        }
    };
    // One-go build: synchronous-feeling via worker thread would need channels;
    // keep it honest — blocking call here with a processing state painted first
    // is the smallest working shape. Real async comes with the tokio upgrade.
    state.mode = writing_tools::Mode::Processing;
    match ai::run_blocking(api_key, &prompt, None) {
        Ok(text) => {
            state.result = text;
            state.action = Some(action);
            // Chat answers never replace a selection; other actions keep
            // whatever the capture reported.
            state.can_replace = action.replaces_selection() && state.can_replace;
            state.mode = if action == Action::Chat {
                writing_tools::Mode::Chat
            } else {
                writing_tools::Mode::Result
            };
        }
        Err(e) => {
            state.error = Some(e);
            state.mode = writing_tools::Mode::Error;
        }
    }
}
