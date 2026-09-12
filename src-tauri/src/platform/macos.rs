use std::{
    ffi::{CString, c_char, c_double, c_long, c_void},
    io::Write,
    process::{Command, Stdio},
    ptr::{self, NonNull},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::security::{CredentialError, CredentialStore, SecretString};

use super::*;

type CfTypeRef = *const c_void;
type CfStringRef = *const c_void;
type CfDictionaryRef = *const c_void;
type AxUiElementRef = *const c_void;
type OsStatus = i32;

const UTF8: u32 = 0x0800_0100;
const AX_SUCCESS: i32 = 0;
const AX_ERROR_API_DISABLED: i32 = -25211;
const ERR_SEC_SUCCESS: OsStatus = 0;
const ERR_SEC_ITEM_NOT_FOUND: OsStatus = -25300;
const ERR_SEC_AUTH_FAILED: OsStatus = -25293;
const AX_VALUE_CGRECT: i32 = 3;
const FN_FLAG: u64 = 0x0080_0000;
const COMMAND_FLAG: u64 = 0x0010_0000;
const CG_SESSION_EVENT_TAP: u32 = 1;
const KEYCODE_C: u16 = 0x08;
const KEYCODE_V: u16 = 0x09;
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(50);
const EVENT_FLAGS_CHANGED: u32 = 12;
const EVENT_KEY_DOWN: u32 = 10;
const EVENT_KEYCODE_FIELD: u32 = 9;
const ESCAPE_KEYCODE: i64 = 53;

static NEXT_SELECTION_TOKEN: AtomicU64 = AtomicU64::new(1);

pub(super) struct PlatformImpl {
    credentials: MacCredentialStore,
}

impl PlatformImpl {
    pub(super) fn new() -> PlatformResult<Self> {
        Ok(Self {
            credentials: MacCredentialStore,
        })
    }

    pub(super) fn credential_store(&self) -> &dyn CredentialStore {
        &self.credentials
    }

    pub(super) fn get_selected_text(&self) -> PlatformResult<SelectionSnapshot> {
        ensure_accessibility("get_selected_text")?;
        let (application, element) = focused_ax_elements("get_selected_text")?;
        let text_value =
            copy_ax_attribute(element.as_ptr(), "AXSelectedText", "get_selected_text")?;
        let text = cf_string_to_string(text_value.as_ptr(), "get_selected_text")?;
        if text.is_empty() {
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                "get_selected_text",
                "Select some text first.",
            ));
        }

        let owner = active_application_from_ax(application.as_ptr(), "get_selected_text")?;
        let bounds = selection_bounds(element.as_ptr()).into_iter().collect();

        Ok(SelectionSnapshot {
            text,
            bounds,
            owner,
            strategy: crate::text::TextAccessStrategy::Accessibility,
            native_token: NEXT_SELECTION_TOKEN.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Clipboard-fallback capture. Must run before the popup takes focus so
    /// the simulated Cmd+C lands in the user's application. Blocking.
    pub(super) fn capture_selection_via_clipboard(&self) -> PlatformResult<SelectionSnapshot> {
        ensure_accessibility("capture_selection_via_clipboard")?;
        // Identify the owner while its window is still focused; the popup
        // will steal focus immediately after this returns.
        let owner = focused_application().unwrap_or_else(|_| ActiveApplication {
            process_id: 0,
            name: "Unknown Application".to_owned(),
            identifier: None,
            native_handle: 0,
        });
        // Best-effort AX bounds hint for placement; absence is fine because
        // the popup anchors to the cursor.
        let bounds = focused_ax_elements("capture_selection_via_clipboard")
            .ok()
            .and_then(|(_, element)| selection_bounds(element.as_ptr()))
            .into_iter()
            .collect();

        let backup = clipboard_text().ok();
        let before = pasteboard_change_count();
        send_command_keystroke(KEYCODE_C, "capture_selection_via_clipboard")?;

        let changed = wait_for_pasteboard_change(before, std::time::Duration::from_secs(2));
        // Always restore silently when we touched nothing observable.
        if !changed {
            restore_clipboard_backup(backup.as_deref(), before);
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                "capture_selection_via_clipboard",
                "Select some text first.",
            ));
        }
        let after = pasteboard_change_count();
        let text = clipboard_text().map_err(|_| {
            restore_clipboard_backup(backup.as_deref(), after);
            os_error(
                "capture_selection_via_clipboard",
                "The selected text could not be read.",
            )
        })?;
        restore_clipboard_backup(backup.as_deref(), after);
        if text.trim().is_empty() {
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                "capture_selection_via_clipboard",
                "Select some text first.",
            ));
        }

        Ok(SelectionSnapshot {
            text,
            bounds,
            owner,
            strategy: crate::text::TextAccessStrategy::ClipboardFallback,
            native_token: NEXT_SELECTION_TOKEN.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Paste-based replacement for clipboard-captured selections. Restores the
    /// previous clipboard content silently before returning, on both success
    /// and failure paths after the clipboard was overwritten.
    pub(super) fn paste_replacement(
        &self,
        snapshot: &SelectionSnapshot,
        replacement: &str,
    ) -> PlatformResult<()> {
        ensure_accessibility("paste_replacement")?;
        if snapshot.owner.process_id == 0 {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "paste_replacement",
                "The original application is no longer available.",
            ));
        }
        activate_process(snapshot.owner.process_id, "paste_replacement")?;
        let backup = clipboard_text().ok();
        set_clipboard_text(replacement)?;
        let written = pasteboard_change_count();
        // Let the pasteboard settle before keystrokes land.
        std::thread::sleep(std::time::Duration::from_millis(120));
        if let Err(error) = send_command_keystroke(KEYCODE_V, "paste_replacement") {
            restore_clipboard_backup(backup.as_deref(), written);
            return Err(error);
        }
        // No cross-process paste-completion signal exists; 500ms covers
        // slower targets (Electron, IDEs) while staying responsive.
        std::thread::sleep(std::time::Duration::from_millis(500));
        restore_clipboard_backup(backup.as_deref(), written);
        Ok(())
    }

    pub(super) fn cursor_position(&self) -> PlatformResult<ScreenPoint> {
        let event = unsafe { CGEventCreate(ptr::null()) };
        if event.is_null() {
            return Err(os_error(
                "cursor_position",
                "The mouse position is unavailable.",
            ));
        }
        let location = unsafe { CGEventGetLocation(event) };
        unsafe { CFRelease(event) };
        let (main_height_points, scale) = main_display_metrics()?;
        Ok(quartz_to_physical_top_left(
            location.x,
            location.y,
            main_height_points,
            scale,
        ))
    }

    pub(super) fn replace_selected_text(
        &self,
        snapshot: &SelectionSnapshot,
        replacement: &str,
    ) -> PlatformResult<()> {
        ensure_accessibility("replace_selected_text")?;
        activate_process(snapshot.owner.process_id, "replace_selected_text")?;
        let (application, element) = focused_ax_elements("replace_selected_text")?;
        let pid = ax_process_id(application.as_ptr(), "replace_selected_text")?;
        if pid != snapshot.owner.process_id {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "The original application is no longer focused.",
            ));
        }

        let current =
            copy_ax_attribute(element.as_ptr(), "AXSelectedText", "replace_selected_text")?;
        let current = cf_string_to_string(current.as_ptr(), "replace_selected_text")?;
        if current != snapshot.text {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "The selection changed before Kivo could replace it.",
            ));
        }

        set_ax_string_attribute(
            element.as_ptr(),
            "AXSelectedText",
            replacement,
            "replace_selected_text",
        )
    }

    pub(super) fn insert_text_at_cursor(&self, text: &str) -> PlatformResult<()> {
        ensure_accessibility("insert_text_at_cursor")?;
        let (_, element) = focused_ax_elements("insert_text_at_cursor")?;
        set_ax_string_attribute(
            element.as_ptr(),
            "AXSelectedText",
            text,
            "insert_text_at_cursor",
        )
    }

    pub(super) fn permission_status(
        &self,
        permission: PermissionKind,
    ) -> PlatformResult<PermissionStatus> {
        let status = match permission {
            PermissionKind::Accessibility => unsafe {
                if AXIsProcessTrusted() {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Denied
                }
            },
            PermissionKind::InputMonitoring => unsafe {
                if CGPreflightListenEventAccess() {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Denied
                }
            },
            PermissionKind::Microphone => {
                permission_from_apple_status(unsafe { kivo_microphone_authorization_status() })
            }
            PermissionKind::SpeechRecognition => {
                permission_from_apple_status(unsafe { kivo_speech_authorization_status() })
            }
        };
        Ok(status)
    }

    pub(super) fn request_permission(&self, permission: PermissionKind) -> PlatformResult<()> {
        match permission {
            PermissionKind::Accessibility => {
                let prompt_key = unsafe { kAXTrustedCheckOptionPrompt };
                let prompt_value = unsafe { kCFBooleanTrue };
                let options = cf_dictionary(&[(prompt_key, prompt_value)])?;
                unsafe {
                    AXIsProcessTrustedWithOptions(options.as_ptr());
                }
            }
            PermissionKind::InputMonitoring => unsafe {
                if !CGRequestListenEventAccess() {
                    return Err(PlatformError::new(
                        PlatformErrorKind::PermissionDenied,
                        "request_permission",
                        "Input Monitoring access was not granted.",
                    ));
                }
            },
            PermissionKind::Microphone => unsafe {
                kivo_request_microphone_authorization();
            },
            PermissionKind::SpeechRecognition => unsafe {
                kivo_request_speech_authorization();
            },
        }
        Ok(())
    }

    pub(super) fn register_dictation_shortcut(
        &self,
        shortcut: HoldShortcut,
        callback: HoldShortcutCallback,
    ) -> PlatformResult<Box<dyn ShortcutRegistration>> {
        if shortcut.modifiers != [ModifierKey::Function] || shortcut.key_code.is_some() {
            return Err(PlatformError::unsupported(
                "register_dictation_shortcut",
                "The macOS native monitor currently supports the Fn/Globe hold shortcut only.",
            ));
        }
        Ok(Box::new(MacShortcutRegistration::start(
            shortcut.suppress,
            callback,
        )?))
    }

    pub(super) fn style_window(
        &self,
        native_window: usize,
        kind: OverlayKind,
    ) -> PlatformResult<()> {
        unsafe { style_ns_window(native_window as *mut c_void, kind) }
    }

    pub(super) fn start_speech(
        &self,
        options: SpeechOptions,
        callback: SpeechCallback,
    ) -> PlatformResult<Box<dyn SpeechSession>> {
        if options.microphone_id.is_some() {
            return Err(PlatformError::unsupported(
                "start_speech",
                "Selecting a non-default microphone is not supported by the Apple Speech adapter yet.",
            ));
        }

        let language = options
            .language
            .as_deref()
            .map(CString::new)
            .transpose()
            .map_err(|_| {
                PlatformError::new(
                    PlatformErrorKind::InvalidState,
                    "start_speech",
                    "The speech language contains an invalid null character.",
                )
            })?;
        let context = Box::new(SpeechCallbackContext { callback });
        let context = Box::into_raw(context);
        let handle = unsafe {
            kivo_speech_start(
                language
                    .as_ref()
                    .map_or(ptr::null(), |value| value.as_ptr()),
                options.require_on_device,
                Some(speech_callback),
                context.cast(),
            )
        };
        let Some(handle) = NonNull::new(handle) else {
            unsafe {
                drop(Box::from_raw(context));
            }
            return Err(PlatformError::new(
                PlatformErrorKind::Speech,
                "start_speech",
                "Apple Speech could not start. Check microphone and Speech Recognition permissions.",
            ));
        };

        Ok(Box::new(MacSpeechSession {
            handle,
            callback_context: NonNull::new(context).expect("Box pointers are non-null"),
            finished: false,
        }))
    }
}

fn permission_from_apple_status(status: i32) -> PermissionStatus {
    match status {
        0 => PermissionStatus::NotDetermined,
        1 => PermissionStatus::Restricted,
        2 => PermissionStatus::Denied,
        3 => PermissionStatus::Granted,
        _ => PermissionStatus::Unavailable,
    }
}

fn ensure_accessibility(operation: &'static str) -> PlatformResult<()> {
    if unsafe { AXIsProcessTrusted() } {
        Ok(())
    } else {
        Err(PlatformError::new(
            PlatformErrorKind::PermissionDenied,
            operation,
            "Writing Tools needs Accessibility access.",
        ))
    }
}

fn focused_ax_elements(operation: &'static str) -> PlatformResult<(CfOwned, CfOwned)> {
    let system = unsafe { CfOwned::from_create(AXUIElementCreateSystemWide()) }
        .ok_or_else(|| os_error(operation, "Could not access the macOS UI server."))?;
    let application = copy_ax_attribute(system.as_ptr(), "AXFocusedApplication", operation)?;
    let element = copy_ax_attribute(application.as_ptr(), "AXFocusedUIElement", operation)?;
    Ok((application, element))
}

fn active_application_from_ax(
    application: CfTypeRef,
    operation: &'static str,
) -> PlatformResult<ActiveApplication> {
    let pid = ax_process_id(application, operation)?;
    let name = copy_ax_attribute(application, "AXTitle", operation)
        .and_then(|value| cf_string_to_string(value.as_ptr(), operation))
        .unwrap_or_else(|_| format!("Application {pid}"));

    Ok(ActiveApplication {
        process_id: pid,
        name,
        identifier: running_application_string(pid, "bundleIdentifier"),
        native_handle: pid as usize,
    })
}

fn ax_process_id(application: CfTypeRef, operation: &'static str) -> PlatformResult<u32> {
    let mut pid = 0_i32;
    let status = unsafe { AXUIElementGetPid(application, &mut pid) };
    if status == AX_SUCCESS && pid > 0 {
        Ok(pid as u32)
    } else {
        Err(ax_error(operation, status))
    }
}

fn selection_bounds(element: CfTypeRef) -> Option<ScreenRect> {
    let range = copy_ax_attribute(element, "AXSelectedTextRange", "selection_bounds").ok()?;
    let attribute = cf_string("AXBoundsForRange").ok()?;
    let mut value = ptr::null();
    let status = unsafe {
        AXUIElementCopyParameterizedAttributeValue(
            element,
            attribute.as_ptr(),
            range.as_ptr(),
            &mut value,
        )
    };
    if status != AX_SUCCESS {
        return None;
    }
    let value = unsafe { CfOwned::from_create(value) }?;
    let mut rect = CgRect::default();
    let copied = unsafe {
        AXValueGetValue(
            value.as_ptr(),
            AX_VALUE_CGRECT,
            (&mut rect as *mut CgRect).cast(),
        )
    };
    if !copied || rect.size.width < 0.0 || rect.size.height < 0.0 {
        return None;
    }
    Some(ScreenRect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    })
}

fn focused_application() -> PlatformResult<ActiveApplication> {
    let (application, _) = focused_ax_elements("capture_selection_via_clipboard")?;
    active_application_from_ax(application.as_ptr(), "capture_selection_via_clipboard")
}

/// Quartz display points (origin at the bottom-left of the primary display)
/// to physical pixels with a top-left origin, matching Tauri monitor space.
/// Per-display scale differences on mixed-scale multi-monitor setups are
/// absorbed later by clamping to the containing monitor.
fn quartz_to_physical_top_left(
    x_quartz: f64,
    y_quartz: f64,
    main_height_points: f64,
    scale: f64,
) -> ScreenPoint {
    ScreenPoint {
        x: x_quartz * scale,
        y: (main_height_points - y_quartz) * scale,
    }
}

fn main_display_metrics() -> PlatformResult<(f64, f64)> {
    let pixels_high = unsafe { CGDisplayPixelsHigh(CGMainDisplayID()) } as f64;
    if pixels_high <= 0.0 {
        return Err(os_error(
            "cursor_position",
            "The display metrics are unavailable.",
        ));
    }
    let scale = unsafe {
        let class = objc_getClass(c"NSScreen".as_ptr());
        if class.is_null() {
            return Err(os_error("cursor_position", "AppKit is unavailable."));
        }
        let screen = msg_send_id(class, sel("mainScreen"));
        if screen.is_null() {
            return Err(os_error(
                "cursor_position",
                "The main screen is unavailable.",
            ));
        }
        msg_send_f64(screen, sel("backingScaleFactor"))
    };
    if !scale.is_finite() || scale <= 0.0 {
        return Err(os_error(
            "cursor_position",
            "The display scale is unavailable.",
        ));
    }
    Ok((pixels_high / scale, scale))
}

fn pasteboard_change_count() -> i64 {
    unsafe {
        let class = objc_getClass(c"NSPasteboard".as_ptr());
        if class.is_null() {
            return -1;
        }
        let board = msg_send_id(class, sel("generalPasteboard"));
        if board.is_null() {
            return -1;
        }
        msg_send_i64(board, sel("changeCount"))
    }
}

fn clipboard_text() -> PlatformResult<String> {
    let output = Command::new("pbpaste")
        .output()
        .map_err(|_| os_error("clipboard", "The clipboard is unavailable."))?;
    if !output.status.success() {
        return Err(os_error("clipboard", "The clipboard could not be read."));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| os_error("clipboard", "The clipboard text was not valid UTF-8."))
}

fn set_clipboard_text(text: &str) -> PlatformResult<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| os_error("clipboard", "The clipboard is unavailable."))?;
    child
        .stdin
        .take()
        .ok_or_else(|| os_error("clipboard", "The clipboard is unavailable."))?
        .write_all(text.as_bytes())
        .map_err(|_| os_error("clipboard", "The clipboard could not be written."))?;
    child
        .wait()
        .map_err(|_| os_error("clipboard", "The clipboard could not be written."))?;
    Ok(())
}

/// Silently restores the pre-capture clipboard text, but only when nothing
/// else changed the pasteboard since (`observed` guards against clobbering a
/// newer copy). A `None` backup means the clipboard held no text; it is left
/// alone so non-text content is never destroyed.
fn restore_clipboard_backup(backup: Option<&str>, observed: i64) {
    let Some(backup) = backup else { return };
    if observed >= 0 && pasteboard_change_count() != observed {
        return;
    }
    let _ = set_clipboard_text(backup);
}

fn wait_for_pasteboard_change(before: i64, timeout: Duration) -> bool {
    if before < 0 {
        return false;
    }
    let deadline = Instant::now() + timeout;
    loop {
        std::thread::sleep(CLIPBOARD_POLL_INTERVAL);
        if pasteboard_change_count() != before {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
    }
}

fn send_command_keystroke(virtual_key: u16, operation: &'static str) -> PlatformResult<()> {
    unsafe {
        let down = CGEventCreateKeyboardEvent(ptr::null(), virtual_key, true);
        let up = CGEventCreateKeyboardEvent(ptr::null(), virtual_key, false);
        if down.is_null() || up.is_null() {
            if !down.is_null() {
                CFRelease(down);
            }
            if !up.is_null() {
                CFRelease(up);
            }
            return Err(os_error(
                operation,
                "The keyboard event could not be created.",
            ));
        }
        CGEventSetFlags(down, COMMAND_FLAG);
        CGEventSetFlags(up, COMMAND_FLAG);
        CGEventPost(CG_SESSION_EVENT_TAP, down);
        CGEventPost(CG_SESSION_EVENT_TAP, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(())
}

fn copy_ax_attribute(
    element: CfTypeRef,
    name: &str,
    operation: &'static str,
) -> PlatformResult<CfOwned> {
    let name = cf_string(name)?;
    let mut value = ptr::null();
    let status = unsafe { AXUIElementCopyAttributeValue(element, name.as_ptr(), &mut value) };
    if status != AX_SUCCESS {
        return Err(ax_error(operation, status));
    }
    unsafe { CfOwned::from_create(value) }
        .ok_or_else(|| os_error(operation, "The accessibility attribute had no value."))
}

fn set_ax_string_attribute(
    element: CfTypeRef,
    name: &str,
    value: &str,
    operation: &'static str,
) -> PlatformResult<()> {
    let name = cf_string(name)?;
    let value = cf_string(value)?;
    let mut settable = false;
    let status = unsafe { AXUIElementIsAttributeSettable(element, name.as_ptr(), &mut settable) };
    if status != AX_SUCCESS {
        return Err(ax_error(operation, status));
    }
    if !settable {
        return Err(PlatformError::unsupported(
            operation,
            "The focused application does not allow direct text replacement.",
        ));
    }
    let status = unsafe { AXUIElementSetAttributeValue(element, name.as_ptr(), value.as_ptr()) };
    if status == AX_SUCCESS {
        Ok(())
    } else {
        Err(ax_error(operation, status))
    }
}

fn activate_process(pid: u32, operation: &'static str) -> PlatformResult<()> {
    unsafe {
        let class = objc_getClass(c"NSRunningApplication".as_ptr());
        if class.is_null() {
            return Err(os_error(operation, "AppKit is unavailable."));
        }
        let app: *mut c_void = msg_send_id_i32(
            class,
            sel("runningApplicationWithProcessIdentifier:"),
            pid as i32,
        );
        if app.is_null() {
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                operation,
                "The original application is no longer running.",
            ));
        }
        let activated = msg_send_bool_u64(app, sel("activateWithOptions:"), 1 << 1);
        if activated {
            Ok(())
        } else {
            Err(os_error(
                operation,
                "macOS did not reactivate the original application.",
            ))
        }
    }
}

fn running_application_string(pid: u32, selector: &str) -> Option<String> {
    unsafe {
        let class = objc_getClass(c"NSRunningApplication".as_ptr());
        if class.is_null() {
            return None;
        }
        let app = msg_send_id_i32(
            class,
            sel("runningApplicationWithProcessIdentifier:"),
            pid as i32,
        );
        if app.is_null() {
            return None;
        }
        let string = msg_send_id(app, sel(selector));
        ns_string_to_string(string)
    }
}

unsafe fn style_ns_window(window: *mut c_void, kind: OverlayKind) -> PlatformResult<()> {
    if window.is_null() {
        return Err(PlatformError::new(
            PlatformErrorKind::InvalidState,
            "style_window",
            "The NSWindow pointer is null.",
        ));
    }

    unsafe {
        msg_send_void_bool(window, sel("setReleasedWhenClosed:"), false);

        match kind {
            OverlayKind::Settings => {}
            OverlayKind::FlowBar | OverlayKind::WritingTools => {
                msg_send_void_bool(window, sel("setOpaque:"), false);
                msg_send_void_bool(window, sel("setHasShadow:"), true);
                msg_send_void_bool(window, sel("setHidesOnDeactivate:"), false);
                msg_send_void_i64(window, sel("setLevel:"), 3);
                let behavior = (1_u64 << 0) | (1_u64 << 6) | (1_u64 << 8);
                msg_send_void_u64(window, sel("setCollectionBehavior:"), behavior);
            }
        }

        if kind == OverlayKind::FlowBar {
            // NSWindowStyleMaskNonactivatingPanel. AppKit honors this for an
            // NSPanel-backed Tauri window; integration must create that window
            // as a panel rather than converting an already-visible window.
            let style = msg_send_u64(window, sel("styleMask"));
            msg_send_void_u64(window, sel("setStyleMask:"), style | (1 << 7));
            if msg_send_bool(
                window,
                sel("respondsToSelector:"),
                sel("setBecomesKeyOnlyIfNeeded:") as *mut c_void,
            ) {
                msg_send_void_bool(window, sel("setBecomesKeyOnlyIfNeeded:"), true);
            }
        }
    }
    Ok(())
}

pub struct MacCredentialStore;

impl CredentialStore for MacCredentialStore {
    fn save_api_key(&self, secret: &SecretString) -> Result<(), CredentialError> {
        let class = unsafe { kSecClass };
        let class_generic = unsafe { kSecClassGenericPassword };
        let attr_service = unsafe { kSecAttrService };
        let attr_account = unsafe { kSecAttrAccount };
        let value_data = unsafe { kSecValueData };
        let synchronizable = unsafe { kSecAttrSynchronizable };
        let false_value = unsafe { kCFBooleanFalse };
        let service = cf_string("com.kivo.desktop").map_err(|_| CredentialError::Backend)?;
        let account = cf_string("gemini-api-key").map_err(|_| CredentialError::Backend)?;
        let data = cf_data(secret.expose().as_bytes()).map_err(|_| CredentialError::Backend)?;

        let query = cf_dictionary(&[
            (class, class_generic),
            (attr_service, service.as_ptr()),
            (attr_account, account.as_ptr()),
        ])
        .map_err(|_| CredentialError::Backend)?;
        let updates = cf_dictionary(&[(value_data, data.as_ptr()), (synchronizable, false_value)])
            .map_err(|_| CredentialError::Backend)?;

        let updated = unsafe { SecItemUpdate(query.as_ptr(), updates.as_ptr()) };
        if updated == ERR_SEC_SUCCESS {
            return Ok(());
        }
        if updated != ERR_SEC_ITEM_NOT_FOUND {
            return Err(keychain_error(updated));
        }

        let item = cf_dictionary(&[
            (class, class_generic),
            (attr_service, service.as_ptr()),
            (attr_account, account.as_ptr()),
            (synchronizable, false_value),
            (value_data, data.as_ptr()),
        ])
        .map_err(|_| CredentialError::Backend)?;
        let status = unsafe { SecItemAdd(item.as_ptr(), ptr::null_mut()) };
        if status == ERR_SEC_SUCCESS {
            Ok(())
        } else {
            Err(keychain_error(status))
        }
    }

    fn load_api_key(&self) -> Result<Option<SecretString>, CredentialError> {
        let service = cf_string("com.kivo.desktop").map_err(|_| CredentialError::Backend)?;
        let account = cf_string("gemini-api-key").map_err(|_| CredentialError::Backend)?;
        let query = cf_dictionary(&[
            (unsafe { kSecClass }, unsafe { kSecClassGenericPassword }),
            (unsafe { kSecAttrService }, service.as_ptr()),
            (unsafe { kSecAttrAccount }, account.as_ptr()),
            (unsafe { kSecReturnData }, unsafe { kCFBooleanTrue }),
            (unsafe { kSecMatchLimit }, unsafe { kSecMatchLimitOne }),
        ])
        .map_err(|_| CredentialError::Backend)?;
        let mut result = ptr::null();
        let status = unsafe { SecItemCopyMatching(query.as_ptr(), &mut result) };
        if status == ERR_SEC_ITEM_NOT_FOUND {
            return Ok(None);
        }
        if status != ERR_SEC_SUCCESS {
            return Err(keychain_error(status));
        }
        let data = unsafe { CfOwned::from_create(result) }.ok_or(CredentialError::Backend)?;
        let length = unsafe { CFDataGetLength(data.as_ptr()) };
        let bytes = unsafe { CFDataGetBytePtr(data.as_ptr()) };
        if length <= 0 || bytes.is_null() {
            return Err(CredentialError::Backend);
        }
        let value = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let value = std::str::from_utf8(value).map_err(|_| CredentialError::Backend)?;
        SecretString::new(value.to_owned())
            .map(Some)
            .map_err(|_| CredentialError::Backend)
    }

    fn clear_api_key(&self) -> Result<(), CredentialError> {
        let service = cf_string("com.kivo.desktop").map_err(|_| CredentialError::Backend)?;
        let account = cf_string("gemini-api-key").map_err(|_| CredentialError::Backend)?;
        let query = cf_dictionary(&[
            (unsafe { kSecClass }, unsafe { kSecClassGenericPassword }),
            (unsafe { kSecAttrService }, service.as_ptr()),
            (unsafe { kSecAttrAccount }, account.as_ptr()),
        ])
        .map_err(|_| CredentialError::Backend)?;
        match unsafe { SecItemDelete(query.as_ptr()) } {
            ERR_SEC_SUCCESS | ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            status => Err(keychain_error(status)),
        }
    }
}

fn keychain_error(status: OsStatus) -> CredentialError {
    match status {
        ERR_SEC_AUTH_FAILED => CredentialError::AccessDenied,
        _ => CredentialError::Backend,
    }
}

struct MacShortcutRegistration {
    run_loop: usize,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MacShortcutRegistration {
    fn start(suppress: bool, callback: HoldShortcutCallback) -> PlatformResult<Self> {
        if !unsafe { CGPreflightListenEventAccess() } {
            return Err(PlatformError::new(
                PlatformErrorKind::PermissionDenied,
                "register_dictation_shortcut",
                "The Fn shortcut needs Input Monitoring access.",
            ));
        }

        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = Arc::clone(&stopped);
        let thread = thread::Builder::new()
            .name("kivo-macos-shortcut".into())
            .spawn(move || unsafe {
                let state = Box::new(MacShortcutState {
                    callback,
                    fn_down: false,
                    active: false,
                    suppress,
                });
                let state = Box::into_raw(state);
                let mask = (1_u64 << EVENT_FLAGS_CHANGED) | (1_u64 << EVENT_KEY_DOWN);
                let tap =
                    CGEventTapCreate(1, 0, 0, mask, Some(mac_event_tap_callback), state.cast());
                if tap.is_null() {
                    drop(Box::from_raw(state));
                    let _ = ready_tx.send(Err(PlatformError::new(
                        PlatformErrorKind::PermissionDenied,
                        "register_dictation_shortcut",
                        "macOS could not create the global Fn event monitor.",
                    )));
                    return;
                }
                let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
                if source.is_null() {
                    CFRelease(tap);
                    drop(Box::from_raw(state));
                    let _ = ready_tx.send(Err(os_error(
                        "register_dictation_shortcut",
                        "macOS could not create the shortcut run loop source.",
                    )));
                    return;
                }
                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                let _ = ready_tx.send(Ok(run_loop as usize));
                CFRunLoopRun();
                thread_stopped.store(true, Ordering::Release);
                CFRunLoopRemoveSource(run_loop, source, kCFRunLoopCommonModes);
                CFRelease(source);
                CFRelease(tap);
                drop(Box::from_raw(state));
            })
            .map_err(|_| {
                os_error(
                    "register_dictation_shortcut",
                    "Could not start the shortcut monitor.",
                )
            })?;

        let run_loop = ready_rx.recv().map_err(|_| {
            os_error(
                "register_dictation_shortcut",
                "The shortcut monitor stopped during startup.",
            )
        })??;
        Ok(Self {
            run_loop,
            stopped,
            thread: Some(thread),
        })
    }
}

impl ShortcutRegistration for MacShortcutRegistration {
    fn stop(&mut self) -> PlatformResult<()> {
        if !self.stopped.swap(true, Ordering::AcqRel) {
            unsafe { CFRunLoopStop(self.run_loop as *const c_void) };
        }
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| {
                os_error(
                    "stop_dictation_shortcut",
                    "The shortcut monitor did not shut down cleanly.",
                )
            })?;
        }
        Ok(())
    }
}

impl Drop for MacShortcutRegistration {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

struct MacShortcutState {
    callback: HoldShortcutCallback,
    fn_down: bool,
    active: bool,
    suppress: bool,
}

extern "C" fn mac_event_tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    context: *mut c_void,
) -> *mut c_void {
    if event.is_null() || context.is_null() {
        return event;
    }
    let state = unsafe { &mut *context.cast::<MacShortcutState>() };

    if event_type == EVENT_FLAGS_CHANGED {
        let now_down = unsafe { CGEventGetFlags(event) } & FN_FLAG != 0;
        let changed = now_down != state.fn_down;
        state.fn_down = now_down;
        if changed && now_down {
            state.active = true;
            (state.callback)(HoldShortcutEvent::Pressed);
        } else if changed && !now_down && state.active {
            state.active = false;
            (state.callback)(HoldShortcutEvent::Released);
        }
        if changed && state.suppress {
            return ptr::null_mut();
        }
    } else if event_type == EVENT_KEY_DOWN && state.active {
        let keycode = unsafe { CGEventGetIntegerValueField(event, EVENT_KEYCODE_FIELD) };
        if keycode == ESCAPE_KEYCODE {
            state.active = false;
            (state.callback)(HoldShortcutEvent::Cancelled);
            if state.suppress {
                return ptr::null_mut();
            }
        }
    }
    event
}

struct SpeechCallbackContext {
    callback: SpeechCallback,
}

extern "C" fn speech_callback(context: *mut c_void, event: i32, value: *const c_char, level: f32) {
    if context.is_null() {
        return;
    }
    let callback = &unsafe { &*context.cast::<SpeechCallbackContext>() }.callback;
    let text = || unsafe {
        if value.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(value)
                .to_string_lossy()
                .into_owned()
        }
    };
    match event {
        0 => callback(SpeechEvent::Listening),
        1 => callback(SpeechEvent::Partial(text())),
        2 => callback(SpeechEvent::Final(text())),
        3 => callback(SpeechEvent::AudioLevel(level.clamp(0.0, 1.0))),
        4 => callback(SpeechEvent::Error(PlatformError::new(
            PlatformErrorKind::Speech,
            "speech_recognition",
            if value.is_null() {
                "Apple Speech recognition failed.".to_owned()
            } else {
                text()
            },
        ))),
        _ => {}
    }
}

struct MacSpeechSession {
    handle: NonNull<c_void>,
    callback_context: NonNull<SpeechCallbackContext>,
    finished: bool,
}

unsafe impl Send for MacSpeechSession {}

impl SpeechSession for MacSpeechSession {
    fn stop(&mut self) -> PlatformResult<()> {
        if !self.finished {
            unsafe { kivo_speech_stop(self.handle.as_ptr()) };
            self.finished = true;
        }
        Ok(())
    }

    fn cancel(&mut self) -> PlatformResult<()> {
        if !self.finished {
            unsafe { kivo_speech_cancel(self.handle.as_ptr()) };
            self.finished = true;
        }
        Ok(())
    }
}

impl Drop for MacSpeechSession {
    fn drop(&mut self) {
        if !self.finished {
            unsafe { kivo_speech_cancel(self.handle.as_ptr()) };
        }
        unsafe {
            kivo_speech_destroy(self.handle.as_ptr());
            drop(Box::from_raw(self.callback_context.as_ptr()));
        }
    }
}

struct CfOwned(CfTypeRef);

impl CfOwned {
    unsafe fn from_create(value: CfTypeRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }

    fn as_ptr(&self) -> CfTypeRef {
        self.0
    }
}

impl Drop for CfOwned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn cf_string(value: &str) -> PlatformResult<CfOwned> {
    let value = CString::new(value).map_err(|_| {
        PlatformError::new(
            PlatformErrorKind::InvalidState,
            "create_native_string",
            "Text contains an invalid null character.",
        )
    })?;
    let string = unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), UTF8) };
    unsafe { CfOwned::from_create(string) }.ok_or_else(|| {
        os_error(
            "create_native_string",
            "macOS could not allocate a native string.",
        )
    })
}

fn cf_string_to_string(value: CfTypeRef, operation: &'static str) -> PlatformResult<String> {
    unsafe {
        if CFGetTypeID(value) != CFStringGetTypeID() {
            return Err(os_error(operation, "The accessibility value was not text."));
        }
        let length = CFStringGetLength(value);
        let capacity = CFStringGetMaximumSizeForEncoding(length, UTF8) + 1;
        let mut buffer = vec![0_u8; capacity.max(1) as usize];
        if !CFStringGetCString(value, buffer.as_mut_ptr().cast(), capacity, UTF8) {
            return Err(os_error(
                operation,
                "macOS could not decode accessibility text.",
            ));
        }
        let nul = buffer
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(buffer.len());
        String::from_utf8(buffer[..nul].to_vec())
            .map_err(|_| os_error(operation, "Accessibility text was not valid UTF-8."))
    }
}

fn cf_data(value: &[u8]) -> PlatformResult<CfOwned> {
    let data = unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as c_long) };
    unsafe { CfOwned::from_create(data) }.ok_or_else(|| {
        os_error(
            "create_keychain_data",
            "macOS could not allocate secure data.",
        )
    })
}

fn cf_dictionary(entries: &[(CfTypeRef, CfTypeRef)]) -> PlatformResult<CfOwned> {
    let keys: Vec<CfTypeRef> = entries.iter().map(|(key, _)| *key).collect();
    let values: Vec<CfTypeRef> = entries.iter().map(|(_, value)| *value).collect();
    let dictionary = unsafe {
        CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            entries.len() as c_long,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    };
    unsafe { CfOwned::from_create(dictionary) }.ok_or_else(|| {
        os_error(
            "create_native_dictionary",
            "macOS could not allocate a dictionary.",
        )
    })
}

fn ax_error(operation: &'static str, code: i32) -> PlatformError {
    let (kind, message) = match code {
        AX_ERROR_API_DISABLED => (
            PlatformErrorKind::PermissionDenied,
            "Writing Tools needs Accessibility access.",
        ),
        -25204 => (
            PlatformErrorKind::Os,
            "The focused application did not respond to Accessibility in time.",
        ),
        -25205 | -25208 | -25212 | -25213 => (
            PlatformErrorKind::Unsupported,
            "The focused application does not expose the required text accessibility information.",
        ),
        _ => (PlatformErrorKind::Os, "macOS Accessibility failed."),
    };
    PlatformError::new(kind, operation, message).with_os_code(code as i64)
}

fn os_error(operation: &'static str, message: impl Into<String>) -> PlatformError {
    PlatformError::new(PlatformErrorKind::Os, operation, message)
}

#[repr(C)]
#[derive(Default)]
struct CgPoint {
    x: c_double,
    y: c_double,
}

#[repr(C)]
#[derive(Default)]
struct CgSize {
    width: c_double,
    height: c_double,
}

#[repr(C)]
#[derive(Default)]
struct CgRect {
    origin: CgPoint,
    size: CgSize,
}

unsafe fn sel(name: &str) -> *const c_void {
    let name = CString::new(name).expect("Objective-C selector has no nulls");
    unsafe { sel_registerName(name.as_ptr()) }
}

unsafe fn ns_string_to_string(value: *mut c_void) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let bytes = unsafe { msg_send_cstr(value, sel("UTF8String")) };
    (!bytes.is_null()).then(|| unsafe {
        std::ffi::CStr::from_ptr(bytes)
            .to_string_lossy()
            .into_owned()
    })
}

unsafe fn msg_send_id(receiver: *mut c_void, selector: *const c_void) -> *mut c_void {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void) -> *mut c_void =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector) }
}

unsafe fn msg_send_id_i32(
    receiver: *mut c_void,
    selector: *const c_void,
    value: i32,
) -> *mut c_void {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void, i32) -> *mut c_void =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector, value) }
}

unsafe fn msg_send_bool(
    receiver: *mut c_void,
    selector: *const c_void,
    value: *mut c_void,
) -> bool {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector, value) != 0 }
}

unsafe fn msg_send_bool_u64(receiver: *mut c_void, selector: *const c_void, value: u64) -> bool {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void, u64) -> i8 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector, value) != 0 }
}

unsafe fn msg_send_void_bool(receiver: *mut c_void, selector: *const c_void, value: bool) {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void, i8) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector, i8::from(value)) }
}

unsafe fn msg_send_void_i64(receiver: *mut c_void, selector: *const c_void, value: i64) {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void, i64) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector, value) }
}

unsafe fn msg_send_void_u64(receiver: *mut c_void, selector: *const c_void, value: u64) {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void, u64) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector, value) }
}

unsafe fn msg_send_u64(receiver: *mut c_void, selector: *const c_void) -> u64 {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void) -> u64 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector) }
}

unsafe fn msg_send_i64(receiver: *mut c_void, selector: *const c_void) -> i64 {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void) -> i64 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector) }
}

unsafe fn msg_send_f64(receiver: *mut c_void, selector: *const c_void) -> f64 {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void) -> f64 =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector) }
}

unsafe fn msg_send_cstr(receiver: *mut c_void, selector: *const c_void) -> *const c_char {
    let function: unsafe extern "C" fn(*mut c_void, *const c_void) -> *const c_char =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { function(receiver, selector) }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CfDictionaryRef) -> bool;
    fn AXUIElementCreateSystemWide() -> AxUiElementRef;
    fn AXUIElementGetPid(element: AxUiElementRef, pid: *mut i32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: AxUiElementRef,
        attribute: CfStringRef,
        value: *mut CfTypeRef,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AxUiElementRef,
        attribute: CfStringRef,
        parameter: CfTypeRef,
        value: *mut CfTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: AxUiElementRef,
        attribute: CfStringRef,
        settable: *mut bool,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AxUiElementRef,
        attribute: CfStringRef,
        value: CfTypeRef,
    ) -> i32;
    fn AXValueGetValue(value: CfTypeRef, value_type: i32, output: *mut c_void) -> bool;

    static kAXTrustedCheckOptionPrompt: CfStringRef;

    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: Option<extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void>,
        user_info: *mut c_void,
    ) -> *const c_void;
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> CgPoint;
    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGEventTapEnable(tap: *const c_void, enable: bool);
    fn CGEventGetFlags(event: *const c_void) -> u64;
    fn CGEventGetIntegerValueField(event: *const c_void, field: u32) -> i64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CfTypeRef);
    fn CFGetTypeID(value: CfTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        value: *const c_char,
        encoding: u32,
    ) -> CfStringRef;
    fn CFStringGetLength(value: CfStringRef) -> c_long;
    fn CFStringGetMaximumSizeForEncoding(length: c_long, encoding: u32) -> c_long;
    fn CFStringGetCString(
        value: CfStringRef,
        buffer: *mut c_char,
        buffer_size: c_long,
        encoding: u32,
    ) -> bool;
    fn CFDataCreate(allocator: *const c_void, bytes: *const u8, length: c_long) -> CfTypeRef;
    fn CFDataGetLength(data: CfTypeRef) -> c_long;
    fn CFDataGetBytePtr(data: CfTypeRef) -> *const u8;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const CfTypeRef,
        values: *const CfTypeRef,
        count: c_long,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CfDictionaryRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *const c_void,
        order: c_long,
    ) -> *const c_void;
    fn CFRunLoopGetCurrent() -> *const c_void;
    fn CFRunLoopAddSource(run_loop: *const c_void, source: *const c_void, mode: CfStringRef);
    fn CFRunLoopRemoveSource(run_loop: *const c_void, source: *const c_void, mode: CfStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: *const c_void);

    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFBooleanTrue: CfTypeRef;
    static kCFBooleanFalse: CfTypeRef;
    static kCFRunLoopCommonModes: CfStringRef;
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecItemAdd(attributes: CfDictionaryRef, result: *mut CfTypeRef) -> OsStatus;
    fn SecItemUpdate(query: CfDictionaryRef, updates: CfDictionaryRef) -> OsStatus;
    fn SecItemCopyMatching(query: CfDictionaryRef, result: *mut CfTypeRef) -> OsStatus;
    fn SecItemDelete(query: CfDictionaryRef) -> OsStatus;

    static kSecClass: CfStringRef;
    static kSecClassGenericPassword: CfStringRef;
    static kSecAttrService: CfStringRef;
    static kSecAttrAccount: CfStringRef;
    static kSecAttrSynchronizable: CfStringRef;
    static kSecValueData: CfStringRef;
    static kSecReturnData: CfStringRef;
    static kSecMatchLimit: CfStringRef;
    static kSecMatchLimitOne: CfStringRef;
}

#[link(name = "AppKit", kind = "framework")]
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *const c_void;
    fn objc_msgSend();
}

unsafe extern "C" {
    fn kivo_microphone_authorization_status() -> i32;
    fn kivo_speech_authorization_status() -> i32;
    fn kivo_request_microphone_authorization();
    fn kivo_request_speech_authorization();
    fn kivo_speech_start(
        locale: *const c_char,
        require_on_device: bool,
        callback: Option<extern "C" fn(*mut c_void, i32, *const c_char, f32)>,
        context: *mut c_void,
    ) -> *mut c_void;
    fn kivo_speech_stop(handle: *mut c_void);
    fn kivo_speech_cancel(handle: *mut c_void);
    fn kivo_speech_destroy(handle: *mut c_void);
}

#[cfg(test)]
mod tests {
    use super::quartz_to_physical_top_left;

    #[test]
    fn quartz_points_convert_to_physical_top_left() {
        // 1440x900-point display at 2x: bottom-left Quartz origin becomes
        // top-left physical pixels.
        let bottom_left = quartz_to_physical_top_left(0.0, 0.0, 900.0, 2.0);
        assert_eq!((bottom_left.x, bottom_left.y), (0.0, 1800.0));
        let top_left = quartz_to_physical_top_left(0.0, 900.0, 900.0, 2.0);
        assert_eq!((top_left.x, top_left.y), (0.0, 0.0));
        let middle = quartz_to_physical_top_left(720.0, 450.0, 900.0, 2.0);
        assert_eq!((middle.x, middle.y), (1440.0, 900.0));
    }
}
