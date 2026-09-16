#[cfg(not(target_os = "windows"))]
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Instant;
#[cfg(target_os = "macos")]
use std::{io::Write, process::Stdio};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Position, Size, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, menu::MenuBuilder, tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::{
    commands::{AppCore, CommandError, FrontendSettings, UpdateResult},
    config::{AppSettings, HostPlatform, SettingsRuntime, SettingsRuntimeError},
    platform::{
        HoldShortcut, HoldShortcutEvent, OverlayKind, PermissionKind, PlatformError,
        PlatformErrorKind, PlatformServices, ShortcutRegistration,
    },
    speech::{DictationPhase, SpeechEvent, SpeechEventSink},
};

pub(crate) struct ShellState {
    paused: AtomicBool,
    dictation_held: AtomicBool,
    dictation_press: Mutex<DictationPress>,
    dictation_cancel_epoch: AtomicU64,
    escape_shortcut_registered: AtomicBool,
    pause_item: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    writing_shortcut: Mutex<Option<String>>,
    dictation_shortcut: Mutex<Option<ActiveDictationShortcut>>,
}

#[derive(Default)]
struct DictationPress {
    started: Option<Instant>,
    released_ms: Option<u64>,
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
            dictation_press: Mutex::new(DictationPress::default()),
            dictation_cancel_epoch: AtomicU64::new(0),
            escape_shortcut_registered: AtomicBool::new(false),
            pause_item: Mutex::new(None),
            writing_shortcut: Mutex::new(None),
            dictation_shortcut: Mutex::new(None),
        }
    }

    pub(crate) fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub(crate) fn set_paused(&self, app: &AppHandle, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        if let Ok(item) = self.pause_item.lock()
            && let Some(item) = item.as_ref()
        {
            let _ = item.set_text(if paused { "Resume Kivo" } else { "Pause Kivo" });
        }
        if let Some(tray) = app.tray_by_id("kivo") {
            let _ = tray.set_tooltip(Some(if paused { "Kivo - paused" } else { "Kivo" }));
        }
        let _ = app.emit("pause-changed", paused);
        if paused {
            dispatch_dictation_event(app.clone(), HoldShortcutEvent::Cancelled);
        }
    }
}

pub(crate) struct ShellSettingsRuntime(pub AppHandle);

impl SettingsRuntime for ShellSettingsRuntime {
    fn apply(
        &self,
        previous: &AppSettings,
        updated: &AppSettings,
    ) -> Result<(), SettingsRuntimeError> {
        if let Err(error) = apply_settings(&self.0, &FrontendSettings::from(updated.clone())) {
            // Best-effort restore of the previous runtime state. This path
            // must never re-apply `updated`: a failing restore returns the
            // original error and the runtime heals on next launch (the
            // settings file is untouched on this path).
            restore_settings(&self.0, &FrontendSettings::from(previous.clone()));
            return Err(error);
        }
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
    initial_text: String,
}

/// User preferences cap the popup; the frontend reports its content height.
const WRITING_DEFAULT_WIDTH: f64 = 380.0;
const WRITING_DEFAULT_HEIGHT: f64 = 460.0;

pub(crate) fn create_windows(app: &AppHandle) -> tauri::Result<()> {
    build_window(app, "flow-bar", "Kivo Dictation", 220.0, 68.0, false, true)?;
    build_window(
        app,
        "writing-tools",
        "Kivo Writing Tools",
        WRITING_DEFAULT_WIDTH,
        WRITING_DEFAULT_HEIGHT,
        true,
        true,
    )?;
    build_window(app, "settings", "Kivo", 900.0, 650.0, true, false)?;
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
        ("onboarding", OverlayKind::Settings),
    ] {
        style_window(app, label, kind);
    }
    Ok(())
}

pub(crate) fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let pause = tauri::menu::MenuItem::with_id(app, "pause", "Pause Kivo", true, None::<&str>)?;
    // A poisoned mutex must degrade (pause label stops updating) rather than
    // panic the whole process at startup.
    if let Ok(mut slot) = app.state::<ShellState>().pause_item.lock() {
        *slot = Some(pause.clone());
    }
    let menu = MenuBuilder::new(app)
        // Primary actions first, then configuration, then lifecycle — the
        // standard tray convention so Dictation/Writing Tools are always at
        // the top where a background utility needs them.
        .text("dictation", "Start Dictation")
        .text("writing-tools", "Writing Tools")
        .separator()
        .text("settings", "Settings…")
        .text("about", "About Kivo")
        .separator()
        .item(&pause)
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
                shell.set_paused(app, !shell.paused());
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
    let (width, height) = if !transparent {
        app.primary_monitor()
            .ok()
            .flatten()
            .map(|monitor| {
                let area = monitor
                    .work_area()
                    .size
                    .to_logical::<f64>(monitor.scale_factor());
                (
                    width.min((area.width - 32.0).max(320.0)),
                    height.min((area.height - 64.0).max(320.0)),
                )
            })
            .unwrap_or((width, height))
    } else {
        (width, height)
    };
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(width, height)
        .decorations(!transparent)
        .transparent(transparent)
        // On Windows, Tauri's undecorated shadow adds its own native frame.
        // A rectangular frame must not surround the smaller recording pill.
        .shadow(!transparent)
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
    // On unsupported hosts there is no native styling to apply; reference the
    // window so the binding stays used on every target (avoids Linux-only
    // unused-variable warnings without cfg-rename churn).
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = &window;
    #[cfg(target_os = "macos")]
    let handle = window.ns_window().ok().map(|handle| handle as usize);
    #[cfg(target_os = "windows")]
    let handle = window.hwnd().ok().map(|handle| handle.0 as usize);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let handle: Option<usize> = None;
    if let (Some(handle), Some(platform)) = (handle, app.try_state::<Arc<PlatformServices>>()) {
        let _ = platform.style_window(handle, kind);
    }
}

pub(crate) fn register_shortcuts(
    app: &AppHandle,
    settings: &FrontendSettings,
) -> Result<(), PlatformError> {
    let shell = app.state::<ShellState>();
    // Independent: a conflicting writing shortcut (e.g. Ctrl+Space grabbed
    // by an IME) must not take the dictation shortcut down with it.
    let writing = register_writing_shortcut(app, &shell, &settings.writing_shortcut);
    let dictation = register_dictation_shortcut(app, &shell, &settings.dictation_shortcut);
    writing.and(dictation)
}

/// Map portable modifier names to global-shortcut accelerators token-wise.
/// A substring replace would mangle keys containing those substrings
/// (e.g. a hypothetical `Ctrlled` key), failing with a misleading
/// "already in use" conflict.
fn normalize_accelerator(accelerator: &str) -> String {
    accelerator
        .split('+')
        .map(|token| match token {
            "Ctrl" => "Control",
            "Meta" => "Super",
            _ => token,
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn register_dictation_shortcut(
    app: &AppHandle,
    shell: &ShellState,
    accelerator: &str,
) -> Result<(), PlatformError> {
    let normalized = normalize_accelerator(accelerator);
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
            if error.kind == PlatformErrorKind::PermissionDenied
                && let Some(mut previous) = current.take()
            {
                let _ = previous.registration.stop();
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
    is_native_dictation_shortcut_for(shortcut, HostPlatform::current())
}

/// Host-parameterized native-shortcut check so one test run covers both
/// platforms. The stored default is "Ctrl+Meta" but shell.rs normalizes
/// Ctrl→Control / Meta→Super before this check, hence "Control+Super".
fn is_native_dictation_shortcut_for(shortcut: &str, host: HostPlatform) -> bool {
    matches!(
        (host, shortcut),
        (HostPlatform::Macos, "Fn") | (HostPlatform::Windows, "Control+Super")
    )
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
                if let Ok(mut press) = shell.dictation_press.lock() {
                    press.started = Some(Instant::now());
                    press.released_ms = None;
                }
                let cancel_epoch = shell.dictation_cancel_epoch.load(Ordering::Acquire);
                let dictation = core.settings().ok().map(|settings| settings.dictation);
                let (tap_enabled, hold_enabled, threshold_ms) = dictation
                    .as_ref()
                    .map(|dictation| {
                        (
                            dictation.tap_enabled,
                            dictation.hold_enabled,
                            dictation.hold_threshold_ms,
                        )
                    })
                    .unwrap_or((true, true, 350));
                if !tap_enabled && !hold_enabled {
                    return;
                }
                if begin_dictation(&app, &core).await.is_ok()
                    && !shell.dictation_held.load(Ordering::Acquire)
                {
                    if shell.dictation_cancel_epoch.load(Ordering::Acquire) != cancel_epoch {
                        let _ = core.cancel_dictation().await;
                        unregister_cancel_shortcut(&app);
                        sync_idle_flow_bar(&app);
                        return;
                    }
                    // ponytail: a release before the mic is ready defers to the
                    // tap/hold settings instead of always stopping an empty
                    // session (which surfaced as "No speech was detected").
                    let released_ms = shell
                        .dictation_press
                        .lock()
                        .ok()
                        .and_then(|press| press.released_ms)
                        .unwrap_or(u64::MAX);
                    match press_outcome(
                        press_kind(released_ms, threshold_ms),
                        tap_enabled,
                        hold_enabled,
                    ) {
                        PressOutcome::Finish => {
                            let _ = finish_dictation(&app, &core).await;
                        }
                        PressOutcome::Cancel => {
                            let _ = core.cancel_dictation().await;
                            unregister_cancel_shortcut(&app);
                            sync_idle_flow_bar(&app);
                        }
                        PressOutcome::Stay => {}
                    }
                }
            }
            HoldShortcutEvent::Released => {
                shell.dictation_held.store(false, Ordering::Release);
                let elapsed_ms = shell
                    .dictation_press
                    .lock()
                    .ok()
                    .map(|mut press| {
                        let elapsed = press.started.map(|started| {
                            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
                        });
                        press.released_ms = elapsed;
                        elapsed.unwrap_or(u64::MAX)
                    })
                    .unwrap_or(u64::MAX);
                let dictation = core.settings().ok().map(|settings| settings.dictation);
                let (tap_enabled, hold_enabled, threshold_ms) = dictation
                    .as_ref()
                    .map(|dictation| {
                        (
                            dictation.tap_enabled,
                            dictation.hold_enabled,
                            dictation.hold_threshold_ms,
                        )
                    })
                    .unwrap_or((true, true, 350));
                if press_outcome(
                    press_kind(elapsed_ms, threshold_ms),
                    tap_enabled,
                    hold_enabled,
                ) == PressOutcome::Finish
                {
                    let _ = finish_dictation(&app, &core).await;
                }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PressKind {
    Tap,
    Hold,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PressOutcome {
    Stay,
    Finish,
    Cancel,
}

fn press_kind(elapsed_ms: u64, threshold_ms: u64) -> PressKind {
    if elapsed_ms < threshold_ms {
        PressKind::Tap
    } else {
        PressKind::Hold
    }
}

fn press_outcome(kind: PressKind, tap_enabled: bool, hold_enabled: bool) -> PressOutcome {
    match kind {
        PressKind::Tap if tap_enabled => PressOutcome::Stay,
        PressKind::Tap => PressOutcome::Cancel,
        PressKind::Hold if hold_enabled => PressOutcome::Finish,
        PressKind::Hold => PressOutcome::Stay,
    }
}

fn register_writing_shortcut(
    app: &AppHandle,
    shell: &ShellState,
    shortcut: &str,
) -> Result<(), PlatformError> {
    let normalized = normalize_accelerator(shortcut);
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
) -> Result<(), SettingsRuntimeError> {
    // Shortcuts first: they are the most likely step to fail (conflicts),
    // and failing before the autostart toggle keeps the one irreversible
    // side effect untouched on the error path.
    // Independent like `register_shortcuts`: one conflicting shortcut must
    // not block the other from (re-)registering.
    let writing =
        register_writing_shortcut(app, &app.state::<ShellState>(), &settings.writing_shortcut);
    let dictation = register_dictation_shortcut(
        app,
        &app.state::<ShellState>(),
        &settings.dictation_shortcut,
    );
    writing.map_err(|_| SettingsRuntimeError::ShortcutUnavailable)?;
    if let Err(error) = dictation
        && error.kind != PlatformErrorKind::PermissionDenied
    {
        return Err(SettingsRuntimeError::ShortcutUnavailable);
    }
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
    if app
        .state::<AppCore>()
        .dictation_phase()
        .ok()
        .is_some_and(|phase| matches!(phase, DictationPhase::Hidden | DictationPhase::Success))
    {
        if settings.show_idle_flow_bar {
            emit_dictation(app, "idle", None, false);
            let _ = show_surface(app, "flow-bar", false);
        } else {
            hide_surface(app, "flow-bar");
        }
    }
    apply_theme(app, &settings.theme);
    Ok(())
}

/// Best-effort counterpart to [`apply_settings`] for the rollback path: runs
/// every step, ignores individual failures, and never restores forward, so a
/// failing rollback cannot reintroduce the rejected settings.
pub(crate) fn restore_settings(app: &AppHandle, settings: &FrontendSettings) {
    let shell = app.state::<ShellState>();
    let _ = register_writing_shortcut(app, &shell, &settings.writing_shortcut);
    let _ = register_dictation_shortcut(app, &shell, &settings.dictation_shortcut);
    let autolaunch = app.autolaunch();
    if let Ok(enabled) = autolaunch.is_enabled()
        && enabled != settings.launch_at_login
    {
        let _ = if settings.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
    }
    if app
        .state::<AppCore>()
        .dictation_phase()
        .ok()
        .is_some_and(|phase| matches!(phase, DictationPhase::Hidden | DictationPhase::Success))
    {
        if settings.show_idle_flow_bar {
            emit_dictation(app, "idle", None, false);
            let _ = show_surface(app, "flow-bar", false);
        } else {
            hide_surface(app, "flow-bar");
        }
    }
    apply_theme(app, &settings.theme);
}

pub(crate) fn apply_theme(app: &AppHandle, theme: &str) {
    for label in ["settings", "onboarding", "writing-tools", "flow-bar"] {
        if let Some(window) = app.get_webview_window(label) {
            let theme = match theme {
                "light" => Some(tauri::Theme::Light),
                "dark" => Some(tauri::Theme::Dark),
                _ => None,
            };
            let _ = window.set_theme(theme);
            #[cfg(target_os = "windows")]
            apply_caption_theme(
                app,
                label,
                theme.unwrap_or_else(|| window.theme().unwrap_or(tauri::Theme::Light)),
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn apply_caption_theme(app: &AppHandle, label: &str, theme: tauri::Theme) {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DwmSetWindowAttribute,
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{
                SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        },
    };
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        let mut contrast = HIGHCONTRASTW {
            cbSize: size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        let _ = SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let high_contrast = contrast.dwFlags & HCF_HIGHCONTRASTON != Default::default();
        let border: u32 = if high_contrast {
            0xFFFF_FFFF
        } else {
            0xFFFF_FFFE
        };
        let _ = DwmSetWindowAttribute(
            HWND(hwnd.0),
            DWMWA_BORDER_COLOR,
            (&border as *const u32).cast(),
            size_of::<u32>() as u32,
        );
        if !matches!(label, "settings" | "onboarding") {
            return;
        }
        let (caption, text): (u32, u32) = if high_contrast {
            (0xFFFF_FFFF, 0xFFFF_FFFF)
        } else if theme == tauri::Theme::Dark {
            (0x00202020, 0x00F3F3F3)
        } else {
            (0x00F9FCFC, 0x00262624)
        };
        for (attribute, color) in [(DWMWA_CAPTION_COLOR, caption), (DWMWA_TEXT_COLOR, text)] {
            let _ = DwmSetWindowAttribute(
                HWND(hwnd.0),
                attribute,
                (&color as *const u32).cast(),
                size_of::<u32>() as u32,
            );
        }
    }
}

pub(crate) async fn begin_dictation(app: &AppHandle, core: &AppCore) -> Result<(), CommandError> {
    if app.state::<ShellState>().paused() {
        return Ok(());
    }
    if matches!(
        core.dictation_phase().map_err(CommandError::from)?,
        DictationPhase::Starting | DictationPhase::Listening | DictationPhase::Processing
    ) {
        return Ok(());
    }
    show_surface(app, "flow-bar", false).map_err(platform_command_error)?;
    emit_dictation(app, "starting", None, false);
    register_cancel_shortcut(app);
    let generation = core.dictation_generation() + 1;
    let event_app = app.clone();
    let events: SpeechEventSink = Arc::new(move |event| {
        if event_app.state::<AppCore>().dictation_generation() != generation {
            return;
        }
        match event {
            SpeechEvent::AudioLevel(level) => {
                let _ = event_app.emit_to("flow-bar", "dictation-level", level);
            }
            SpeechEvent::SpeechDetected => {}
        }
    });
    if let Err(error) = core.begin_dictation(events).await {
        if core.dictation_generation() != generation
            || matches!(
                error,
                crate::commands::AppCoreError::Speech(crate::speech::SpeechError::AlreadyRunning)
            )
        {
            return Ok(());
        }
        unregister_cancel_shortcut(app);
        let error = CommandError::from(error);
        emit_dictation(app, "error", Some(&error.message), error.recoverable);
        return Err(error);
    }
    if core.dictation_phase().ok() != Some(DictationPhase::Listening) {
        return Ok(());
    }
    emit_dictation(app, "listening", None, false);
    play_dictation_feedback(core, FeedbackMoment::Start);
    Ok(())
}

pub(crate) async fn open_writing_tools(app: &AppHandle) -> Result<(), CommandError> {
    let core = app.state::<AppCore>();
    // Native capture must finish while the original application's control
    // still owns focus. Both UIA and macOS Accessibility depend on this.
    match core.open_writing_tools().await {
        Ok(context) => {
            // Size before positioning: placement clamps against the real
            // window frame, so the order matters.
            size_writing_surface(app, "menu", None).map_err(platform_command_error)?;
            position_writing_surface(app, context.cursor, context.anchor);
            show_surface(app, "writing-tools", true).map_err(platform_command_error)?;
            let _ = app.emit_to(
                "writing-tools",
                "writing-context",
                WritingContextEvent {
                    has_selection: context.has_selection,
                    application_name: context.application.display_name,
                    can_replace: context.has_selection,
                    bounds: context.anchor,
                    initial_text: context.initial_text,
                },
            );
            Ok(())
        }
        Err(crate::commands::AppCoreError::WritingCancelled) => Ok(()),
        Err(error) => {
            size_writing_surface(app, "error", None).map_err(platform_command_error)?;
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
    let generation = core.dictation_generation();
    emit_dictation(app, "processing", None, false);
    let result = core.finish_dictation().await;
    if core.dictation_generation() != generation {
        return Ok(());
    }
    unregister_cancel_shortcut(app);
    let _ = app.emit_to("settings", "recovery-changed", ());
    match result {
        Ok(DictationPhase::Hidden) => {
            sync_idle_flow_bar(app);
            Ok(())
        }
        Ok(_) => {
            play_dictation_feedback(core, FeedbackMoment::Finish);
            emit_dictation(app, "success", None, false);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(620)).await;
                let core = app.state::<AppCore>();
                if core.dictation_generation() == generation
                    && core.dictation_phase().ok() == Some(DictationPhase::Success)
                {
                    sync_idle_flow_bar(&app);
                }
            });
            Ok(())
        }
        Err(error) => {
            let recovered = matches!(error, crate::commands::AppCoreError::Text(_));
            let error = CommandError::from(error);
            let message = if recovered && core.recovery_text().ok().flatten().is_some() {
                "Text saved. Open Kivo to copy it."
            } else {
                &error.message
            };
            emit_dictation(app, "error", Some(message), error.recoverable);
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
    let _ = app.emit(
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
        "listening" => (164.0, 48.0),
        "processing" | "starting" => (128.0, 48.0),
        "success" => (48.0, 48.0),
        "error" => (380.0, 96.0),
        _ => (40.0, 40.0),
    };
    if let Some(window) = app.get_webview_window("flow-bar") {
        let _ = window.set_size(Size::Logical(LogicalSize::new(width, height)));
        position_flow_bar(app, &window);
    }
}

pub(crate) fn size_writing_surface(
    app: &AppHandle,
    _mode: &str,
    content_height: Option<f64>,
) -> Result<(), PlatformError> {
    let (width, max_height) = writing_popup_size(app);
    let height = content_height
        .filter(|height| height.is_finite())
        .unwrap_or(max_height)
        .clamp(44.0, max_height);
    let window = app.get_webview_window("writing-tools").ok_or_else(|| {
        PlatformError::new(
            PlatformErrorKind::NotFound,
            "size_writing_surface",
            "The Writing Tools window is unavailable.",
        )
    })?;
    window
        .set_size(Size::Logical(LogicalSize::new(width, height)))
        .map_err(|_| window_error("size_writing_surface"))?;
    // Keep the top-left stable between modes; only move to stay on-screen.
    if content_height.is_some()
        && let (Ok(mut position), Ok(size)) = (window.outer_position(), window.outer_size())
    {
        clamp_to_monitor_on(&window, &mut position, size, None);
        let _ = window.set_position(Position::Physical(position));
    }
    Ok(())
}

fn writing_popup_size(app: &AppHandle) -> (f64, f64) {
    app.try_state::<AppCore>()
        .and_then(|core| core.settings().ok())
        .map(|settings| {
            (
                settings.writing_tools.popup_width,
                settings.writing_tools.popup_height,
            )
        })
        .unwrap_or((WRITING_DEFAULT_WIDTH, WRITING_DEFAULT_HEIGHT))
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
    // Cursor-only on purpose: the bar is bottom-centered on the active
    // monitor, so the AX selection query only added latency here.
    let cursor = app
        .try_state::<Arc<PlatformServices>>()
        .and_then(|platform| platform.cursor_position().ok());
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
    let work_area = monitor.work_area();
    let size = &work_area.size;
    let origin = &work_area.position;
    let Ok(window_size) = window.outer_size() else {
        return;
    };
    let x = origin.x + (size.width.saturating_sub(window_size.width) / 2) as i32;
    let y = origin.y
        + size
            .height
            .saturating_sub(window_size.height + (16.0 * monitor.scale_factor()) as u32)
            as i32;
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

    // Unsupported hosts have no feedback sound; keep the argument used so the
    // early return above is never the final statement (silences
    // clippy::needless_return on targets where both blocks above are compiled
    // out).
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = moment;
}

pub(crate) fn position_writing_surface(
    app: &AppHandle,
    cursor: Option<crate::text::ScreenPoint>,
    anchor: Option<crate::text::ScreenRect>,
) {
    let Some(window) = app.get_webview_window("writing-tools") else {
        return;
    };
    let Ok(window_size) = window.outer_size() else {
        return;
    };
    let (anchor_mode, fixed_x, fixed_y) = writing_popup_placement(app);

    // "fixed" pins the top-left corner exactly (clamped on-screen); every
    // other mode anchors a point and offsets below the cursor/selection.
    if anchor_mode == "fixed" {
        let mut position = PhysicalPosition::new(fixed_x.round() as i32, fixed_y.round() as i32);
        // Clamp against the monitor holding the fixed point so a position
        // saved on a secondary display is not dragged to the primary one.
        let fixed_monitor = monitor_containing(&window, fixed_x, fixed_y);
        clamp_to_monitor_on(&window, &mut position, window_size, fixed_monitor);
        let _ = window.set_position(Position::Physical(position));
        return;
    }

    let selection_point =
        anchor.map(|bounds| (bounds.x + bounds.width / 2.0, bounds.y + bounds.height));
    let Some((point_x, point_y)) = (match anchor_mode.as_str() {
        "selection" => selection_point.or_else(|| cursor.map(|point| (point.x, point.y))),
        _ => cursor.map(|point| (point.x, point.y)).or(selection_point),
    }) else {
        center_on_monitor(&window, window_size);
        return;
    };

    let monitor = monitor_containing(&window, point_x, point_y)
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work_area = monitor.work_area();
    let origin = &work_area.position;
    let monitor_size = &work_area.size;
    let window_width = window_size.width as f64;
    let window_height = window_size.height as f64;
    // Points arrive in physical pixels; the stored cursor/selection values
    // are converted at capture time, so only rounding happens here.
    let mut x = (point_x - window_width / 2.0).round() as i32;
    let mut y = (point_y + 16.0).round() as i32;
    let right = origin.x + monitor_size.width as i32;
    let bottom = origin.y + monitor_size.height as i32;
    x = x.clamp(
        origin.x + 8,
        (right - window_size.width as i32 - 8).max(origin.x + 8),
    );
    // Flip above the point when there is no room below.
    if y + window_size.height as i32 > bottom - 8 {
        y = (point_y - window_height - 12.0).round() as i32;
    }
    y = y.clamp(
        origin.y + 8,
        (bottom - window_size.height as i32 - 8).max(origin.y + 8),
    );
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

fn writing_popup_placement(app: &AppHandle) -> (String, f64, f64) {
    app.try_state::<AppCore>()
        .and_then(|core| core.settings().ok())
        .map(|settings| {
            let mode = match settings.writing_tools.popup_anchor {
                crate::config::PopupAnchor::Selection => "selection",
                crate::config::PopupAnchor::Fixed => "fixed",
                crate::config::PopupAnchor::Cursor => "cursor",
            }
            .to_owned();
            (
                mode,
                settings.writing_tools.popup_fixed_x,
                settings.writing_tools.popup_fixed_y,
            )
        })
        .unwrap_or(("cursor".to_owned(), 480.0, 320.0))
}

fn monitor_containing(window: &WebviewWindow, x: f64, y: f64) -> Option<tauri::Monitor> {
    window
        .available_monitors()
        .ok()?
        .into_iter()
        .find(|monitor| {
            let origin = monitor.position();
            let size = monitor.size();
            let origin_x = f64::from(origin.x);
            let origin_y = f64::from(origin.y);
            x >= origin_x
                && x < origin_x + f64::from(size.width)
                && y >= origin_y
                && y < origin_y + f64::from(size.height)
        })
}

fn clamp_to_monitor_on(
    window: &WebviewWindow,
    position: &mut PhysicalPosition<i32>,
    window_size: tauri::PhysicalSize<u32>,
    preferred: Option<tauri::Monitor>,
) {
    let monitor = preferred
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work_area = monitor.work_area();
    let origin = &work_area.position;
    let monitor_size = &work_area.size;
    position.x = position.x.clamp(
        origin.x + 8,
        (origin.x + monitor_size.width as i32 - window_size.width as i32 - 8).max(origin.x + 8),
    );
    position.y = position.y.clamp(
        origin.y + 8,
        (origin.y + monitor_size.height as i32 - window_size.height as i32 - 8).max(origin.y + 8),
    );
}

fn center_on_monitor(window: &WebviewWindow, window_size: tauri::PhysicalSize<u32>) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work_area = monitor.work_area();
    let origin = &work_area.position;
    let monitor_size = &work_area.size;
    let x = origin.x + (monitor_size.width.saturating_sub(window_size.width) / 2) as i32;
    let y = origin.y + (monitor_size.height.saturating_sub(window_size.height) / 2) as i32;
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

pub(crate) fn hide_surface(app: &AppHandle, surface: &str) {
    if let Some(window) = app.get_webview_window(surface) {
        let _ = window.hide();
    }
}

pub(crate) fn open_permission_settings(permission: PermissionKind) -> Result<(), PlatformError> {
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
    let url = match permission {
        // Desktop SAPI uses installed speech languages, not online speech consent.
        PermissionKind::Microphone => "ms-settings:privacy-microphone",
        _ => "ms-settings:speech",
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = permission;
        Err(PlatformError::new(
            PlatformErrorKind::Unsupported,
            "open_permission_settings",
            "System settings pages are only available on macOS and Windows.",
        ))
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
    {
        use windows::{
            Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
            core::{PCWSTR, w},
        };
        let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(url.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize > 32 {
            Ok(())
        } else {
            Err(window_error("open_url"))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    #[cfg(not(target_os = "windows"))]
    command.spawn().map(|_| ()).map_err(|_| {
        PlatformError::new(
            PlatformErrorKind::Os,
            "open_url",
            "The system settings page could not be opened.",
        )
    })
}

pub(crate) fn copy_text(app: &AppHandle, text: &str) -> Result<(), PlatformError> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::{
            Foundation::{GlobalFree, HANDLE, HWND},
            System::{
                DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
                Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
                Ole::CF_UNICODETEXT,
            },
        };
        let window = app
            .get_webview_window("settings")
            .ok_or_else(clipboard_error)?;
        let hwnd = window.hwnd().map_err(|_| clipboard_error())?;
        let owner = Some(HWND(hwnd.0));
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let memory =
                GlobalAlloc(GMEM_MOVEABLE, text.len() * 2).map_err(|_| clipboard_error())?;
            let locked = GlobalLock(memory) as *mut u16;
            if locked.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(clipboard_error());
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), locked, text.len());
            let _ = GlobalUnlock(memory);
            // The clipboard is often held briefly by another app (clipboard
            // managers, RDP, Office). Retry instead of failing immediately.
            // Fall back to no owner window so a missing settings HWND can
            // never break copying on its own.
            let mut opened = false;
            for _ in 0..5 {
                if OpenClipboard(owner).is_ok() {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            if !opened && OpenClipboard(None).is_err() {
                let _ = GlobalFree(Some(memory));
                return Err(clipboard_busy_error());
            }
            let result = EmptyClipboard().and_then(|_| {
                SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0)))
            });
            let _ = CloseClipboard();
            if result.is_err() {
                let _ = GlobalFree(Some(memory));
                return Err(clipboard_error());
            }
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        // Absolute path: GUI-launched apps inherit a sparse PATH where a bare
        // `pbcopy` lookup can fail.
        let mut child = Command::new("/usr/bin/pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|_| clipboard_error())?;
        child
            .stdin
            .take()
            .ok_or_else(clipboard_error)?
            .write_all(text.as_bytes())
            .map_err(|_| clipboard_error())?;
        if !child.wait().map_err(|_| clipboard_error())?.success() {
            return Err(clipboard_error());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, text);
        Err(clipboard_error())
    }
}

fn clipboard_error() -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Os,
        "copy_text",
        "The result could not be copied.",
    )
}

#[cfg(target_os = "windows")]
fn clipboard_busy_error() -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Os,
        "copy_text",
        "The clipboard is busy. Try copying again.",
    )
}

pub(crate) async fn check_for_updates(app: &AppHandle) -> Result<UpdateResult, CommandError> {
    #[derive(serde::Deserialize)]
    struct GitHubRelease {
        tag_name: String,
        html_url: Option<String>,
    }

    // Rolling beta manifest published by the CI `publish-continuous` job as
    // `continuous.json` on the `continuous` pre-release. `builtAt` and any
    // future fields are ignored: only `version` + `sha` drive detection.
    #[derive(serde::Deserialize)]
    struct ContinuousManifest {
        version: String,
        #[serde(default)]
        sha: Option<String>,
    }

    let current_version = app.package_info().version.to_string();
    let current_sha = current_build_sha();

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|_| update_check_error())?;
    let user_agent = "Kivo desktop updater";

    // 1. Prefer the latest stable release. When any stable release exists the
    // rolling beta is obsolete by definition, so return here without checking
    // `continuous` — stable installs never get dragged onto a beta.
    // A 404 just means no stable release has been published yet.
    let stable = client
        .get("https://api.github.com/repos/0libote/Kivo/releases/latest")
        .header("accept", "application/vnd.github+json")
        .header("user-agent", user_agent)
        .send()
        .await
        .map_err(|_| update_check_error())?;
    if stable.status().is_success() {
        let release = stable
            .json::<GitHubRelease>()
            .await
            .map_err(|_| update_check_error())?;
        let available_version = stable_version_from_tag(&release.tag_name);
        // A mistagged stable release (tag not starting with `app-v`) must not
        // read as "up to date": surface it as a check failure instead.
        let available_version = available_version.ok_or_else(update_check_error)?;
        let available = version_is_newer(&available_version, &current_version);
        return Ok(UpdateResult {
            current_version,
            available_version: Some(available_version),
            available,
            download_url: Some(
                release
                    .html_url
                    .unwrap_or_else(|| STABLE_RELEASES_URL.to_owned()),
            ),
            channel: Some("stable".into()),
            current_sha,
            available_sha: None,
        });
    } else if stable.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(update_check_error());
    }

    // 2. No stable release yet: fall back to the rolling `continuous`
    // pre-release. The version number rarely changes between betas, so a
    // same-version build with a different commit SHA is still an update.
    let continuous = client
        .get("https://github.com/0libote/Kivo/releases/download/continuous/continuous.json")
        .header("accept", "application/json")
        .header("user-agent", user_agent)
        .send()
        .await
        .map_err(|_| update_check_error())?;
    if continuous.status().is_success() {
        let manifest = continuous
            .json::<ContinuousManifest>()
            .await
            .map_err(|_| update_check_error())?;
        let available = beta_is_newer(
            current_sha.as_deref(),
            manifest.sha.as_deref(),
            &manifest.version,
            &current_version,
        );
        return Ok(UpdateResult {
            current_version,
            available_version: Some(manifest.version),
            available,
            download_url: Some(CONTINUOUS_RELEASE_URL.to_owned()),
            channel: Some("beta".into()),
            current_sha,
            available_sha: manifest.sha,
        });
    }
    if continuous.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(update_check_error());
    }

    // No stable release and no continuous pre-release yet.
    Ok(UpdateResult {
        current_version,
        available_version: None,
        available: false,
        download_url: None,
        channel: None,
        current_sha,
        available_sha: None,
    })
}

const STABLE_RELEASES_URL: &str = "https://github.com/0libote/Kivo/releases/latest";
const CONTINUOUS_RELEASE_URL: &str = "https://github.com/0libote/Kivo/releases/tag/continuous";

/// Commit SHA stamped into CI beta builds via `KIVO_BUILD_SHA`. Local builds
/// have none, which the beta comparison treats as "definitely not the latest
/// beta" so developers still get offered the download.
fn current_build_sha() -> Option<String> {
    option_env!("KIVO_BUILD_SHA")
        .map(str::to_owned)
        .filter(|sha| !sha.is_empty())
}

fn update_check_error() -> CommandError {
    CommandError {
        code: "update_check_failed".into(),
        message: "Kivo couldn’t check for updates right now.".into(),
        recoverable: true,
    }
}

/// Install the pending stable update in-app via the Tauri updater plugin
/// (signed artifacts from `latest.json`). Only offered for the stable
/// channel: beta builds are ad-hoc signed and local builds carry no trusted
/// updater key, so those keep the manual GitHub download. Windows takes the
/// same path once its release job publishes updater artifacts; until then
/// the plugin reports no installable update and callers fall back to the
/// download link. Identical Rust on both desktops; platform differences
/// (installer exit on Windows vs. relaunch on macOS) are handled by the
/// plugin and the explicit `restart_app` step below.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), CommandError> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|_| {
        update_install_error(
            "update_install_unavailable",
            "This build can’t install updates itself. Use the download link instead.",
        )
    })?;
    let update = updater
        .check()
        .await
        .map_err(|_| {
            update_install_error(
                "update_install_unavailable",
                "No installable update was found. Use the download link instead.",
            )
        })?
        .ok_or_else(|| {
            update_install_error("update_not_available", "Kivo is already up to date.")
        })?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|_| {
            update_install_error(
                "update_install_failed",
                "The update couldn’t be installed. Use the download link instead.",
            )
        })?;
    Ok(())
}

/// Relaunch after an in-app install. The Windows installer exits the app
/// itself; on macOS the user finishes the update with this restart.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

fn update_install_error(code: &str, message: &str) -> CommandError {
    CommandError {
        code: code.into(),
        message: message.into(),
        recoverable: code != "update_not_available",
    }
}

fn stable_version_from_tag(tag: &str) -> Option<String> {
    tag.strip_prefix("app-v").map(str::to_owned)
}

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

/// Whether the rolling beta described by the manifest is newer than the
/// running build. A bumped version is always newer; otherwise any commit
/// difference counts (same-version rebuilds are the normal beta case). A
/// build without an embedded SHA (local dev) is treated as outdated whenever
/// a beta manifest exists so the download is still offered.
fn beta_is_newer(
    current_sha: Option<&str>,
    manifest_sha: Option<&str>,
    manifest_version: &str,
    current_version: &str,
) -> bool {
    if version_is_newer(manifest_version, current_version) {
        return true;
    }
    match (current_sha, manifest_sha) {
        (Some(current), Some(manifest)) => current != manifest,
        (None, Some(_)) => true,
        _ => false,
    }
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
    use super::{
        PressKind, PressOutcome, beta_is_newer, is_native_dictation_shortcut_for,
        normalize_accelerator, press_kind, press_outcome, stable_version_from_tag,
        version_is_newer,
    };
    use crate::config::HostPlatform;

    #[test]
    fn accelerator_normalization_maps_whole_modifier_tokens_only() {
        assert_eq!(normalize_accelerator("Ctrl+Space"), "Control+Space");
        assert_eq!(normalize_accelerator("Ctrl+Meta"), "Control+Super");
        assert_eq!(normalize_accelerator("Fn"), "Fn");
        // Substring content must survive: only full `+`-separated tokens map.
        assert_eq!(normalize_accelerator("Ctrlled+F1"), "Ctrlled+F1");
        assert_eq!(normalize_accelerator("MetaFoo"), "MetaFoo");
    }

    #[test]
    fn native_dictation_shortcuts_are_os_exclusive_on_both_hosts() {
        // Runs on every CI platform: a Windows-only shortcut must never be
        // treated as native on macOS and vice versa.
        assert!(is_native_dictation_shortcut_for("Fn", HostPlatform::Macos));
        assert!(!is_native_dictation_shortcut_for(
            "Control+Super",
            HostPlatform::Macos
        ));
        assert!(!is_native_dictation_shortcut_for(
            "Fn",
            HostPlatform::Windows
        ));
        assert!(is_native_dictation_shortcut_for(
            "Control+Super",
            HostPlatform::Windows
        ));
        // Portable shortcuts always go through the global-shortcut plugin.
        for host in [
            HostPlatform::Macos,
            HostPlatform::Windows,
            HostPlatform::Other,
        ] {
            assert!(!is_native_dictation_shortcut_for("Ctrl+Alt+D", host));
            assert!(!is_native_dictation_shortcut_for("Control+Space", host));
        }
    }

    #[test]
    fn github_release_versions_are_compared_without_lexical_ordering() {
        assert!(version_is_newer("1.10.0", "1.9.9"));
        assert!(!version_is_newer("1.2.3", "1.2.3"));
        assert!(!version_is_newer("not-a-version", "1.2.3"));
    }

    #[test]
    fn shortcut_presses_split_into_taps_and_holds() {
        assert_eq!(press_kind(0, 350), PressKind::Tap);
        assert_eq!(press_kind(349, 350), PressKind::Tap);
        assert_eq!(press_kind(350, 350), PressKind::Hold);
        // Taps toggle (stay listening) while holds finish on release.
        assert_eq!(
            press_outcome(PressKind::Tap, true, true),
            PressOutcome::Stay
        );
        assert_eq!(
            press_outcome(PressKind::Hold, true, true),
            PressOutcome::Finish
        );
        // A disabled gesture never acts: taps vanish, holds become taps.
        assert_eq!(
            press_outcome(PressKind::Tap, false, true),
            PressOutcome::Cancel
        );
        assert_eq!(
            press_outcome(PressKind::Hold, true, false),
            PressOutcome::Stay
        );
    }

    #[test]
    fn stable_tags_resolve_to_a_version_while_continuous_does_not() {
        assert_eq!(
            stable_version_from_tag("app-v1.2.3"),
            Some("1.2.3".to_owned())
        );
        assert_eq!(stable_version_from_tag("continuous"), None);
    }

    #[test]
    fn same_version_beta_with_a_new_commit_is_an_update() {
        // The normal rolling-beta case: version never bumps, SHA changes.
        assert!(beta_is_newer(Some("aaa"), Some("bbb"), "0.1.0", "0.1.0"));
        assert!(!beta_is_newer(Some("aaa"), Some("aaa"), "0.1.0", "0.1.0"));
    }

    #[test]
    fn beta_version_bumps_win_regardless_of_sha() {
        assert!(beta_is_newer(Some("aaa"), Some("aaa"), "0.2.0", "0.1.0"));
        assert!(beta_is_newer(None, None, "0.2.0", "0.1.0"));
    }

    #[test]
    fn local_builds_without_a_sha_are_offered_the_beta() {
        assert!(beta_is_newer(None, Some("bbb"), "0.1.0", "0.1.0"));
        // Without a manifest SHA there is nothing commit-based to compare.
        assert!(!beta_is_newer(Some("aaa"), None, "0.1.0", "0.1.0"));
        assert!(!beta_is_newer(None, None, "0.1.0", "0.1.0"));
    }
}
