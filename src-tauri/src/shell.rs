use std::{
    io::Write,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Position, Size, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, menu::MenuBuilder, tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_updater::UpdaterExt;

use crate::{
    commands::{AppCore, CommandError, FrontendSettings, UpdateResult},
    config::{AppSettings, SettingsRuntime, SettingsRuntimeError},
    platform::{
        HoldShortcut, HoldShortcutEvent, OverlayKind, PermissionKind, PlatformError,
        PlatformErrorKind, PlatformServices, ShortcutRegistration,
    },
    speech::{DictationPhase, SpeechEvent, SpeechEventSink},
};

pub(crate) struct ShellState {
    paused: AtomicBool,
    dictation_held: AtomicBool,
    dictation_cancel_epoch: AtomicU64,
    escape_shortcut_registered: AtomicBool,
    writing_shortcut: Mutex<Option<String>>,
    dictation_shortcut: Mutex<Option<ActiveDictationShortcut>>,
}

struct ActiveDictationShortcut {
    accelerator: String,
    registration: Box<dyn ShortcutRegistration>,
}

struct GlobalHoldShortcutRegistration {
    app: AppHandle,
    accelerator: String,
    stopped: bool,
}

impl ShortcutRegistration for GlobalHoldShortcutRegistration {
    fn stop(&mut self) -> Result<(), PlatformError> {
        if !self.stopped {
            let _ = self
                .app
                .global_shortcut()
                .unregister(self.accelerator.as_str());
            self.stopped = true;
        }
        Ok(())
    }
}

impl Drop for GlobalHoldShortcutRegistration {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl ShellState {
    pub(crate) fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            dictation_held: AtomicBool::new(false),
            dictation_cancel_epoch: AtomicU64::new(0),
            escape_shortcut_registered: AtomicBool::new(false),
            writing_shortcut: Mutex::new(None),
            dictation_shortcut: Mutex::new(None),
        }
    }

    pub(crate) fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }
}

pub(crate) struct DeferredSettingsRuntime;

impl SettingsRuntime for DeferredSettingsRuntime {
    fn apply(
        &self,
        _previous: &AppSettings,
        _updated: &AppSettings,
    ) -> Result<(), SettingsRuntimeError> {
        // Tauri objects are main-thread-bound. The command boundary applies these
        // effects before persisting the corresponding frontend settings patch.
        Ok(())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DictationSnapshot<'a> {
    status: &'a str,
    message: Option<&'a str>,
    can_retry: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WritingContextEvent {
    has_selection: bool,
    application_name: String,
    can_replace: bool,
    bounds: Option<crate::text::ScreenRect>,
}

pub(crate) fn create_windows(app: &AppHandle) -> tauri::Result<()> {
    build_window(app, "flow-bar", "Kivo Dictation", 220.0, 68.0, false, true)?;
    build_window(
        app,
        "writing-tools",
        "Kivo Writing Tools",
        360.0,
        420.0,
        true,
        true,
    )?;
    build_window(app, "settings", "Kivo Settings", 820.0, 600.0, true, false)?;
    build_window(
        app,
        "onboarding",
        "Welcome to Kivo",
        640.0,
        560.0,
        true,
        false,
    )?;

    for (label, kind) in [
        ("flow-bar", OverlayKind::FlowBar),
        ("writing-tools", OverlayKind::WritingTools),
        ("settings", OverlayKind::Settings),
    ] {
        style_window(app, label, kind);
    }
    Ok(())
}

pub(crate) fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("settings", "Settings…")
        .text("writing-tools", "Writing Tools")
        .text("dictation", "Start Dictation")
        .separator()
        .text("pause", "Pause Kivo")
        .text("about", "About Kivo")
        .separator()
        .quit()
        .build()?;
    let mut tray = TrayIconBuilder::with_id("kivo")
        .tooltip("Kivo")
        .menu(&menu)
        .icon_as_template(cfg!(target_os = "macos"))
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => {
                let _ = show_surface(app, "settings", true);
            }
            "writing-tools" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_writing_tools(&app).await;
                });
            }
            "dictation" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let core = app.state::<AppCore>();
                    let _ = begin_dictation(&app, &core).await;
                });
            }
            "pause" => {
                let shell = app.state::<ShellState>();
                shell.set_paused(!shell.paused());
            }
            "about" => {
                let _ = show_surface(app, "settings", true);
                let _ = app.emit_to("settings", "show-about", ());
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn build_window(
    app: &AppHandle,
    label: &str,
    title: &str,
    width: f64,
    height: f64,
    focusable: bool,
    transparent: bool,
) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(width, height)
        .decorations(!transparent)
        .transparent(transparent)
        .always_on_top(label == "flow-bar" || label == "writing-tools")
        .skip_taskbar(label == "flow-bar" || label == "writing-tools")
        .focusable(focusable)
        .visible(false)
        .resizable(label == "settings")
        .visible_on_all_workspaces(label == "flow-bar");
    let builder = if matches!(label, "settings" | "onboarding") {
        builder.min_inner_size(width.min(520.0), height.min(420.0))
    } else {
        builder
    };
    builder.build()
}

fn style_window(app: &AppHandle, label: &str, kind: OverlayKind) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    #[cfg(target_os = "macos")]
    let handle = window.ns_window().ok().map(|handle| handle as usize);
    #[cfg(target_os = "windows")]
    let handle = window.hwnd().ok().map(|handle| handle.0 as usize);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let handle = None;
    if let (Some(handle), Some(platform)) = (handle, app.try_state::<Arc<PlatformServices>>()) {
        let _ = platform.style_window(handle, kind);
    }
}

pub(crate) fn register_shortcuts(
    app: &AppHandle,
    settings: &FrontendSettings,
) -> Result<(), PlatformError> {
    let shell = app.state::<ShellState>();
    register_writing_shortcut(app, &shell, &settings.writing_shortcut)?;

    register_dictation_shortcut(app, &shell, &settings.dictation_shortcut)?;
    Ok(())
}

fn register_dictation_shortcut(
    app: &AppHandle,
    shell: &ShellState,
    accelerator: &str,
) -> Result<(), PlatformError> {
    let normalized = accelerator
        .replace("Ctrl", "Control")
        .replace("Meta", "Super");
    let mut current = shell.dictation_shortcut.lock().map_err(|_| {
        PlatformError::new(
            PlatformErrorKind::InvalidState,
            "register_shortcuts",
            "Shortcut state is unavailable.",
        )
    })?;
    if current
        .as_ref()
        .is_some_and(|current| current.accelerator == normalized)
    {
        return Ok(());
    }

    let app_for_shortcut = app.clone();
    let callback = Arc::new(move |event| dispatch_dictation_event(app_for_shortcut.clone(), event));
    let registration_result: Result<Box<dyn ShortcutRegistration>, PlatformError> =
        if is_native_dictation_shortcut(&normalized) {
            app.state::<Arc<PlatformServices>>()
                .register_dictation_shortcut(HoldShortcut::platform_default(), callback)
        } else {
            let callback_for_shortcut = Arc::clone(&callback);
            app.global_shortcut()
                .on_shortcut(normalized.as_str(), move |_, _, event| {
                    let event = match event.state() {
                        ShortcutState::Pressed => HoldShortcutEvent::Pressed,
                        ShortcutState::Released => HoldShortcutEvent::Released,
                    };
                    callback_for_shortcut(event);
                })
                .map_err(|_| {
                    PlatformError::new(
                        PlatformErrorKind::ShortcutConflict,
                        "register_dictation_shortcut",
                        "That dictation shortcut is already in use.",
                    )
                })
                .map(|_| {
                    Box::new(GlobalHoldShortcutRegistration {
                        app: app.clone(),
                        accelerator: normalized.clone(),
                        stopped: false,
                    }) as Box<dyn ShortcutRegistration>
                })
        };

    let registration = match registration_result {
        Ok(registration) => registration,
        Err(error) => {
            if error.kind == PlatformErrorKind::PermissionDenied {
                if let Some(mut previous) = current.take() {
                    let _ = previous.registration.stop();
                }
            }
            return Err(error);
        }
    };

    if let Some(mut previous) = current.take() {
        let _ = previous.registration.stop();
    }
    *current = Some(ActiveDictationShortcut {
        accelerator: normalized,
        registration,
    });
    Ok(())
}

fn is_native_dictation_shortcut(shortcut: &str) -> bool {
    (cfg!(target_os = "macos") && shortcut == "Fn")
        || (cfg!(target_os = "windows") && shortcut == "Control+Super")
}

fn dispatch_dictation_event(app: AppHandle, event: HoldShortcutEvent) {
    tauri::async_runtime::spawn(async move {
        let shell = app.state::<ShellState>();
        let core = app.state::<AppCore>();
        match event {
            HoldShortcutEvent::Pressed => {
                if shell.paused() {
                    return;
                }
                shell.dictation_held.store(true, Ordering::Release);
                let cancel_epoch = shell.dictation_cancel_epoch.load(Ordering::Acquire);
                if begin_dictation(&app, &core).await.is_ok()
                    && !shell.dictation_held.load(Ordering::Acquire)
                {
                    if shell.dictation_cancel_epoch.load(Ordering::Acquire) != cancel_epoch {
                        let _ = core.cancel_dictation().await;
                        unregister_cancel_shortcut(&app);
                        sync_idle_flow_bar(&app);
                    } else {
                        let _ = finish_dictation(&app, &core).await;
                    }
                }
            }
            HoldShortcutEvent::Released => {
                shell.dictation_held.store(false, Ordering::Release);
                let _ = finish_dictation(&app, &core).await;
            }
            HoldShortcutEvent::Cancelled => {
                shell.dictation_held.store(false, Ordering::Release);
                shell.dictation_cancel_epoch.fetch_add(1, Ordering::AcqRel);
                let _ = core.cancel_dictation().await;
                unregister_cancel_shortcut(&app);
                sync_idle_flow_bar(&app);
            }
        }
    });
}

fn register_writing_shortcut(
    app: &AppHandle,
    shell: &ShellState,
    shortcut: &str,
) -> Result<(), PlatformError> {
    let normalized = shortcut.replace("Ctrl", "Control").replace("Meta", "Super");
    let mut current = shell.writing_shortcut.lock().map_err(|_| {
        PlatformError::new(
            PlatformErrorKind::InvalidState,
            "register_writing_shortcut",
            "Shortcut state is unavailable.",
        )
    })?;
    if current.as_deref() == Some(normalized.as_str()) {
        return Ok(());
    }
    app.global_shortcut()
        .on_shortcut(normalized.as_str(), |app, _, event| {
            if event.state() == ShortcutState::Pressed && !app.state::<ShellState>().paused() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_writing_tools(&app).await;
                });
            }
        })
        .map_err(|_| {
            PlatformError::new(
                PlatformErrorKind::ShortcutConflict,
                "register_writing_shortcut",
                "That Writing Tools shortcut is already in use.",
            )
        })?;
    if let Some(previous) = current.take() {
        let _ = app.global_shortcut().unregister(previous.as_str());
    }
    *current = Some(normalized);
    Ok(())
}

pub(crate) fn apply_settings(
    app: &AppHandle,
    settings: &FrontendSettings,
) -> Result<(), crate::commands::AppCoreError> {
    let autolaunch = app.autolaunch();
    let autolaunch_enabled = autolaunch
        .is_enabled()
        .map_err(|_| SettingsRuntimeError::AutostartUnavailable)?;
    if settings.launch_at_login != autolaunch_enabled {
        if settings.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        }
        .map_err(|_| SettingsRuntimeError::AutostartUnavailable)?;
    }
    register_writing_shortcut(app, &app.state::<ShellState>(), &settings.writing_shortcut)
        .map_err(|_| SettingsRuntimeError::ShortcutUnavailable)?;
    if let Err(error) = register_dictation_shortcut(
        app,
        &app.state::<ShellState>(),
        &settings.dictation_shortcut,
    ) {
        if error.kind != PlatformErrorKind::PermissionDenied {
            return Err(SettingsRuntimeError::ShortcutUnavailable.into());
        }
    }
    if settings.show_idle_flow_bar {
        emit_dictation(app, "idle", None, false);
        let _ = show_surface(app, "flow-bar", false);
    } else {
        hide_surface(app, "flow-bar");
    }
    Ok(())
}

pub(crate) async fn begin_dictation(app: &AppHandle, core: &AppCore) -> Result<(), CommandError> {
    if core.dictation_phase().map_err(CommandError::from)? == DictationPhase::Listening {
        return Ok(());
    }
    show_surface(app, "flow-bar", false).map_err(platform_command_error)?;
    emit_dictation(app, "listening", None, false);
    let event_app = app.clone();
    let events: SpeechEventSink = Arc::new(move |event| match event {
        SpeechEvent::AudioLevel(level) => {
            let _ = event_app.emit_to("flow-bar", "dictation-level", level);
        }
        SpeechEvent::SpeechDetected => {}
    });
    if let Err(error) = core.begin_dictation(events).await {
        let error = CommandError::from(error);
        emit_dictation(app, "error", Some(&error.message), error.recoverable);
        return Err(error);
    }
    register_cancel_shortcut(app);
    play_dictation_feedback(core, FeedbackMoment::Start);
    Ok(())
}

pub(crate) async fn open_writing_tools(app: &AppHandle) -> Result<(), CommandError> {
    let core = app.state::<AppCore>();
    match core.open_writing_tools().await {
        Ok(context) => {
            size_writing_surface(app, "menu").map_err(platform_command_error)?;
            position_writing_surface(app, context.anchor);
            show_surface(app, "writing-tools", true).map_err(platform_command_error)?;
            let _ = app.emit_to(
                "writing-tools",
                "writing-context",
                WritingContextEvent {
                    has_selection: true,
                    application_name: context.application.display_name,
                    can_replace: true,
                    bounds: context.anchor,
                },
            );
            Ok(())
        }
        Err(error) => {
            size_writing_surface(app, "error").map_err(platform_command_error)?;
            show_surface(app, "writing-tools", true).map_err(platform_command_error)?;
            let command_error = CommandError::from(error);
            let _ = app.emit_to("writing-tools", "writing-error", command_error.clone());
            Err(command_error)
        }
    }
}

pub(crate) async fn finish_dictation(app: &AppHandle, core: &AppCore) -> Result<(), CommandError> {
    if core.dictation_phase().map_err(CommandError::from)? != DictationPhase::Listening {
        return Ok(());
    }
    unregister_cancel_shortcut(app);
    emit_dictation(app, "processing", None, false);
    match core.finish_dictation().await {
        Ok(_) => {
            play_dictation_feedback(core, FeedbackMoment::Finish);
            emit_dictation(app, "success", None, false);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(620)).await;
                sync_idle_flow_bar(&app);
            });
            Ok(())
        }
        Err(error) => {
            let error = CommandError::from(error);
            emit_dictation(app, "error", Some(&error.message), error.recoverable);
            Err(error)
        }
    }
}

fn register_cancel_shortcut(app: &AppHandle) {
    let shell = app.state::<ShellState>();
    if shell
        .escape_shortcut_registered
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let result = app
        .global_shortcut()
        .on_shortcut("Escape", |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                dispatch_dictation_event(app.clone(), HoldShortcutEvent::Cancelled);
            }
        });
    if result.is_err() {
        shell
            .escape_shortcut_registered
            .store(false, Ordering::Release);
    }
}

pub(crate) fn unregister_cancel_shortcut(app: &AppHandle) {
    let shell = app.state::<ShellState>();
    if shell
        .escape_shortcut_registered
        .swap(false, Ordering::AcqRel)
    {
        let _ = app.global_shortcut().unregister("Escape");
    }
}

pub(crate) fn emit_dictation(
    app: &AppHandle,
    status: &str,
    message: Option<&str>,
    can_retry: bool,
) {
    size_flow_bar(app, status);
    let _ = app.emit_to(
        "flow-bar",
        "dictation-state",
        DictationSnapshot {
            status,
            message,
            can_retry,
        },
    );
}

fn size_flow_bar(app: &AppHandle, status: &str) {
    let (width, height) = match status {
        "idle" => (40.0, 40.0),
        "listening" => (104.0, 48.0),
        "processing" => (128.0, 48.0),
        "success" => (48.0, 48.0),
        "error" => (220.0, 56.0),
        _ => (40.0, 40.0),
    };
    if let Some(window) = app.get_webview_window("flow-bar") {
        let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
        position_flow_bar(app, &window);
    }
}

pub(crate) fn size_writing_surface(app: &AppHandle, mode: &str) -> Result<(), PlatformError> {
    let height = match mode {
        "menu" => 246.0,
        "custom" | "processing" => 56.0,
        "result" => 420.0,
        "error" => 96.0,
        _ => {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "size_writing_surface",
                "The Writing Tools surface mode is invalid.",
            ));
        }
    };
    let window = app.get_webview_window("writing-tools").ok_or_else(|| {
        PlatformError::new(
            PlatformErrorKind::NotFound,
            "size_writing_surface",
            "The Writing Tools window is unavailable.",
        )
    })?;
    window
        .set_size(Size::Logical(LogicalSize::new(360.0, height)))
        .map_err(|_| window_error("size_writing_surface"))
}

pub(crate) fn show_surface(
    app: &AppHandle,
    surface: &str,
    focus: bool,
) -> Result<(), PlatformError> {
    let window = app.get_webview_window(surface).ok_or_else(|| {
        PlatformError::new(
            PlatformErrorKind::NotFound,
            "show_surface",
            "The requested Kivo window is unavailable.",
        )
    })?;
    if surface == "flow-bar" {
        position_flow_bar(app, &window);
    }
    window.show().map_err(|_| window_error("show_surface"))?;
    if focus {
        window
            .set_focus()
            .map_err(|_| window_error("show_surface"))?;
    }
    Ok(())
}

fn position_flow_bar(app: &AppHandle, window: &WebviewWindow) {
    let cursor = app
        .try_state::<Arc<PlatformServices>>()
        .and_then(|platform| platform.get_cursor_or_selection_position().ok());
    let monitor = cursor
        .and_then(|point| {
            window.available_monitors().ok().and_then(|monitors| {
                monitors.into_iter().find(|monitor| {
                    let origin = monitor.position();
                    let size = monitor.size();
                    point.x >= f64::from(origin.x)
                        && point.x < f64::from(origin.x) + f64::from(size.width)
                        && point.y >= f64::from(origin.y)
                        && point.y < f64::from(origin.y) + f64::from(size.height)
                })
            })
        })
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let size = monitor.size();
    let origin = monitor.position();
    let Ok(window_size) = window.outer_size() else {
        return;
    };
    let x = origin.x + (size.width.saturating_sub(window_size.width) / 2) as i32;
    let y = origin.y + size.height.saturating_sub(window_size.height + 48) as i32;
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

pub(crate) fn sync_idle_flow_bar(app: &AppHandle) {
    let show_idle = app
        .try_state::<AppCore>()
        .and_then(|core| core.settings().ok())
        .is_some_and(|settings| settings.general.show_flow_bar_while_idle);
    if show_idle {
        emit_dictation(app, "idle", None, false);
        let _ = show_surface(app, "flow-bar", false);
    } else {
        emit_dictation(app, "hidden", None, false);
        hide_surface(app, "flow-bar");
    }
}

#[derive(Clone, Copy)]
enum FeedbackMoment {
    Start,
    Finish,
}

fn play_dictation_feedback(core: &AppCore, moment: FeedbackMoment) {
    let enabled = core
        .settings()
        .is_ok_and(|settings| settings.dictation.sound_feedback);
    if !enabled {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let sound = match moment {
            FeedbackMoment::Start => "/System/Library/Sounds/Tink.aiff",
            FeedbackMoment::Finish => "/System/Library/Sounds/Pop.aiff",
        };
        let _ = Command::new("afplay")
            .arg(sound)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = moment;
        unsafe {
            let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(
                windows::Win32::UI::WindowsAndMessaging::MB_OK,
            );
        }
    }
}

pub(crate) fn position_writing_surface(app: &AppHandle, bounds: Option<crate::text::ScreenRect>) {
    let (Some(window), Some(bounds)) = (app.get_webview_window("writing-tools"), bounds) else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let mut x = (bounds.x + bounds.width / 2.0 - f64::from(size.width) / 2.0).round() as i32;
    let mut y = (bounds.y + bounds.height + 10.0).round() as i32;
    if let Ok(Some(monitor)) = window
        .current_monitor()
        .or_else(|_| window.primary_monitor())
    {
        let origin = monitor.position();
        let monitor_size = monitor.size();
        let right = origin.x + monitor_size.width as i32;
        let bottom = origin.y + monitor_size.height as i32;
        x = x.clamp(origin.x + 8, right - size.width as i32 - 8);
        if y + size.height as i32 > bottom - 8 {
            y = (bounds.y - f64::from(size.height) - 10.0).round() as i32;
        }
        y = y.clamp(origin.y + 8, bottom - size.height as i32 - 8);
    }
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

pub(crate) fn hide_surface(app: &AppHandle, surface: &str) {
    if let Some(window) = app.get_webview_window(surface) {
        let _ = window.hide();
    }
}

pub(crate) fn open_permission_settings(permission: PermissionKind) -> Result<(), PlatformError> {
    #[cfg(target_os = "windows")]
    let _ = &permission;
    #[cfg(target_os = "macos")]
    let url = match permission {
        PermissionKind::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        PermissionKind::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        PermissionKind::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        PermissionKind::SpeechRecognition => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition"
        }
    };
    #[cfg(target_os = "windows")]
    let url = "ms-settings:privacy-microphone";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let url = "";
    open_url(url)
}

pub(crate) fn open_external(_app: &AppHandle, url: &str) -> Result<(), PlatformError> {
    let allowed = [
        "https://aistudio.google.com/",
        "https://github.com/0libote/Kivo",
    ];
    if !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err(PlatformError::new(
            PlatformErrorKind::InvalidState,
            "open_external",
            "Kivo blocked an untrusted external URL.",
        ));
    }
    open_url(url)
}

fn open_url(url: &str) -> Result<(), PlatformError> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    command.spawn().map(|_| ()).map_err(|_| {
        PlatformError::new(
            PlatformErrorKind::Os,
            "open_url",
            "The system settings page could not be opened.",
        )
    })
}

pub(crate) fn copy_text(text: &str) -> Result<(), PlatformError> {
    #[cfg(target_os = "macos")]
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| clipboard_error())?;
    #[cfg(target_os = "windows")]
    let mut child = Command::new("clip")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| clipboard_error())?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Err(clipboard_error());
    child
        .stdin
        .take()
        .ok_or_else(clipboard_error)?
        .write_all(text.as_bytes())
        .map_err(|_| clipboard_error())?;
    child.wait().map_err(|_| clipboard_error())?;
    Ok(())
}

fn clipboard_error() -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Os,
        "copy_text",
        "The result could not be copied.",
    )
}

pub(crate) async fn check_for_updates(app: &AppHandle) -> Result<UpdateResult, CommandError> {
    let current_version = app.package_info().version.to_string();

    #[cfg(target_os = "windows")]
    {
        #[derive(serde::Deserialize)]
        struct GitHubRelease {
            tag_name: String,
        }

        let release = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .map_err(|_| update_check_error())?
            .get("https://api.github.com/repos/0libote/Kivo/releases/latest")
            .header("accept", "application/vnd.github+json")
            .header("user-agent", "Kivo desktop updater")
            .send()
            .await
            .map_err(|_| update_check_error())?
            .error_for_status()
            .map_err(|_| update_check_error())?
            .json::<GitHubRelease>()
            .await
            .map_err(|_| update_check_error())?;
        let available_version = release.tag_name.strip_prefix("app-v").map(str::to_owned);
        let available = available_version
            .as_deref()
            .is_some_and(|version| version_is_newer(version, &current_version));
        Ok(UpdateResult {
            current_version,
            available_version: available.then_some(release.tag_name.replace("app-v", "")),
            available,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        let update = app
            .updater()
            .map_err(|_| CommandError {
                code: "updater_unavailable".into(),
                message: "Update checks are unavailable in this build.".into(),
                recoverable: false,
            })?
            .check()
            .await
            .map_err(|_| CommandError {
                code: "update_check_failed".into(),
                message: "Kivo couldn’t check for updates right now.".into(),
                recoverable: true,
            })?;
        Ok(UpdateResult {
            current_version,
            available_version: update.as_ref().map(|update| update.version.clone()),
            available: update.is_some(),
        })
    }
}

#[cfg(target_os = "windows")]
fn update_check_error() -> CommandError {
    CommandError {
        code: "update_check_failed".into(),
        message: "Kivo couldn’t check for updates right now.".into(),
        recoverable: true,
    }
}

#[cfg(any(target_os = "windows", test))]
fn version_is_newer(candidate: &str, current: &str) -> bool {
    fn parts(version: &str) -> Option<Vec<u64>> {
        version
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    }
    matches!((parts(candidate), parts(current)), (Some(candidate), Some(current)) if candidate > current)
}

fn platform_command_error(error: PlatformError) -> CommandError {
    CommandError {
        code: format!("platform_{:?}", error.kind).to_lowercase(),
        message: error.message,
        recoverable: matches!(
            error.kind,
            PlatformErrorKind::Speech | PlatformErrorKind::Os
        ),
    }
}

fn window_error(operation: &'static str) -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Os,
        operation,
        "The Kivo window could not be displayed.",
    )
}

#[cfg(test)]
mod tests {
    use super::version_is_newer;

    #[test]
    fn github_release_versions_are_compared_without_lexical_ordering() {
        assert!(version_is_newer("1.10.0", "1.9.9"));
        assert!(!version_is_newer("1.2.3", "1.2.3"));
        assert!(!version_is_newer("not-a-version", "1.2.3"));
    }
}
