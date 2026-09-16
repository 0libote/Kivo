use std::{
    mem::size_of,
    ptr,
    sync::{Mutex, OnceLock},
    thread::{self, JoinHandle},
};

use ::windows::{
    Win32::{
        Foundation::{
            ERROR_NOT_FOUND, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, WPARAM,
        },
        Graphics::Dwm::{
            DWM_SYSTEMBACKDROP_TYPE, DWMSBT_NONE, DWMWA_BORDER_COLOR, DWMWA_SYSTEMBACKDROP_TYPE,
            DwmSetWindowAttribute,
        },
        Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
            CredReadW, CredWriteW,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, IDataObject,
        },
        System::DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
            OpenClipboard, SetClipboardData,
        },
        System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        System::Ole::{CF_UNICODETEXT, OleFlushClipboard, OleGetClipboard, OleSetClipboard},
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
            },
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
                SendInput, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_RWIN, VK_V,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetMessageW,
                GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, KBDLLHOOKSTRUCT, MSG,
                PostThreadMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowsHookExW,
                UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
                WM_SYSKEYUP, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{HRESULT, PWSTR, w},
};

use crate::security::{CredentialError, CredentialStore, SecretString};

use super::*;

static NEXT_SELECTION_TOKEN: AtomicU64 = AtomicU64::new(1);
/// The low-level keyboard hook has nowhere to carry per-registration state,
/// so the active monitor thread publishes its callback here. At most one
/// native monitor is active; overlapping lifetimes during re-registration are
/// disambiguated by `SHORTCUT_GENERATION` so a stale thread's cleanup can
/// never wipe a newer registration.
static SHORTCUT_CONTEXT: OnceLock<Mutex<Option<ShortcutContext>>> = OnceLock::new();
static SHORTCUT_GENERATION: AtomicU64 = AtomicU64::new(1);

use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct PlatformImpl {
    credentials: WindowsCredentialStore,
    selection: Mutex<Option<(u64, WindowsTextTarget)>>,
}

impl PlatformImpl {
    pub(super) fn new() -> PlatformResult<Self> {
        Ok(Self {
            credentials: WindowsCredentialStore,
            selection: Mutex::new(None),
        })
    }

    pub(super) fn credential_store(&self) -> &dyn CredentialStore {
        &self.credentials
    }

    pub(super) fn get_selected_text(&self) -> PlatformResult<SelectionSnapshot> {
        let native = selected_text();
        let window = unsafe { GetForegroundWindow() };
        let target = WindowsTextTarget::capture()?;
        let (text, process_id, bounds, strategy) = match native {
            Ok((text, process_id, bounds)) => (
                text,
                process_id,
                bounds,
                crate::text::TextAccessStrategy::Accessibility,
            ),
            Err(error)
                if error.operation == "get_selected_text"
                    && matches!(
                        error.kind,
                        PlatformErrorKind::Unsupported | PlatformErrorKind::NotFound
                    ) =>
            {
                let text = capture_selected_text_fallback(window)?;
                (
                    text,
                    target.process_id,
                    None,
                    crate::text::TextAccessStrategy::ClipboardFallback,
                )
            }
            Err(error) => return Err(error),
        };
        let mut title = [0_u16; 260];
        let title_length = unsafe { GetWindowTextW(window, &mut title) }.max(0) as usize;
        let name = String::from_utf16_lossy(&title[..title_length]);
        let token = NEXT_SELECTION_TOKEN.fetch_add(1, Ordering::Relaxed);
        *self
            .selection
            .lock()
            .map_err(|_| os_error("get_selected_text", "Selection state unavailable."))? =
            Some((token, target));
        Ok(SelectionSnapshot {
            text,
            bounds: bounds.into_iter().collect(),
            owner: ActiveApplication {
                process_id,
                name: if name.is_empty() {
                    format!("Application {process_id}")
                } else {
                    name
                },
                identifier: None,
                native_handle: window.0 as usize,
            },
            strategy,
            native_token: token,
        })
    }

    pub(super) fn cursor_position(&self) -> PlatformResult<ScreenPoint> {
        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point)
                .map_err(|_| os_error("cursor_position", "The mouse position is unavailable."))?;
        }
        Ok(ScreenPoint {
            x: f64::from(point.x),
            y: f64::from(point.y),
        })
    }

    pub(super) fn replace_selected_text(
        &self,
        snapshot: &SelectionSnapshot,
        replacement: &str,
    ) -> PlatformResult<()> {
        let window = HWND(snapshot.owner.native_handle as *mut _);
        if !unsafe { SetForegroundWindow(window) }.as_bool() {
            return Err(os_error(
                "replace_selected_text",
                "The original application could not be focused.",
            ));
        }
        let selection = self
            .selection
            .lock()
            .map_err(|_| os_error("replace_selected_text", "Selection state unavailable."))?;
        if !selection
            .as_ref()
            .is_some_and(|(token, target)| *token == snapshot.native_token && target.is_current())
        {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "The original text field or selection changed.",
            ));
        }
        let (current, process_id) = match selected_text() {
            Ok((current, process_id, _)) => (current, process_id),
            Err(error)
                if snapshot.strategy == crate::text::TextAccessStrategy::ClipboardFallback
                    && matches!(
                        error.kind,
                        PlatformErrorKind::Unsupported | PlatformErrorKind::NotFound
                    ) =>
            {
                (
                    capture_selected_text_fallback(window)?,
                    snapshot.owner.process_id,
                )
            }
            Err(error) => return Err(error),
        };
        if current != snapshot.text || process_id != snapshot.owner.process_id {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "The selection changed before Kivo could replace it.",
            ));
        }
        paste_text_fallback(window, replacement, "replace_selected_text")
    }

    pub(super) fn capture_insertion_target(
        &self,
    ) -> PlatformResult<Box<dyn crate::text::InsertionTarget>> {
        Ok(Box::new(WindowsTextTarget::capture()?))
    }

    pub(super) fn permission_status(
        &self,
        permission: PermissionKind,
    ) -> PlatformResult<PermissionStatus> {
        Ok(match permission {
            PermissionKind::Accessibility | PermissionKind::InputMonitoring => {
                PermissionStatus::Unavailable
            }
            // Desktop Windows has no in-app consent prompt for these: SAPI
            // and WASAPI capture just work or fail at use time with an
            // actionable error (see windows_speech::start). Reporting
            // NotDetermined here left the UI on an Allow button that could
            // never resolve, so report the checkable state instead: the
            // microphone is assumed available until capture fails, while
            // speech recognition reflects whether a desktop recognizer is
            // actually installed.
            PermissionKind::Microphone => PermissionStatus::Granted,
            PermissionKind::SpeechRecognition => {
                if super::windows_speech::languages().is_empty() {
                    PermissionStatus::Denied
                } else {
                    PermissionStatus::Granted
                }
            }
        })
    }

    pub(super) fn request_permission(&self, permission: PermissionKind) -> PlatformResult<()> {
        match permission {
            PermissionKind::Microphone | PermissionKind::SpeechRecognition => Ok(()),
            _ => Err(PlatformError::unsupported(
                "request_permission",
                "Windows does not require this permission.",
            )),
        }
    }

    pub(super) fn register_dictation_shortcut(
        &self,
        shortcut: HoldShortcut,
        callback: HoldShortcutCallback,
    ) -> PlatformResult<Box<dyn ShortcutRegistration>> {
        if shortcut.modifiers != [ModifierKey::Control, ModifierKey::Meta]
            || shortcut.key_code.is_some()
        {
            return Err(PlatformError::unsupported(
                "register_dictation_shortcut",
                "The Windows native monitor currently supports the Ctrl+Win hold shortcut.",
            ));
        }
        Ok(Box::new(WindowsShortcutRegistration::start(
            callback,
            shortcut.suppress,
        )?))
    }

    pub(super) fn style_window(
        &self,
        native_window: usize,
        kind: OverlayKind,
    ) -> PlatformResult<()> {
        let window = HWND(native_window as *mut _);
        unsafe {
            if matches!(kind, OverlayKind::FlowBar | OverlayKind::WritingTools) {
                let existing = GetWindowLongPtrW(window, GWL_EXSTYLE);
                let no_activate = if kind == OverlayKind::FlowBar {
                    WS_EX_NOACTIVATE.0
                } else {
                    0
                };
                SetWindowLongPtrW(
                    window,
                    GWL_EXSTYLE,
                    existing | (WS_EX_TOOLWINDOW.0 | no_activate) as isize,
                );
            }
            // Let the webview supply a compact opaque surface, without a
            // native acrylic rectangle behind the smaller recording pill.
            let border: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE on Windows 11.
            let _ = DwmSetWindowAttribute(
                window,
                DWMWA_BORDER_COLOR,
                (&border as *const u32).cast(),
                size_of::<u32>() as u32,
            );
            let backdrop: DWM_SYSTEMBACKDROP_TYPE = DWMSBT_NONE;
            DwmSetWindowAttribute(
                window,
                DWMWA_SYSTEMBACKDROP_TYPE,
                (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast(),
                size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )
            .map_err(|_| {
                os_error(
                    "style_window",
                    "Windows could not apply the system material.",
                )
            })?;
        }
        Ok(())
    }

    pub(super) fn start_speech(
        &self,
        options: SpeechOptions,
        callback: SpeechCallback,
    ) -> PlatformResult<Box<dyn SpeechSession>> {
        super::windows_speech::start(options, callback)
    }
}

struct WindowsTextTarget {
    window: usize,
    process_id: u32,
    identity: Vec<i32>,
}

impl WindowsTextTarget {
    fn capture() -> PlatformResult<Self> {
        let window = unsafe { GetForegroundWindow() };
        let (identity, process_id) = focused_identity().unwrap_or_else(|_| {
            let mut process_id = 0;
            unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
            (Vec::new(), process_id)
        });
        Ok(Self {
            window: window.0 as usize,
            process_id,
            identity,
        })
    }
    fn is_current(&self) -> bool {
        unsafe { GetForegroundWindow() }.0 as usize == self.window
            && if self.identity.is_empty() {
                let mut process_id = 0;
                unsafe {
                    GetWindowThreadProcessId(HWND(self.window as *mut _), Some(&mut process_id))
                };
                process_id == self.process_id
            } else {
                focused_identity()
                    .ok()
                    .is_some_and(|(identity, process_id)| {
                        identity == self.identity && process_id == self.process_id
                    })
            }
    }
}

// UIA calls may run on Tauri's existing STA or a worker's new MTA. Only
// balance successful initialization; RPC_E_CHANGED_MODE reuses the STA.
struct AutomationApartment(bool);
impl AutomationApartment {
    fn new() -> Self {
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}
impl Drop for AutomationApartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

impl crate::text::InsertionTarget for WindowsTextTarget {
    fn insert(&self, text: &str) -> Result<(), crate::text::TextError> {
        if !self.is_current() {
            return Err(crate::text::TextError::SelectionExpired);
        }
        paste_text_fallback(HWND(self.window as *mut _), text, "insert_text_at_cursor")
            .map_err(|_| crate::text::TextError::InsertionFailed)
    }
}

fn focused_identity() -> PlatformResult<(Vec<i32>, u32)> {
    use ::windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
    };
    // Hash (not length) of the surrounding text: a same-length edit in the
    // same control must invalidate the snapshot, or replacement could land
    // on changed text.
    fn text_identity(text: &str) -> i32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish() as i32
    }
    let read = || -> ::windows::core::Result<(Vec<i32>, u32)> {
        unsafe {
            let _apartment = AutomationApartment::new();
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let element = automation.GetFocusedElement()?;
            if element.CurrentIsPassword()?.as_bool() {
                return Err(::windows::core::Error::from_hresult(HRESULT(
                    0x80070005u32 as i32,
                )));
            }
            let array = element.GetRuntimeId()?;
            let result = (|| {
                let low = SafeArrayGetLBound(array, 1)?;
                let high = SafeArrayGetUBound(array, 1)?;
                let process_id = element.CurrentProcessId()?;
                let mut identity = Vec::new();
                for index in low..=high {
                    let mut value = 0_i32;
                    SafeArrayGetElement(array, &index, (&mut value as *mut i32).cast())?;
                    identity.push(value);
                }
                // Remember the caret/selection offsets as well as the control.
                // A moved caret in the same editor must also require recovery.
                if let Ok(pattern) =
                    element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                    && let Ok(selections) = pattern.GetSelection()
                    && selections.Length().unwrap_or_default() == 1
                    && let Ok(selected) = selections.GetElement(0)
                    && let Ok(before) = pattern.DocumentRange()
                {
                    use ::windows::Win32::UI::Accessibility::{
                        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
                    };
                    if before
                        .MoveEndpointByRange(
                            TextPatternRangeEndpoint_End,
                            &selected,
                            TextPatternRangeEndpoint_Start,
                        )
                        .is_ok()
                    {
                        // Hash the text, not its length: a same-length edit
                        // in the same control must invalidate the snapshot,
                        // or replacement could land on changed text.
                        identity.push(
                            before
                                .GetText(-1)
                                .map(|v| text_identity(&v.to_string()))
                                .unwrap_or(-1),
                        );
                        identity.push(
                            selected
                                .GetText(-1)
                                .map(|v| text_identity(&v.to_string()))
                                .unwrap_or(-1),
                        );
                    }
                }
                Ok((identity, process_id.max(0) as u32))
            })();
            let _ = SafeArrayDestroy(array);
            result
        }
    };
    read().map_err(|_| {
        PlatformError::unsupported(
            "capture_insertion_target",
            "Focus an editable text field first.",
        )
    })
}

struct WindowsCredentialStore;

impl CredentialStore for WindowsCredentialStore {
    fn save_api_key(&self, secret: &SecretString) -> Result<(), CredentialError> {
        let mut blob = secret.expose().as_bytes().to_vec();
        let mut target = wide("com.kivo.desktop/gemini-api-key");
        let mut username = wide("Kivo");
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(username.as_mut_ptr()),
            ..Default::default()
        };
        let result = unsafe { CredWriteW(&credential, 0) }.map_err(|_| CredentialError::Backend);
        blob.fill(0);
        result
    }

    fn load_api_key(&self) -> Result<Option<SecretString>, CredentialError> {
        let mut credential = ptr::null_mut();
        let result = unsafe {
            CredReadW(
                w!("com.kivo.desktop/gemini-api-key"),
                CRED_TYPE_GENERIC,
                None,
                &mut credential,
            )
        };
        if let Err(error) = result {
            if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
                return Ok(None);
            }
            return Err(CredentialError::Backend);
        }
        if credential.is_null() {
            return Ok(None);
        }
        let value = unsafe {
            let credential = &*credential;
            let bytes = std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            );
            String::from_utf8(bytes.to_vec()).map_err(|_| CredentialError::Backend)
        };
        unsafe { CredFree(credential.cast()) };
        value.and_then(SecretString::new).map(Some)
    }

    fn clear_api_key(&self) -> Result<(), CredentialError> {
        match unsafe {
            CredDeleteW(
                w!("com.kivo.desktop/gemini-api-key"),
                CRED_TYPE_GENERIC,
                None,
            )
        } {
            Ok(()) => Ok(()),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => Ok(()),
            Err(_) => Err(CredentialError::Backend),
        }
    }
}

fn selected_text() -> PlatformResult<(String, u32, Option<ScreenRect>)> {
    unsafe {
        let _apartment = AutomationApartment::new();
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|_| {
                os_error("get_selected_text", "Windows UI Automation is unavailable.")
            })?;
        let focused = automation
            .GetFocusedElement()
            .map_err(|_| os_error("get_selected_text", "The focused control is unavailable."))?;
        if focused.CurrentIsPassword().unwrap_or_default().as_bool() {
            return Err(PlatformError::unsupported(
                "secure_text",
                "Password fields are excluded.",
            ));
        }
        let pattern: IUIAutomationTextPattern = focused
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| {
                PlatformError::unsupported(
                    "get_selected_text",
                    "The focused control does not expose text selection.",
                )
            })?;
        let selection = pattern
            .GetSelection()
            .map_err(|_| os_error("get_selected_text", "The selection could not be read."))?;
        if selection.Length().unwrap_or_default() != 1 {
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                "get_selected_text",
                "Select some text first.",
            ));
        }
        let range = selection
            .GetElement(0)
            .map_err(|_| os_error("get_selected_text", "The selection could not be read."))?;
        let text = range
            .GetText(-1)
            .map_err(|_| os_error("get_selected_text", "The selected text could not be read."))?
            .to_string();
        if text.trim().is_empty() {
            return Err(PlatformError::new(
                PlatformErrorKind::NotFound,
                "get_selected_text",
                "Select some text first.",
            ));
        }
        let process_id = focused.CurrentProcessId().unwrap_or_default().max(0) as u32;
        let bounds = focused
            .CurrentBoundingRectangle()
            .ok()
            .map(|rect| ScreenRect {
                x: f64::from(rect.left),
                y: f64::from(rect.top),
                width: f64::from(rect.right - rect.left),
                height: f64::from(rect.bottom - rect.top),
            });
        Ok((text, process_id, bounds))
    }
}

fn capture_selected_text_fallback(window: HWND) -> PlatformResult<String> {
    let _apartment = AutomationApartment::new();
    let snapshot = unsafe { OleGetClipboard().ok() };
    // Never clear the clipboard before asking the target to copy: clearing
    // first destroys non-text formats, and if the target then ignores Ctrl+C
    // (or another app writes in between) the restore is skipped and the
    // user's original clipboard is lost. Instead, record the sequence and
    // treat only a change as a fresh copy — a real copy always bumps the
    // sequence, even when the bytes are identical.
    let before_sequence = unsafe { GetClipboardSequenceNumber() };
    if let Err(error) = send_control_shortcut(VK_C, "get_selected_text") {
        return Err(error);
    }
    let mut captured_sequence = before_sequence;
    for _ in 0..150 {
        captured_sequence = unsafe { GetClipboardSequenceNumber() };
        if captured_sequence != before_sequence {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
    if captured_sequence == before_sequence {
        // The target ignored the copy; the clipboard was never touched.
        return Err(PlatformError::new(
            PlatformErrorKind::Unsupported,
            "get_selected_text",
            "The selection could not be read in this application.",
        ));
    }
    let text = read_clipboard_text(window, "get_selected_text");
    restore_clipboard_if_unchanged(snapshot.as_ref(), captured_sequence);
    let text = text?;
    if text.trim().is_empty() {
        Err(PlatformError::new(
            PlatformErrorKind::NotFound,
            "get_selected_text",
            "Select some text first.",
        ))
    } else {
        Ok(text)
    }
}

fn paste_text_fallback(window: HWND, text: &str, operation: &'static str) -> PlatformResult<()> {
    let _apartment = AutomationApartment::new();
    let snapshot = unsafe { OleGetClipboard().ok() };
    write_clipboard_text(window, text, operation)?;
    let written_sequence = unsafe { GetClipboardSequenceNumber() };
    if let Err(error) = send_control_shortcut(VK_V, operation) {
        restore_clipboard_if_unchanged(snapshot.as_ref(), written_sequence);
        return Err(error);
    }
    thread::sleep(std::time::Duration::from_millis(350));
    restore_clipboard_if_unchanged(snapshot.as_ref(), written_sequence);
    Ok(())
}

fn write_clipboard_text(window: HWND, value: &str, operation: &'static str) -> PlatformResult<()> {
    let text: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2)
            .map_err(|_| os_error(operation, "The clipboard text could not be allocated."))?;
        let locked = GlobalLock(memory) as *mut u16;
        if locked.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(os_error(
                operation,
                "The clipboard text could not be written.",
            ));
        }
        ptr::copy_nonoverlapping(text.as_ptr(), locked, text.len());
        let _ = GlobalUnlock(memory);
        if OpenClipboard(Some(window)).is_err() {
            let _ = GlobalFree(Some(memory));
            return Err(os_error(operation, "The clipboard is busy."));
        }
        let result = EmptyClipboard()
            .and_then(|_| SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))));
        let _ = CloseClipboard();
        if result.is_err() {
            let _ = GlobalFree(Some(memory));
            return Err(os_error(
                operation,
                "The clipboard text could not be written.",
            ));
        }
    }
    Ok(())
}

fn read_clipboard_text(window: HWND, operation: &'static str) -> PlatformResult<String> {
    unsafe {
        OpenClipboard(Some(window)).map_err(|_| os_error(operation, "The clipboard is busy."))?;
        let result = (|| {
            let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0))?;
            let locked = GlobalLock(HGLOBAL(handle.0)) as *const u16;
            if locked.is_null() {
                return Err(::windows::core::Error::from_hresult(HRESULT(
                    0x80004005u32 as i32,
                )));
            }
            let mut length = 0usize;
            while *locked.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(locked, length));
            let _ = GlobalUnlock(HGLOBAL(handle.0));
            Ok(text)
        })();
        let _ = CloseClipboard();
        result.map_err(|_| os_error(operation, "The copied selection did not contain text."))
    }
}

fn restore_clipboard_if_unchanged(snapshot: Option<&IDataObject>, expected_sequence: u32) {
    if unsafe { GetClipboardSequenceNumber() } != expected_sequence {
        return;
    }
    unsafe {
        if OleSetClipboard(snapshot).is_ok() && snapshot.is_some() {
            let _ = OleFlushClipboard();
        }
    }
}

fn send_control_shortcut(key: VIRTUAL_KEY, operation: &'static str) -> PlatformResult<()> {
    let inputs = [
        virtual_key_input(VK_CONTROL, Default::default()),
        virtual_key_input(key, Default::default()),
        virtual_key_input(key, KEYEVENTF_KEYUP),
        virtual_key_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } as usize;
    if sent == inputs.len() {
        Ok(())
    } else {
        Err(os_error(
            operation,
            "Windows blocked the keyboard shortcut.",
        ))
    }
}

fn virtual_key_input(
    key: VIRTUAL_KEY,
    flags: ::windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

struct ShortcutContext {
    callback: HoldShortcutCallback,
    active: bool,
    suppress: bool,
    generation: u64,
}

struct WindowsShortcutRegistration {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl WindowsShortcutRegistration {
    fn start(callback: HoldShortcutCallback, suppress: bool) -> PlatformResult<Self> {
        // Claimed before spawning: the shell starts the replacement before
        // stopping the previous monitor, so generations strictly order them.
        let generation = SHORTCUT_GENERATION.fetch_add(1, Ordering::AcqRel);
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = thread::spawn(move || unsafe {
            let thread_id = ::windows::Win32::System::Threading::GetCurrentThreadId();
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0);
            match hook {
                Ok(hook) => {
                    let context = SHORTCUT_CONTEXT.get_or_init(|| Mutex::new(None));
                    if let Ok(mut state) = context.lock() {
                        *state = Some(ShortcutContext {
                            callback,
                            active: false,
                            suppress,
                            generation,
                        });
                    }
                    let _ = sender.send(Ok(thread_id));
                    let mut message = MSG::default();
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {}
                    let _ = UnhookWindowsHookEx(hook);
                    // Only clear what we published: an older thread exiting
                    // after a re-registration must leave the newer monitor's
                    // callback in place, otherwise the hotkey silently dies.
                    if let Ok(mut state) = context.lock()
                        && state
                            .as_ref()
                            .is_some_and(|current| current.generation == generation)
                    {
                        *state = None;
                    }
                }
                Err(_) => {
                    let _ = sender.send(Err(PlatformError::new(
                        PlatformErrorKind::ShortcutConflict,
                        "register_dictation_shortcut",
                        "Windows could not register the dictation shortcut.",
                    )));
                }
            }
        });
        let thread_id = receiver.recv().map_err(|_| {
            os_error(
                "register_dictation_shortcut",
                "The shortcut monitor did not start.",
            )
        })??;
        Ok(Self {
            thread_id,
            thread: Some(thread),
        })
    }
}

impl ShortcutRegistration for WindowsShortcutRegistration {
    fn stop(&mut self) -> PlatformResult<()> {
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }.map_err(
            |_| {
                os_error(
                    "register_dictation_shortcut",
                    "The shortcut monitor did not stop.",
                )
            },
        )?;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Ok(())
    }
}

impl Drop for WindowsShortcutRegistration {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = wparam.0 as u32;
        let is_down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let is_up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        let is_control = event.vkCode == u32::from(VK_CONTROL.0);
        let is_meta = event.vkCode == u32::from(VK_LWIN.0) || event.vkCode == u32::from(VK_RWIN.0);
        let is_escape = event.vkCode == u32::from(VK_ESCAPE.0);
        if let Some(context) = SHORTCUT_CONTEXT.get()
            && let Ok(mut slot) = context.lock()
            && let Some(context) = slot.as_mut()
        {
            let control_down =
                unsafe { GetAsyncKeyState(VK_CONTROL.0.into()) } < 0 || (is_control && is_down);
            let meta_down = unsafe { GetAsyncKeyState(VK_LWIN.0.into()) } < 0
                || unsafe { GetAsyncKeyState(VK_RWIN.0.into()) } < 0
                || (is_meta && is_down);
            if !context.active && control_down && meta_down {
                context.active = true;
                (context.callback)(HoldShortcutEvent::Pressed);
                if context.suppress {
                    return LRESULT(1);
                }
            } else if context.active && is_escape && is_down {
                context.active = false;
                (context.callback)(HoldShortcutEvent::Cancelled);
                return LRESULT(1);
            } else if context.active && is_up && (is_control || is_meta) {
                context.active = false;
                (context.callback)(HoldShortcutEvent::Released);
                if context.suppress {
                    return LRESULT(1);
                }
            } else if context.active && context.suppress && (is_control || is_meta) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn os_error(operation: &'static str, message: impl Into<String>) -> PlatformError {
    PlatformError::new(PlatformErrorKind::Os, operation, message)
}
