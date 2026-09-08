use std::{
    mem::size_of,
    ptr,
    sync::{Arc, Mutex, OnceLock},
    thread::{self, JoinHandle},
};

use ::windows::{
    Foundation::TypedEventHandler,
    Globalization::Language,
    Media::SpeechRecognition::{
        SpeechContinuousRecognitionResultGeneratedEventArgs, SpeechContinuousRecognitionSession,
        SpeechRecognitionResultStatus, SpeechRecognizer,
    },
    Win32::{
        Foundation::{ERROR_NOT_FOUND, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Dwm::{
            DWM_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
            DwmSetWindowAttribute,
        },
        Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
            CredReadW, CredWriteW,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
            },
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
                KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_RWIN,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GWL_EXSTYLE, GetForegroundWindow, GetMessageW, GetWindowLongPtrW,
                GetWindowTextW, KBDLLHOOKSTRUCT, MSG, PostThreadMessageW, SetForegroundWindow,
                SetWindowLongPtrW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
                WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{HRESULT, HSTRING, PWSTR, w},
};

use crate::security::{CredentialError, CredentialStore, SecretString};

use super::*;

static NEXT_SELECTION_TOKEN: AtomicU64 = AtomicU64::new(1);
static SHORTCUT_CONTEXT: OnceLock<Mutex<Option<ShortcutContext>>> = OnceLock::new();

use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct PlatformImpl {
    credentials: WindowsCredentialStore,
}

impl PlatformImpl {
    pub(super) fn new() -> PlatformResult<Self> {
        Ok(Self {
            credentials: WindowsCredentialStore,
        })
    }

    pub(super) fn credential_store(&self) -> &dyn CredentialStore {
        &self.credentials
    }

    pub(super) fn get_selected_text(&self) -> PlatformResult<SelectionSnapshot> {
        let (text, process_id, bounds) = selected_text()?;
        let window = unsafe { GetForegroundWindow() };
        let mut title = [0_u16; 260];
        let title_length = unsafe { GetWindowTextW(window, &mut title) }.max(0) as usize;
        let name = String::from_utf16_lossy(&title[..title_length]);
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
            native_token: NEXT_SELECTION_TOKEN.fetch_add(1, Ordering::Relaxed),
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
        let (current, process_id, _) = selected_text()?;
        if current != snapshot.text || process_id != snapshot.owner.process_id {
            return Err(PlatformError::new(
                PlatformErrorKind::InvalidState,
                "replace_selected_text",
                "The selection changed before Kivo could replace it.",
            ));
        }
        send_unicode(replacement, "replace_selected_text")
    }

    pub(super) fn insert_text_at_cursor(&self, text: &str) -> PlatformResult<()> {
        send_unicode(text, "insert_text_at_cursor")
    }

    pub(super) fn get_cursor_or_selection_position(&self) -> PlatformResult<ScreenPoint> {
        let (_, _, bounds) = selected_text()?;
        bounds.map(ScreenRect::anchor_below).ok_or_else(|| {
            PlatformError::new(
                PlatformErrorKind::NotFound,
                "get_cursor_or_selection_position",
                "The active control did not expose the caret position.",
            )
        })
    }

    pub(super) fn permission_status(
        &self,
        permission: PermissionKind,
    ) -> PlatformResult<PermissionStatus> {
        Ok(match permission {
            PermissionKind::Accessibility | PermissionKind::InputMonitoring => {
                PermissionStatus::Unavailable
            }
            PermissionKind::Microphone | PermissionKind::SpeechRecognition => {
                PermissionStatus::Granted
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
            let backdrop: DWM_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW;
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
        if options.microphone_id.is_some() {
            return Err(PlatformError::unsupported(
                "start_speech",
                "Selecting a non-default microphone is not supported by Windows Speech.",
            ));
        }
        let recognizer = match options.language.as_deref() {
            Some(language) => {
                let language = Language::CreateLanguage(&HSTRING::from(language))
                    .map_err(|_| speech_error("The selected dictation language is unavailable."))?;
                SpeechRecognizer::Create(&language)
            }
            None => SpeechRecognizer::new(),
        }
        .map_err(|_| speech_error("Windows Speech could not be initialized."))?;
        let compilation = recognizer
            .CompileConstraintsAsync()
            .and_then(|operation| operation.join())
            .map_err(|_| speech_error("Windows Speech could not prepare dictation."))?;
        if compilation
            .Status()
            .unwrap_or(SpeechRecognitionResultStatus::Unknown)
            != SpeechRecognitionResultStatus::Success
        {
            return Err(speech_error(
                "Windows Speech is unavailable for this language.",
            ));
        }

        let session = recognizer
            .ContinuousRecognitionSession()
            .map_err(|_| speech_error("Windows Speech could not start a session."))?;
        let transcript = Arc::new(Mutex::new(Vec::<String>::new()));
        let callback_for_results = Arc::clone(&callback);
        let transcript_for_results = Arc::clone(&transcript);
        let result_token = session
            .ResultGenerated(&TypedEventHandler::<
                SpeechContinuousRecognitionSession,
                SpeechContinuousRecognitionResultGeneratedEventArgs,
            >::new(move |_, args| {
                if let Ok(text) = args
                    .ok()
                    .and_then(|args| args.Result())
                    .and_then(|result| result.Text())
                {
                    let text = text.to_string();
                    if !text.trim().is_empty()
                        && let Ok(mut transcript) = transcript_for_results.lock()
                    {
                        transcript.push(text);
                        callback_for_results(SpeechEvent::Partial(transcript.join(" ")));
                    }
                }
                Ok(())
            }))
            .map_err(|_| speech_error("Windows Speech could not attach its result handler."))?;
        session
            .StartAsync()
            .and_then(|operation| operation.join())
            .map_err(|_| speech_error("Windows Speech could not access the microphone."))?;
        callback(SpeechEvent::Listening);
        Ok(Box::new(WindowsSpeechSession {
            recognizer,
            session,
            result_token,
            transcript,
            callback,
            finished: false,
        }))
    }
}

struct WindowsSpeechSession {
    recognizer: SpeechRecognizer,
    session: SpeechContinuousRecognitionSession,
    result_token: i64,
    transcript: Arc<Mutex<Vec<String>>>,
    callback: SpeechCallback,
    finished: bool,
}

impl SpeechSession for WindowsSpeechSession {
    fn stop(&mut self) -> PlatformResult<()> {
        if self.finished {
            return Ok(());
        }
        self.session
            .StopAsync()
            .and_then(|operation| operation.join())
            .map_err(|_| speech_error("Windows Speech could not finish dictation."))?;
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| speech_error("Windows Speech result state is unavailable."))?
            .join(" ");
        (self.callback)(SpeechEvent::Final(transcript));
        self.finished = true;
        Ok(())
    }

    fn cancel(&mut self) -> PlatformResult<()> {
        if !self.finished {
            self.session
                .CancelAsync()
                .and_then(|operation| operation.join())
                .map_err(|_| speech_error("Windows Speech could not cancel dictation."))?;
            self.finished = true;
        }
        Ok(())
    }
}

impl Drop for WindowsSpeechSession {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self
                .session
                .CancelAsync()
                .and_then(|operation| operation.join());
        }
        let _ = self.session.RemoveResultGenerated(self.result_token);
        let _ = self.recognizer.Close();
    }
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
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|_| {
                os_error("get_selected_text", "Windows UI Automation is unavailable.")
            })?;
        let focused = automation
            .GetFocusedElement()
            .map_err(|_| os_error("get_selected_text", "The focused control is unavailable."))?;
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
        if selection.Length().unwrap_or_default() < 1 {
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

fn send_unicode(text: &str, operation: &'static str) -> PlatformResult<()> {
    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
    for unit in text.encode_utf16() {
        inputs.push(keyboard_input(unit, KEYEVENTF_UNICODE));
        inputs.push(keyboard_input(unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } as usize;
    if sent == inputs.len() {
        Ok(())
    } else {
        Err(os_error(operation, "Windows could not insert the text."))
    }
}

fn keyboard_input(
    scan: u16,
    flags: ::windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

struct ShortcutContext {
    callback: HoldShortcutCallback,
    active: bool,
    suppress: bool,
}

struct WindowsShortcutRegistration {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl WindowsShortcutRegistration {
    fn start(callback: HoldShortcutCallback, suppress: bool) -> PlatformResult<Self> {
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
                        });
                    }
                    let _ = sender.send(Ok(thread_id));
                    let mut message = MSG::default();
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {}
                    let _ = UnhookWindowsHookEx(hook);
                    if let Ok(mut state) = context.lock() {
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

fn speech_error(message: impl Into<String>) -> PlatformError {
    PlatformError::new(PlatformErrorKind::Speech, "start_speech", message)
}
