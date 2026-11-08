use crate::ai::{self, Action};
use crate::app::Surface;
use crate::platform;
use crate::surfaces::{flow_bar, writing_tools};
use crate::theme;
use egui::RichText;

fn flow_status_text(status: &flow_bar::Status) -> &'static str {
    match status {
        flow_bar::Status::Hidden => "Hidden",
        flow_bar::Status::Idle => "Ready",
        flow_bar::Status::Listening => "Listening",
        flow_bar::Status::Success => "Done",
        flow_bar::Status::Error => "Needs attention",
    }
}

fn cached_chars(cached_bytes: &mut usize, cached_count: &mut usize, text: &str) -> usize {
    if *cached_bytes != text.len() {
        *cached_bytes = text.len();
        *cached_count = text.chars().count();
    }
    *cached_count
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn count_label(chars: usize, words: usize) -> String {
    let c = if chars == 1 {
        "character"
    } else {
        "characters"
    };
    let w = if words == 1 { "word" } else { "words" };
    format!("{chars} {c} · {words} {w}")
}

fn elapsed_label(since: Option<std::time::Instant>) -> String {
    let secs = since.map(|t| t.elapsed().as_secs()).unwrap_or(0);
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn show_flow_bar(ui: &mut egui::Ui, state: &mut flow_bar::State) {
    ui.label(RichText::new("Dictation").size(26.0).strong());
    ui.add_space(2.0);
    theme::helper(
        ui,
        "Speak, then review the transcript. Nothing leaves this computer.",
    );
    ui.add_space(10.0);

    let listening = state.status == flow_bar::Status::Listening;
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            if listening {
                // Gentle pulse while the (forthcoming) microphone is live.
                let pulse = (ui.ctx().time() * 3.0).sin() * 0.5 + 0.5;
                let glow = theme::ACCENT.lerp_to_gamma(egui::Color32::WHITE, (pulse * 0.35) as f32);
                theme::status_dot(ui, glow, 13.0);
                theme::pill(ui, "Listening", theme::ACCENT_SOFT, theme::INK);
            } else {
                let (fill, ink, dot) = match state.status {
                    flow_bar::Status::Success => (theme::ACCENT_SOFT, theme::INK, theme::ACCENT),
                    flow_bar::Status::Error => (
                        egui::Color32::from_rgb(250, 226, 224),
                        theme::ERROR,
                        theme::ERROR,
                    ),
                    _ => (
                        ui.visuals().widgets.inactive.weak_bg_fill,
                        ui.visuals().text_color(),
                        theme::WARN,
                    ),
                };
                theme::status_dot(ui, dot, 11.0);
                theme::pill(ui, flow_status_text(&state.status), fill, ink);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::mono_small(ui, &elapsed_label(state.listening_since));
            });
        });
        ui.add_space(10.0);
        if listening {
            if theme::primary_button_wide(ui, "Stop and review").clicked() {
                state.listening_since = None;
                if state.transcript.trim().is_empty() {
                    state.status = flow_bar::Status::Error;
                    state.message = Some("Nothing captured yet.".to_string());
                } else {
                    state.status = flow_bar::Status::Idle;
                    state.message = Some("Dictation preview: typed text kept.".to_string());
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Mark done").clicked() {
                    state.status = flow_bar::Status::Success;
                    state.listening_since = None;
                    state.message = None;
                }
                if ui.button("Discard").clicked() {
                    state.status = flow_bar::Status::Hidden;
                    state.transcript.clear();
                    state.listening_since = None;
                    state.message = None;
                }
            });
        } else {
            if theme::primary_button_wide(ui, "Start listening").clicked() {
                state.status = flow_bar::Status::Listening;
                state.listening_since = Some(std::time::Instant::now());
                state.message = None;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Mark done").clicked() {
                    state.status = flow_bar::Status::Success;
                    state.message = None;
                }
                let can_clear =
                    !state.transcript.is_empty() || state.status != flow_bar::Status::Hidden;
                if ui
                    .add_enabled(can_clear, egui::Button::new("Clear transcript"))
                    .clicked()
                {
                    state.status = flow_bar::Status::Hidden;
                    state.transcript.clear();
                    state.message = None;
                }
            });
        }
    });

    theme::card(ui, |ui| {
        theme::card_title(ui, "Transcript");
        theme::helper(ui, "Kept in memory only, never logged.");
        ui.add_space(6.0);
        if state.status == flow_bar::Status::Idle && state.transcript.trim().is_empty() {
            theme::helper(
                ui,
                "Nothing here yet. Start listening, then type or paste what you would have said.",
            );
        }
        ui.add(
            egui::TextEdit::multiline(&mut state.transcript)
                .hint_text("Type or paste what you would have said…")
                .desired_rows(7)
                .desired_width(f32::INFINITY),
        );
        let transcript_count = cached_chars(
            &mut state.transcript_bytes,
            &mut state.transcript_chars,
            &state.transcript,
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            theme::mono_small(
                ui,
                &count_label(transcript_count, word_count(&state.transcript)),
            );
        });
        if state.status == flow_bar::Status::Success {
            ui.add_space(4.0);
            theme::success_label(ui, "Marked done. The transcript stays in memory.");
        }
        if let Some(msg) = state.message.clone() {
            ui.add_space(4.0);
            theme::error_label(ui, &msg);
            if state.status == flow_bar::Status::Error {
                theme::helper(
                    ui,
                    "Add some text above, then choose Stop and review again.",
                );
            }
        }
    });
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
    go: &mut Option<Surface>,
) {
    poll_action(state);
    ui.label(RichText::new("Writing tools").size(26.0).strong());
    ui.add_space(2.0);
    theme::helper(
        ui,
        "Reshape pasted text with Gemini. Your words stay yours.",
    );
    ui.add_space(10.0);

    let busy = state.pending.is_some();
    let key_missing = api_key.trim().is_empty();
    if key_missing {
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                theme::status_dot(ui, theme::WARN, 10.0);
                ui.label(RichText::new("Add your Gemini API key to run actions.").strong());
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Open Settings").clicked() {
                    *go = Some(Surface::Settings);
                }
                theme::helper(ui, "One minute, then everything lights up.");
            });
        });
    }

    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::card_title(ui, "Text to process:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.input.is_empty()
                    && ui
                        .add_enabled(!busy, egui::Button::new("Clear"))
                        .on_hover_text("Clear the input text.")
                        .clicked()
                {
                    state.input.clear();
                    state.captured_app = None;
                    state.can_replace = false;
                }
            });
        });
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::multiline(&mut state.input)
                .hint_text("Paste the text to process…")
                .desired_rows(7)
                .desired_width(f32::INFINITY),
        );
        let input_count =
            cached_chars(&mut state.input_bytes, &mut state.input_chars, &state.input);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            theme::mono_small(ui, &count_label(input_count, word_count(&state.input)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(app) = &state.captured_app {
                    theme::helper(ui, &format!("From {app}"));
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Capture clipboard"))
                .on_hover_text("Reads the OS clipboard — copy text in any app first, then Capture.")
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
            theme::helper(ui, "Copy anywhere, then Capture — or paste above.");
        });
        if busy {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button("Cancel").clicked() {
                    state.pending = None;
                    state.mode = writing_tools::Mode::Error;
                    state.error = Some("Cancelled.".to_string());
                }
            });
        }
    });

    theme::card(ui, |ui| {
        theme::card_title(ui, "Actions");
        ui.add_space(2.0);
        theme::section_label(ui, "Fix and reshape");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(14.0, 9.0);
            for (label, action) in RESHAPE_ACTIONS {
                if ui
                    .add_enabled(!busy, egui::Button::new(label))
                    .on_hover_text(format!("{} with {}", label, model))
                    .clicked()
                {
                    run_action(state, action, api_key, model, None, ctx);
                }
            }
        });
        ui.add_space(6.0);
        theme::section_label(ui, "Extract");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(14.0, 9.0);
            for (label, action) in EXTRACT_ACTIONS {
                if ui
                    .add_enabled(!busy, egui::Button::new(label))
                    .on_hover_text(format!("{} with {}", label, model))
                    .clicked()
                {
                    run_action(state, action, api_key, model, None, ctx);
                }
            }
        });
        ui.add_space(6.0);
        theme::section_label(ui, "Ask");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(14.0, 9.0);
            if ui
                .add_enabled(!busy, egui::Button::new("Quick chat"))
                .on_hover_text(format!("Ask about this text with {model}"))
                .clicked()
            {
                run_action(state, Action::Chat, api_key, model, None, ctx);
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Custom:");
            let row_spacing = ui.spacing().item_spacing.x;
            let button_width = 116.0;
            let edit_width = (ui.available_width() - button_width - row_spacing).max(80.0);
            ui.add_sized(
                [edit_width, theme::CONTROL_HEIGHT],
                egui::TextEdit::singleline(&mut state.custom_instruction)
                    .hint_text("For example: turn into a checklist"),
            );
            let run_custom = ui
                .add_enabled_ui(!busy, |ui| {
                    ui.add_sized(
                        [button_width, theme::CONTROL_HEIGHT],
                        egui::Button::new("Run custom"),
                    )
                    .clicked()
                })
                .inner;
            if run_custom {
                let custom = state.custom_instruction.clone();
                run_action(state, Action::Custom, api_key, model, Some(custom), ctx);
            }
        });
    });

    match state.mode {
        writing_tools::Mode::Processing => {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("Working with {model}…")).strong());
                });
                ui.add_space(4.0);
                theme::helper(
                    ui,
                    "The request runs in the background (up to 90s). Actions stay disabled until it finishes.",
                );
                ui.add_space(6.0);
                if ui.button("Cancel").clicked() {
                    state.pending = None;
                    state.mode = writing_tools::Mode::Error;
                    state.error = Some("Cancelled.".to_string());
                }
            });
        }
        writing_tools::Mode::Result | writing_tools::Mode::Chat => {
            theme::card(ui, |ui| {
                theme::card_title(ui, "Result");
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::multiline(&mut state.result)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                );
                let result_count = cached_chars(
                    &mut state.result_bytes,
                    &mut state.result_chars,
                    &state.result,
                );
                ui.add_space(4.0);
                theme::mono_small(ui, &count_label(result_count, word_count(&state.result)));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Copy").clicked() {
                        platform::copy_to_clipboard(ctx, &state.result);
                        state.copied_at = Some(std::time::Instant::now());
                    }
                    let can_replace =
                        state.can_replace && state.action.is_some_and(Action::replaces_selection);
                    let replace =
                        ui.add_enabled(can_replace, egui::Button::new("Copy for replace"));
                    let replace = replace.on_disabled_hover_text(
                        "Replacement needs a captured selection from an action that rewrites text.",
                    );
                    if replace
                        .on_hover_text("Copies the result so you can paste over the original.")
                        .clicked()
                    {
                        match platform::replace_selection(&state.result) {
                            Ok(_) => {
                                state.error = None;
                                state.copied_at = Some(std::time::Instant::now());
                            }
                            Err(e) => {
                                state.error = Some(e.to_string());
                                state.mode = writing_tools::Mode::Error;
                            }
                        }
                    }
                    if ui.button("Start over").clicked() {
                        state.mode = writing_tools::Mode::Menu;
                    }
                });
                if let Some(at) = state.copied_at {
                    if at.elapsed().as_secs() < 5 {
                        ui.add_space(4.0);
                        theme::success_label(ui, "Copied — paste with Ctrl+V (Cmd+V on Mac).");
                    }
                }
                if let Some(err) = state.error.clone() {
                    ui.add_space(4.0);
                    theme::error_label(ui, &err);
                }
            });
        }
        writing_tools::Mode::Error => {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::status_dot(ui, theme::ERROR, 10.0);
                    ui.label(RichText::new("Something needs attention").strong());
                });
                if let Some(err) = state.error.clone() {
                    ui.add_space(4.0);
                    theme::error_label(ui, &err);
                }
                theme::helper(
                    ui,
                    "Check the text, key, model, and connection, then try again.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Back to input").clicked() {
                        state.mode = writing_tools::Mode::Menu;
                    }
                    if let Some(action) = state.action {
                        if ui.button(format!("Retry {}", action.label())).clicked() {
                            let custom = if action == Action::Custom {
                                Some(state.custom_instruction.clone())
                            } else {
                                None
                            };
                            run_action(state, action, api_key, model, custom, ctx);
                        }
                    }
                });
            });
        }
        writing_tools::Mode::Menu => {
            if !state.input.trim().is_empty() {
                theme::helper(ui, "Choose an action above — the result lands here.");
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
    if action == Action::Custom && custom.as_deref().unwrap_or("").trim().is_empty() {
        state.error = Some("Enter a custom instruction first.".to_string());
        state.mode = writing_tools::Mode::Error;
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
    if api_key.trim().is_empty() {
        state.error = Some("Set a valid Gemini API key in Settings first.".to_string());
        state.mode = writing_tools::Mode::Error;
        return;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    state.pending = Some(receiver);
    state.action = Some(action);
    state.error = None;
    state.copied_at = None;
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
    fn counts_cover_words_and_chars() {
        assert_eq!(count_label(11, 2), "11 characters · 2 words");
        assert_eq!(count_label(1, 1), "1 character · 1 word");
        assert_eq!(count_label(0, 0), "0 characters · 0 words");
        assert_eq!(word_count("  hello   world "), 2);
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn elapsed_timer_renders_mm_ss() {
        assert_eq!(elapsed_label(None), "00:00");
        let since = std::time::Instant::now() - std::time::Duration::from_secs(75);
        assert_eq!(elapsed_label(Some(since)), "01:15");
    }

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

    #[test]
    fn missing_key_is_a_clean_error_not_a_spawn() {
        let ctx = egui::Context::default();
        let mut state = writing_tools::State {
            input: "hello".to_owned(),
            ..Default::default()
        };
        run_action(
            &mut state,
            Action::Proofread,
            "   ",
            "gemini-3.8-flash",
            None,
            &ctx,
        );
        assert_eq!(state.mode, writing_tools::Mode::Error);
        assert!(state.pending.is_none());
        assert!(state.error.as_deref().unwrap_or("").contains("API key"));
    }
}
