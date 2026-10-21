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
    model: &str,
) {
    poll_action(state);
    ui.heading("Writing tools");
    if ui
        .add_enabled(
            state.pending.is_none(),
            egui::Button::new("Capture system selection"),
        )
        .clicked()
    {
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
            if ui
                .add_enabled(state.pending.is_none(), egui::Button::new(*label))
                .clicked()
            {
                run_action(state, *action, api_key, model, None, ctx);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Custom instruction:");
        ui.text_edit_singleline(&mut state.custom_instruction);
        if ui
            .add_enabled(state.pending.is_none(), egui::Button::new("Run custom"))
            .clicked()
        {
            let custom = state.custom_instruction.clone();
            run_action(state, Action::Custom, api_key, model, Some(custom), ctx);
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
                let can_replace =
                    state.can_replace && state.action.is_some_and(Action::replaces_selection);
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
    model: &str,
    custom: Option<String>,
    ctx: &egui::Context,
) {
    if state.pending.is_some() {
        return;
    }
    let prompt = match ai::prompt_for(action, &state.input, custom.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            state.error = Some(e);
            state.mode = writing_tools::Mode::Error;
            return;
        }
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    state.pending = Some(receiver);
    state.action = Some(action);
    state.error = None;
    state.mode = writing_tools::Mode::Processing;
    let key = api_key.to_owned();
    let model = model.to_owned();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = sender.send(ai::run_blocking(&key, &model, &prompt));
        ctx.request_repaint();
    });
}

fn poll_action(state: &mut writing_tools::State) {
    use std::sync::mpsc::TryRecvError;
    let result = match state.pending.as_ref().map(|pending| pending.try_recv()) {
        None | Some(Err(TryRecvError::Empty)) => return,
        Some(Err(TryRecvError::Disconnected)) => {
            Err("The AI worker stopped. Try again.".to_string())
        }
        Some(Ok(result)) => result,
    };
    state.pending = None;
    match result {
        Ok(text) => {
            state.result = text;
            state.mode = if state.action == Some(Action::Chat) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_request_does_not_block_and_keeps_selection_capability() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut state = writing_tools::State {
            pending: Some(receiver),
            action: Some(Action::Summarize),
            mode: writing_tools::Mode::Processing,
            can_replace: true,
            ..Default::default()
        };
        poll_action(&mut state);
        assert_eq!(state.mode, writing_tools::Mode::Processing);
        assert!(state.pending.is_some());
        sender.send(Ok("Summary".to_owned())).unwrap();
        poll_action(&mut state);
        assert_eq!(state.result, "Summary");
        assert_eq!(state.mode, writing_tools::Mode::Result);
        assert!(state.pending.is_none());
        assert!(state.can_replace);
        assert!(!state.action.unwrap().replaces_selection());
    }

    #[test]
    fn failed_and_disconnected_workers_leave_processing_state() {
        for result in [Some(Err("Quota exhausted".to_owned())), None] {
            let (sender, receiver) = std::sync::mpsc::channel();
            let mut state = writing_tools::State {
                pending: Some(receiver),
                mode: writing_tools::Mode::Processing,
                ..Default::default()
            };
            if let Some(result) = result {
                sender.send(result).unwrap();
            }
            drop(sender);
            poll_action(&mut state);
            assert_eq!(state.mode, writing_tools::Mode::Error);
            assert!(state.pending.is_none());
            assert!(state.error.is_some());
        }
    }
}
