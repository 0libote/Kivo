//! Desktop SAPI recognition: works in the NSIS .exe without package identity.
//! COM objects stay on one worker; only commands and transcript callbacks cross it.
use super::{
    PlatformError, PlatformErrorKind, PlatformResult, SpeechCallback, SpeechEvent, SpeechOptions,
    SpeechSession,
};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Globalization::{LCIDToLocaleName, LocaleNameToLCID},
        Media::Speech::*,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
    },
    core::{IUnknown, Interface, PCWSTR, PWSTR, w},
};

fn unavailable() -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Unsupported,
        "start_speech",
        "Install a Windows desktop speech language in Settings, then try again.",
    )
}
fn failed() -> PlatformError {
    PlatformError::new(
        PlatformErrorKind::Speech,
        "start_speech",
        "Windows Speech could not use the microphone. Check the default input and microphone privacy settings.",
    )
}

struct Apartment;
impl Apartment {
    fn new() -> PlatformResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|_| failed())?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

unsafe fn category(id: PCWSTR) -> windows::core::Result<ISpObjectTokenCategory> {
    unsafe {
        let category: ISpObjectTokenCategory =
            CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER)?;
        category.SetId(id, false)?;
        Ok(category)
    }
}

unsafe fn take_string(value: PWSTR) -> String {
    unsafe {
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0.cast()));
        text
    }
}

unsafe fn default_token(category_id: PCWSTR) -> windows::core::Result<ISpObjectToken> {
    unsafe {
        let category = category(category_id)?;
        let id = match category.GetDefaultTokenId() {
            Ok(id) => id,
            Err(_) => return category.EnumTokens(PCWSTR::null(), PCWSTR::null())?.Item(0),
        };
        let token: ISpObjectToken = CoCreateInstance(&SpObjectToken, None, CLSCTX_INPROC_SERVER)?;
        let result = token.SetId(PCWSTR::null(), PCWSTR(id.0), false);
        CoTaskMemFree(Some(id.0.cast()));
        result?;
        Ok(token)
    }
}

pub(crate) fn languages() -> Vec<(String, String)> {
    thread::spawn(|| {
        let Ok(_apartment) = Apartment::new() else {
            return Vec::new();
        };
        let enumerate = || -> windows::core::Result<Vec<(String, String)>> {
            unsafe {
                let tokens =
                    category(SPCAT_RECOGNIZERS)?.EnumTokens(PCWSTR::null(), PCWSTR::null())?;
                let mut count = 0;
                tokens.GetCount(&mut count)?;
                let mut result = Vec::new();
                for index in 0..count {
                    let token = tokens.Item(index)?;
                    let attributes = token.OpenKey(w!("Attributes"))?;
                    let codes = take_string(attributes.GetStringValue(w!("Language"))?);
                    for code in codes.split(';') {
                        let Ok(locale) = u32::from_str_radix(code, 16) else {
                            continue;
                        };
                        let mut buffer = [0_u16; 85];
                        let length = LCIDToLocaleName(locale, Some(&mut buffer), 0);
                        if length > 1 {
                            let tag = String::from_utf16_lossy(&buffer[..length as usize - 1]);
                            if !result.iter().any(|(existing, _)| existing == &tag) {
                                result.push((tag.clone(), tag));
                            }
                        }
                    }
                }
                Ok(result)
            }
        };
        enumerate().unwrap_or_default()
    })
    .join()
    .unwrap_or_default()
}

enum Command {
    Stop(mpsc::SyncSender<PlatformResult<()>>),
    Cancel,
}
struct Session {
    commands: mpsc::Sender<Command>,
    worker: Option<thread::JoinHandle<()>>,
}

pub(super) fn start(
    options: SpeechOptions,
    callback: SpeechCallback,
) -> PlatformResult<Box<dyn SpeechSession>> {
    if options.microphone_id.is_some() {
        return Err(unavailable());
    }
    let (commands, receiver) = mpsc::channel();
    let (ready, started) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let Ok(_apartment) = Apartment::new() else {
            let _ = ready.send(Err(failed()));
            return;
        };
        let setup = || -> PlatformResult<(ISpRecognizer, ISpRecoContext, ISpRecoGrammar)> {
            unsafe {
                let recognizer: ISpRecognizer =
                    CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_INPROC_SERVER)
                        .map_err(|_| unavailable())?;
                let token = if let Some(language) = options.language {
                    let name: Vec<u16> = language.encode_utf16().chain(Some(0)).collect();
                    let locale = LocaleNameToLCID(PCWSTR(name.as_ptr()), 0);
                    if locale == 0 {
                        return Err(unavailable());
                    }
                    let filter: Vec<u16> = format!("Language={:X}", locale & 0xffff)
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    category(SPCAT_RECOGNIZERS)
                        .and_then(|category| {
                            category.EnumTokens(PCWSTR(filter.as_ptr()), PCWSTR::null())
                        })
                        .and_then(|tokens| tokens.Item(0))
                        .map_err(|_| unavailable())?
                } else {
                    default_token(SPCAT_RECOGNIZERS).map_err(|_| unavailable())?
                };
                recognizer
                    .SetRecognizer(&token)
                    .map_err(|_| unavailable())?;
                recognizer
                    .SetInput(&default_token(SPCAT_AUDIOIN).map_err(|_| failed())?, true)
                    .map_err(|_| failed())?;
                let context = recognizer.CreateRecoContext().map_err(|_| failed())?;
                let interest = (1_u64 << SPEI_RECOGNITION.0)
                    | (1_u64 << SPEI_END_SR_STREAM.0)
                    | (1_u64 << 30)
                    | (1_u64 << 33);
                context
                    .SetInterest(interest, interest)
                    .map_err(|_| failed())?;
                let grammar = context.CreateGrammar(1).map_err(|_| unavailable())?;
                grammar
                    .LoadDictation(PCWSTR::null(), SPLO_STATIC)
                    .map_err(|_| unavailable())?;
                grammar
                    .SetDictationState(SPRS_ACTIVE)
                    .map_err(|_| failed())?;
                recognizer
                    .SetRecoState(SPRST_ACTIVE)
                    .map_err(|_| failed())?;
                Ok((recognizer, context, grammar))
            }
        };
        let (recognizer, context, _grammar) = match setup() {
            Ok(state) => state,
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        if ready.send(Ok(())).is_err() {
            unsafe {
                let _ = recognizer.SetRecoState(SPRST_INACTIVE_WITH_PURGE);
            }
            return;
        }
        callback(SpeechEvent::Listening);
        let mut transcript = Vec::new();
        let mut stopping: Option<(Instant, mpsc::SyncSender<PlatformResult<()>>)> = None;
        loop {
            match receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(Command::Cancel) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Command::Stop(reply)) => {
                    if unsafe { recognizer.SetRecoState(SPRST_INACTIVE) }.is_err() {
                        let _ = reply.send(Err(failed()));
                        break;
                    }
                    stopping = Some((Instant::now() + Duration::from_secs(3), reply));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let ended = drain_events(&context, &mut transcript);
            match ended {
                Err(_) => {
                    callback(SpeechEvent::Error(failed()));
                    if let Some((_, reply)) = stopping.take() {
                        let _ = reply.send(Err(failed()));
                    }
                    break;
                }
                Ok(ended) => {
                    if let Some((deadline, _)) = &stopping {
                        if ended || Instant::now() >= *deadline {
                            callback(SpeechEvent::Final(transcript.join(" ")));
                            if let Some((_, reply)) = stopping.take() {
                                let _ = reply.send(Ok(()));
                            }
                            break;
                        }
                    } else if ended {
                        callback(SpeechEvent::Error(failed()));
                        break;
                    }
                }
            }
        }
        unsafe {
            let _ = recognizer.SetRecoState(SPRST_INACTIVE_WITH_PURGE);
        }
    });
    let session = Session {
        commands,
        worker: Some(worker),
    };
    started
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| failed())??;
    Ok(Box::new(session))
}

fn drain_events(
    context: &ISpRecoContext,
    transcript: &mut Vec<String>,
) -> windows::core::Result<bool> {
    unsafe {
        let mut ended = false;
        loop {
            let mut event = SPEVENT::default();
            let mut fetched = 0;
            context.GetEvents(1, &mut event, &mut fetched)?;
            if fetched == 0 {
                return Ok(ended);
            }
            let id = event._bitfield & 0xffff;
            let parameter_kind = (event._bitfield as u32 >> 16) as i32;
            if id == SPEI_END_SR_STREAM.0 {
                ended = true;
            }
            if event.lParam.0 != 0 {
                if parameter_kind == SPET_LPARAM_IS_OBJECT.0
                    || parameter_kind == SPET_LPARAM_IS_TOKEN.0
                {
                    let object = IUnknown::from_raw(event.lParam.0 as *mut _);
                    if id == SPEI_RECOGNITION.0 {
                        let result = object.cast::<ISpRecoResult>()?;
                        let mut text = PWSTR::null();
                        result.GetText(0, u32::MAX, true, &mut text, None)?;
                        let text = take_string(text);
                        if !text.trim().is_empty() {
                            transcript.push(text);
                        }
                    }
                } else if parameter_kind == SPET_LPARAM_IS_POINTER.0
                    || parameter_kind == SPET_LPARAM_IS_STRING.0
                {
                    CoTaskMemFree(Some(event.lParam.0 as *const _));
                }
            }
        }
    }
}

impl SpeechSession for Session {
    fn stop(&mut self) -> PlatformResult<()> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Stop(reply))
            .map_err(|_| failed())?;
        result
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| failed())?
    }
    fn cancel(&mut self) -> PlatformResult<()> {
        let _ = self.commands.send(Command::Cancel);
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Cancel);
        // The worker owns COM resources until recognition has fully shut down.
        // Detach rather than blocking the UI on a slow microphone driver.
        self.worker.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_desktop_engine_loads_dictation_without_recording() {
        let _apartment = Apartment::new().expect("COM initialization");
        unsafe {
            let Ok(category) = category(SPCAT_RECOGNIZERS) else {
                return;
            };
            let tokens = category.EnumTokens(PCWSTR::null(), PCWSTR::null()).unwrap();
            let mut count = 0;
            tokens.GetCount(&mut count).unwrap();
            if count == 0 {
                return;
            } // Hosted CI images may have no speech pack.
            let recognizer: ISpRecognizer =
                CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_INPROC_SERVER).unwrap();
            recognizer.SetRecoState(SPRST_INACTIVE_WITH_PURGE).unwrap();
            recognizer
                .SetRecognizer(&default_token(SPCAT_RECOGNIZERS).unwrap())
                .unwrap();
            let context = recognizer.CreateRecoContext().unwrap();
            let grammar = context.CreateGrammar(1).unwrap();
            grammar.LoadDictation(PCWSTR::null(), SPLO_STATIC).unwrap();
            // No audio input or active grammar: this check never opens a microphone.
        }
    }
}
