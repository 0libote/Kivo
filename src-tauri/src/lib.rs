mod ai;
mod commands;
mod config;
mod platform;
mod security;
mod shell;
mod speech;
mod text;

use std::sync::Arc;

use tauri::Manager;

use crate::{
    ai::GeminiClient,
    commands::AppCore,
    config::SettingsRepository,
    platform::{
        PlatformServices,
        adapters::{PlatformCredentialStore, PlatformSpeechEngine, PlatformTextService},
    },
    shell::{ShellSettingsRuntime, ShellState},
    speech::{local::LocalSpeechEngine, model_store::ModelStore, router::SelectableSpeechEngine},
};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = shell::show_surface(app, "settings", true);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(ShellState::new())
        .setup(|app| {
            let handle = app.handle().clone();
            let platform = Arc::new(PlatformServices::new()?);
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            // On-device models live in the app data directory and are managed
            // by Kivo (downloaded, verified, deleted); nothing is bundled.
            let model_store = Arc::new(ModelStore::new(app.path().app_data_dir()?.join("models")));
            let system_speech = Arc::new(PlatformSpeechEngine::new(Arc::clone(&platform)));
            let local_speech = Arc::new(LocalSpeechEngine::new(Arc::clone(&model_store)));
            let core = AppCore::new(
                SettingsRepository::new(settings_path),
                Arc::new(ShellSettingsRuntime(handle.clone())),
                Arc::new(PlatformCredentialStore::new(Arc::clone(&platform))),
                GeminiClient::new()?,
                Arc::new(SelectableSpeechEngine::new(system_speech, local_speech)),
                Arc::new(PlatformTextService::new(Arc::clone(&platform))),
            )?;
            let settings = commands::FrontendSettings::from(core.settings()?);
            app.manage(model_store);
            app.manage(platform);
            app.manage(core);

            shell::create_windows(&handle)?;
            shell::create_tray(&handle)?;
            shell::apply_theme(&handle, &settings.theme);
            let _ = shell::register_shortcuts(&handle, &settings);
            if settings.onboarding_complete {
                shell::sync_idle_flow_bar(&handle);
            }
            if settings.onboarding_complete {
                if !settings.start_in_background {
                    shell::show_surface(&handle, "settings", true)?;
                }
            } else {
                shell::show_surface(&handle, "onboarding", true)?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(target_os = "windows")]
            if let tauri::WindowEvent::ThemeChanged(theme) = event {
                shell::apply_caption_theme(window.app_handle(), window.label(), *theme);
            }
            #[cfg(target_os = "windows")]
            if let tauri::WindowEvent::Focused(_) = event
                && let Ok(theme) = window.theme()
            {
                shell::apply_caption_theme(window.app_handle(), window.label(), theme);
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && matches!(window.label(), "settings" | "onboarding")
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_context,
            commands::get_settings,
            commands::update_settings,
            commands::get_permission_statuses,
            commands::request_permission,
            commands::open_permission_settings,
            commands::reset_permission_grants,
            commands::list_microphones,
            commands::list_speech_languages,
            commands::list_local_speech_models,
            commands::download_local_speech_model,
            commands::cancel_local_speech_model_download,
            commands::delete_local_speech_model,
            commands::detect_local_ai_servers,
            commands::install_local_ai_runtime,
            commands::list_ai_providers,
            commands::list_ai_models,
            commands::get_api_key_status,
            commands::store_api_key,
            commands::remove_api_key,
            commands::test_api_key,
            commands::start_dictation,
            commands::stop_dictation,
            commands::cancel_dictation,
            commands::retry_dictation,
            commands::get_writing_context,
            commands::run_writing_action,
            commands::replace_writing_result,
            commands::copy_text,
            commands::get_dictation_recovery,
            commands::clear_dictation_recovery,
            commands::close_surface,
            commands::show_surface,
            commands::set_surface_mode,
            commands::complete_onboarding,
            commands::set_paused,
            commands::check_for_updates,
            crate::shell::install_update,
            crate::shell::restart_app,
            commands::open_external,
        ])
        .run(tauri::generate_context!())
        .expect("Kivo failed to start");
}
