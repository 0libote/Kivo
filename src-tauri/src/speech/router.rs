//! Routes each dictation to the operating-system engine or the on-device
//! engine based on the session's [`SpeechBackend`], without the callers in
//! `AppCore` knowing which one runs.
//!
//! The router owns the session ids it hands out and remembers which engine
//! each one belongs to, so the two underlying engines' counters can overlap
//! freely.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use super::{
    MicrophoneDevice, SpeechBackend, SpeechEngine, SpeechError, SpeechEventSink, SpeechFuture,
    SpeechSessionId, SpeechStartOptions, SpeechTranscript,
};

#[derive(Clone, Copy, Debug)]
enum Engine {
    System,
    Local,
}

pub struct SelectableSpeechEngine {
    system: Arc<dyn SpeechEngine>,
    local: Arc<dyn SpeechEngine>,
    next_session: AtomicU64,
    sessions: Mutex<HashMap<SpeechSessionId, (Engine, SpeechSessionId)>>,
}

impl SelectableSpeechEngine {
    pub fn new(system: Arc<dyn SpeechEngine>, local: Arc<dyn SpeechEngine>) -> Self {
        Self {
            system,
            local,
            next_session: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn engine(&self, engine: Engine) -> &Arc<dyn SpeechEngine> {
        match engine {
            Engine::System => &self.system,
            Engine::Local => &self.local,
        }
    }
}

impl SpeechEngine for SelectableSpeechEngine {
    fn microphones(&self) -> SpeechFuture<'_, Result<Vec<MicrophoneDevice>, SpeechError>> {
        // The picker always reflects the system list (which includes a
        // "System Default" entry both engines accept). The local engine maps a
        // stored device id to a matching capture device by name and otherwise
        // records the default.
        self.system.microphones()
    }

    fn start(
        &self,
        options: SpeechStartOptions,
        events: SpeechEventSink,
    ) -> SpeechFuture<'_, Result<SpeechSessionId, SpeechError>> {
        Box::pin(async move {
            let (kind, engine) = match options.backend {
                SpeechBackend::System => (Engine::System, self.system.as_ref()),
                SpeechBackend::Local { .. } => (Engine::Local, self.local.as_ref()),
            };
            let inner = engine.start(options, events).await?;
            let session = SpeechSessionId(self.next_session.fetch_add(1, Ordering::Relaxed));
            self.sessions
                .lock()
                .map_err(|_| SpeechError::Backend)?
                .insert(session, (kind, inner));
            Ok(session)
        })
    }

    fn stop(
        &self,
        session: SpeechSessionId,
    ) -> SpeechFuture<'_, Result<SpeechTranscript, SpeechError>> {
        Box::pin(async move {
            let (kind, inner) = self.take(session)?;
            self.engine(kind).stop(inner).await
        })
    }

    fn cancel(&self, session: SpeechSessionId) -> SpeechFuture<'_, Result<(), SpeechError>> {
        Box::pin(async move {
            let (kind, inner) = self.take(session)?;
            self.engine(kind).cancel(inner).await
        })
    }
}

impl SelectableSpeechEngine {
    fn take(&self, session: SpeechSessionId) -> Result<(Engine, SpeechSessionId), SpeechError> {
        self.sessions
            .lock()
            .map_err(|_| SpeechError::Backend)?
            .remove(&session)
            .ok_or(SpeechError::NotRunning)
    }
}
