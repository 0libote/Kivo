use crate::ai::{self, Action};
use crate::platform;
use crate::surfaces::{flow_bar, writing_tools};
use crate::theme;
use egui::RichText;

fn flow_status_text(status: &flow_bar::Status) -> &'static str {
    match status {
        flow_bar::Status::Hidden => "Hidden",
        flow_bar::Status::Idle => "Ready",
        flow_bar::Status::Listening => "Listening",
        flow_bar::Status::Processing => "Working",
        flow_bar::Status::Success => "Done",
        flow_bar::Status::Error => "Needs attention",
    }
}

pub fn show_flow_bar(ui: &mut egui::Ui, state: &mut flow_bar::State) {
    ui.heading("Dictation");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        theme::section_label(ui, "Status");
        ui.label(RichText::new(flow_status_text(&state.status)).strong());
        ui.add_space(16.0);
        ui.label(RichText::new("Input level").small().weak());
        ui.add(
            egui::ProgressBar::new(state.level.clamp(0.0, 1.0))
                .desired_width(140.0)
                .show_percentage(),
        );
    });
    ui.add_space(4.0);

    let listening = state.status == flow_bar::Status::Listening;
    ui.horizontal(|ui| {
        if listening {
            if theme::primary_button(ui, "Stop and review").clicked() {
                if state.transcript.trim().is_empty() {
                    state.status = flow_bar::Status::Error;
                    state.message = Some("Nothing captured yet.".to_string());
                } else {
                    state.status = flow_bar::Status::Processing;
                }
            }
        } else if theme::primary_button(ui, "Start listening").clicked() {
            state.session += 1;
            state.status = flow_bar::Status::Listening;
            state.message = None;
        }
        if ui.button("Mark done").clicked() {
            state.status = flow_bar::Status::Success;
        }
        let can_clear = !state.transcript.is_empty() || state.status != flow_bar::Status::Hidden;
        if ui
            .add_enabled(can_clear, egui::Button::new("Clear transcript"))
            .clicked()
        {
            state.status = flow_bar::Status::Hidden;
            state.transcript.clear();
        }
    });
    ui.add_space(4.0);

    if state.status == flow_bar::Status::Idle && state.transcript.trim().is_empty() {
        theme::helper(
            ui,
            "Nothing here yet. Start listening, then type or paste what you would have said.",
        );
    }
    ui.label("Transcript (kept in memory only, never logged):");
    ui.add(
        egui::TextEdit::multiline(&mut state.transcript)
            .hint_text("Type or paste what you would have said…")
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    theme::mono_small(
        ui,
        &format!("{} characters", state.transcript.chars().count()),
    );
    if state.status == flow_bar::Status::Success {
        theme::success_label(ui, "Marked done. The transcript stays in memory.");
    }
    if let Some(msg) = state.message.clone() {
        theme::error_label(ui, &msg);
        if state.status == flow_bar::Status::Error {
            theme::helper(
                ui,
                "Add some text above, then choose Stop and review again.",
            );
        }
    }
    theme::helper(
        ui,
        "Microphone capture arrives with the OS integration; typing works now.",
    );
}

const RESHAPE_ACTIONS: [(&str, Action); 5] = [
    ("Proofread", Action::Proofread),
    ("Rewrite", Action::Rewrite),
    ("Friendly", Action::Friendly),
    ("Professional", Action::Professional),
    ("Concise", Action::Concise),
];

const EXTRACT_ACTIONS: [(&str, Action); 2] = [
    ("Summarize", Action::Summarize),
    ("Key points", Action::KeyPoints),
];

pub fn show_writing_tools(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut writing_tools::State,
    api_key: &str,
    model: &str,
) {
    poll_action(state);
    ui.heading("Writing tools");
    ui.add_space(4.0);

    let busy = state.pending.is_some();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy, egui::Button::new("Capture system selection"))
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
        if busy {
            ui.spinner();
        }
    });
    if let Some(app) = &state.captured_app {
        theme::helper(ui, &format!("Captured from {app}"));
    } else {
        theme::helper(ui, "Paste text below, or capture the current selection.");
    }
    ui.add_space(4.0);

    ui.label("Text to process:");
    ui.add(
        egui::TextEdit::multiline(&mut state.input)
            .hint_text("Paste the text to process…")
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    theme::mono_small(ui, &format!("{} characters", state.input.chars().count()));
    ui.add_space(4.0);

    theme::section_label(ui, "Fix and reshape");
    ui.horizontal_wrapped(|ui| {
        for (label, action) in RESHAPE_ACTIONS {
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                run_action(state, action, api_key, model, None, ctx);
            }
        }
    });
    theme::section_label(ui, "Extract");
    ui.horizontal_wrapped(|ui| {
        for (label, action) in EXTRACT_ACTIONS {
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                run_action(state, action, api_key, model, None, ctx);
            }
        }
    });
    theme::section_label(ui, "Ask");
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!busy, egui::Button::new("Quick chat"))
            .clicked()
        {
            run_action(state, Action::Chat, api_key, model, None, ctx);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Custom instruction:");
        let row_spacing = ui.spacing().item_spacing.x;
        let button_width = 110.0;
        let edit_width = (ui.available_width() - button_width - row_spacing).max(80.0);
        ui.add_sized(
            [edit_width, theme::CONTROL_HEIGHT],
            egui::TextEdit::singleline(&mut state.custom_instruction)
                .hint_text("For example: turn into a checklist"),
        );
        if ui
            .add_sized(
                [button_width, theme::CONTROL_HEIGHT],
                egui::Button::new("Run custom"),
            )
            .clicked()
        {
            let custom = state.custom_instruction.clone();
            run_action(state, Action::Custom, api_key, model, Some(custom), ctx);
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    match state.mode {
        writing_tools::Mode::Processing => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("Working with {model}…"));
            });
            theme::helper(
                ui,
                "The request runs in the background. The actions stay disabled until it finishes.",
            );
        }
        writing_tools::Mode::Result | writing_tools::Mode::Chat => {
            ui.label("Result:");
            ui.add(
                egui::TextEdit::multiline(&mut state.result)
                    .desired_rows(7)
                    .desired_width(f32::INFINITY),
            );
            theme::mono_small(ui, &format!("{} characters", state.result.chars().count()));
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Copy").clicked() {
                    platform::copy_to_clipboard(ctx, &state.result);
                }
                let can_replace =
                    state.can_replace && state.action.is_some_and(Action::replaces_selection);
                let replace = ui.add_enabled(can_replace, egui::Button::new("Replace selection"));
                let replace = replace.on_disabled_hover_text(
                    "Replacement needs a captured selection from an action that rewrites text.",
                );
                if replace.clicked() {
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
                theme::error_label(ui, &err);
                theme::helper(
                    ui,
                    "Check the text, key, model, and connection, then go back and try again.",
                );
                if ui.button("Back").clicked() {
                    state.mode = writing_tools::Mode::Menu;
                }
            }
        }
        writing_tools::Mode::Menu => {
            if state.input.trim().is_empty() {
                theme::helper(
                    ui,
                    "The result lands here. Paste text above and choose an action to begin.",
                );
            }
        }
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
