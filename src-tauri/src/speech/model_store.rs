//! On-device speech-model catalog and download store.
//!
//! Models are multilingual Whisper GGML/GGUF conversions published by
//! `handy-computer` on Hugging Face (Apache-2.0), the same files the
//! `transcribe-cpp` runtime is built for. Every entry is pinned to a commit
//! and verified by SHA-256 before it is treated as installed, so a moved tag
//! or a truncated download can never silently become a model.

use std::{
    collections::HashMap,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

/// Default model when the setting is unset: the best size/accuracy balance for
/// a desktop CPU. See the catalog descriptions for the trade-offs.
pub const DEFAULT_LOCAL_MODEL_ID: &str = "whisper-small";

struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    /// Hugging Face repo + the pinned commit the file was verified against.
    repo: &'static str,
    revision: &'static str,
    filename: &'static str,
    sha256: &'static str,
    size_bytes: u64,
    recommended: bool,
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "whisper-tiny",
        name: "Tiny",
        description: "Fastest and smallest. Good for quick notes on older machines.",
        repo: "handy-computer/whisper-tiny-gguf",
        revision: "2678cc66038359b97c8e6fd6454c56fc9006d571",
        filename: "whisper-tiny-Q5_K_M.gguf",
        sha256: "72cfa8ee436a635a5b6fb373cc056a828b9efe96d32d6eb8769ed3cc5b429719",
        size_bytes: 44_211_616,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-base",
        name: "Base",
        description: "A light, responsive model for everyday dictation.",
        repo: "handy-computer/whisper-base-gguf",
        revision: "30c1704968f7540e112bab3ea3cc4b274902c360",
        filename: "whisper-base-Q5_K_M.gguf",
        sha256: "8e0feb7bc35780353cf31821018e601bb7b7cff6c9a0e17ada5a5db23f4db867",
        size_bytes: 63_786_048,
        recommended: false,
    },
    CatalogEntry {
        id: DEFAULT_LOCAL_MODEL_ID,
        name: "Small",
        description: "Recommended. Noticeably more accurate, still quick on a modern CPU.",
        repo: "handy-computer/whisper-small-gguf",
        revision: "a2073177cb69bd74b9ca9460b852d17fbfd5d68c",
        filename: "whisper-small-Q5_K_M.gguf",
        sha256: "326cd00c3e7217c751667c7c1600eaf7e0de174e186ca2c16b4bf590251c3c3b",
        size_bytes: 193_749_056,
        recommended: true,
    },
    CatalogEntry {
        id: "whisper-medium",
        name: "Medium",
        description: "Higher accuracy for accents and noisy rooms. Slower on CPU.",
        repo: "handy-computer/whisper-medium-gguf",
        revision: "835ad19fff976143d650e35113a6544980cbd983",
        filename: "whisper-medium-Q4_K_M.gguf",
        sha256: "6c9cad9c41d1fa0fcc624898f7ee3485b66127627878081a5655c6f6f3ce20ab",
        size_bytes: 504_102_848,
        recommended: false,
    },
    CatalogEntry {
        id: "whisper-large-v3-turbo",
        name: "Large v3 Turbo",
        description: "Best quality. Needs a capable machine and a one-time download.",
        repo: "handy-computer/whisper-large-v3-turbo-gguf",
        revision: "ceea6c8a94a21ab85be244d311e874a39344dbf5",
        filename: "whisper-large-v3-turbo-Q4_K_M.gguf",
        sha256: "ecfe9b6beb4ab18fef49187cc968cc74b5168b94629c8830e2ca6b794c6e25ed",
        size_bytes: 536_069_728,
        recommended: false,
    },
];

/// A model as the UI sees it (no URL, hash, or filesystem path).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSpeechModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub recommended: bool,
    pub downloaded: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug)]
pub enum ModelStoreError {
    UnknownModel,
    Unavailable,
    Download,
    /// SHA-256 mismatch: the file was removed and the download can be retried.
    Corrupt,
    Io(io::Error),
}

impl fmt::Display for ModelStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownModel => "That speech model is not available.",
            Self::Unavailable => "The speech models folder is unavailable.",
            Self::Download => {
                "The model could not be downloaded. Check your connection and try again."
            }
            Self::Corrupt => {
                "The downloaded model failed its integrity check and was removed. Try again."
            }
            Self::Io(_) => "The speech models folder could not be read or written.",
        })
    }
}

impl std::error::Error for ModelStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ModelStoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

fn catalog_entry(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

pub struct ModelStore {
    directory: PathBuf,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl ModelStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            cancels: Mutex::new(HashMap::new()),
        }
    }

    /// Resolved path for a downloaded model, if the file is present.
    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        let entry = catalog_entry(id)?;
        let path = self.directory.join(entry.filename);
        path.is_file().then_some(path)
    }

    pub fn is_downloaded(&self, id: &str) -> bool {
        self.path_for(id).is_some()
    }

    /// `id` of the first installed model in catalog order, if any.
    pub fn first_downloaded(&self) -> Option<&'static str> {
        CATALOG
            .iter()
            .find(|entry| self.is_downloaded(entry.id))
            .map(|entry| entry.id)
    }

    pub fn list(&self) -> Vec<LocalSpeechModel> {
        CATALOG
            .iter()
            .map(|entry| LocalSpeechModel {
                id: entry.id.to_owned(),
                name: entry.name.to_owned(),
                description: entry.description.to_owned(),
                size_bytes: entry.size_bytes,
                recommended: entry.recommended,
                downloaded: self.is_downloaded(entry.id),
            })
            .collect()
    }

    pub async fn download(&self, app: &AppHandle, id: &str) -> Result<(), ModelStoreError> {
        let entry = catalog_entry(id).ok_or(ModelStoreError::UnknownModel)?;
        let target = self.directory.join(entry.filename);
        if target.is_file() {
            return Ok(());
        }
        tokio::fs::create_dir_all(&self.directory)
            .await
            .map_err(|_| ModelStoreError::Unavailable)?;
        // A leftover `.partial` is resumed rather than discarded: dropping the
        // connection halfway through a 500 MB model should not restart it.
        // The final SHA-256 check still rejects a partial that came from a
        // different file.
        let partial = self.directory.join(format!("{}.partial", entry.filename));

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels
            .lock()
            .map_err(|_| ModelStoreError::Unavailable)?
            .insert(id.to_owned(), Arc::clone(&cancel));
        let result = self.stream_to_file(app, entry, &partial, &cancel).await;
        self.cancels.lock().ok().map(|mut map| map.remove(id));
        if let Err(error) = result {
            // Keep an interrupted download for the next attempt; a cancel is
            // deliberate, so discard it.
            if cancel.load(Ordering::SeqCst) {
                let _ = tokio::fs::remove_file(&partial).await;
            }
            return Err(error);
        }

        // Verify off the async runtime: hashing a few hundred MB is CPU work.
        let expected = entry.sha256.to_owned();
        let verify_path = partial.clone();
        let valid = tokio::task::spawn_blocking(move || verify_sha256(&verify_path, &expected))
            .await
            .map_err(|_| ModelStoreError::Unavailable)?
            .unwrap_or(false);
        if !valid {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(ModelStoreError::Corrupt);
        }
        tokio::fs::rename(&partial, &target)
            .await
            .map_err(|_| ModelStoreError::Unavailable)?;
        let _ = app.emit("local-models-changed", self.list());
        Ok(())
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(map) = self.cancels.lock()
            && let Some(flag) = map.get(id)
        {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Deletes an installed model. The file is removed even if it is currently
    /// selected; the next dictation reports the model as missing.
    pub fn delete(&self, id: &str) -> Result<(), ModelStoreError> {
        let entry = catalog_entry(id).ok_or(ModelStoreError::UnknownModel)?;
        let path = self.directory.join(entry.filename);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn stream_to_file(
        &self,
        app: &AppHandle,
        entry: &CatalogEntry,
        partial: &Path,
        cancel: &AtomicBool,
    ) -> Result<(), ModelStoreError> {
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            entry.repo, entry.revision, entry.filename
        );
        let existing = tokio::fs::metadata(partial)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| ModelStoreError::Download)?;
        let mut request = client.get(&url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|_| ModelStoreError::Download)?;
        let plan = plan_download(
            existing,
            response.status().as_u16(),
            response.content_length(),
            entry.size_bytes,
        )
        .map_err(|_| ModelStoreError::Download)?;

        let (mut file, mut downloaded, total) = match plan {
            // A previous attempt wrote the whole file but never verified it;
            // the hash check below still decides.
            DownloadPlan::Complete => return Ok(()),
            DownloadPlan::Resume { offset, total } => {
                let file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(partial)
                    .await
                    .map_err(|_| ModelStoreError::Unavailable)?;
                (file, offset, total)
            }
            // A server that ignores Range restarts from zero, so truncate.
            DownloadPlan::Restart { total } => {
                let file = tokio::fs::File::create(partial)
                    .await
                    .map_err(|_| ModelStoreError::Unavailable)?;
                (file, 0, total)
            }
        };
        let mut stream = response.bytes_stream();
        let mut last_emit = Instant::now();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                return Err(ModelStoreError::Download);
            }
            let chunk = chunk.map_err(|_| ModelStoreError::Download)?;
            file.write_all(&chunk)
                .await
                .map_err(|_| ModelStoreError::Unavailable)?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                last_emit = Instant::now();
                let _ = app.emit(
                    "local-model-progress",
                    LocalModelProgress {
                        model_id: entry.id.to_owned(),
                        downloaded,
                        total,
                    },
                );
            }
        }
        file.flush()
            .await
            .map_err(|_| ModelStoreError::Unavailable)?;
        drop(file);
        let _ = app.emit(
            "local-model-progress",
            LocalModelProgress {
                model_id: entry.id.to_owned(),
                downloaded,
                total,
            },
        );
        Ok(())
    }
}

/// How a download response maps onto the `.partial` file. `Err(())` is a
/// non-success status the caller turns into a retryable download error.
#[derive(Debug, PartialEq, Eq)]
enum DownloadPlan {
    /// 416: the partial already holds every byte the server has.
    Complete,
    /// 206: append the remaining bytes after `offset`.
    Resume { offset: u64, total: u64 },
    /// 200 (or a server that ignored `Range`): truncate and start over.
    Restart { total: u64 },
}

fn plan_download(
    existing: u64,
    status: u16,
    content_length: Option<u64>,
    fallback_size: u64,
) -> Result<DownloadPlan, ()> {
    if status == 416 {
        return Ok(DownloadPlan::Complete);
    }
    let resumed = status == 206;
    if !resumed && !(200..300).contains(&status) {
        return Err(());
    }
    if resumed {
        Ok(DownloadPlan::Resume {
            offset: existing,
            total: content_length
                .map(|remaining| remaining + existing)
                .unwrap_or(fallback_size),
        })
    } else {
        Ok(DownloadPlan::Restart {
            total: content_length.unwrap_or(fallback_size),
        })
    }
}

fn verify_sha256(path: &Path, expected: &str) -> io::Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    // std::io::copy avoids a hand-rolled buffer loop and streams the file.
    io::copy(&mut file, &mut hasher)?;
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::{DownloadPlan, ModelStore, catalog_entry, plan_download, verify_sha256};

    #[test]
    fn download_plan_resumes_and_restarts_correctly() {
        // 206 appends after the bytes already on disk and reports the real total.
        assert_eq!(
            plan_download(100, 206, Some(900), 1000),
            Ok(DownloadPlan::Resume {
                offset: 100,
                total: 1000
            })
        );
        // 206 without a length falls back to the catalog size.
        assert_eq!(
            plan_download(100, 206, None, 1000),
            Ok(DownloadPlan::Resume {
                offset: 100,
                total: 1000
            })
        );
        // 200 means the server ignored Range: truncate and start over.
        assert_eq!(
            plan_download(100, 200, Some(1000), 1000),
            Ok(DownloadPlan::Restart { total: 1000 })
        );
        // 416 means the partial already covers the file; let verification decide.
        assert_eq!(
            plan_download(1000, 416, None, 1000),
            Ok(DownloadPlan::Complete)
        );
        // Anything else is a retryable failure.
        assert!(plan_download(0, 500, None, 1000).is_err());
        assert!(plan_download(0, 404, None, 1000).is_err());
    }

    #[test]
    fn every_catalog_entry_is_well_formed() {
        for entry in super::CATALOG {
            assert!(!entry.id.is_empty());
            assert!(!entry.filename.is_empty());
            assert!(entry.sha256.len() == 64);
            assert!(entry.revision.len() >= 7);
            assert!(entry.size_bytes > 0);
        }
        assert!(catalog_entry(super::DEFAULT_LOCAL_MODEL_ID).is_some());
        assert!(
            catalog_entry(super::DEFAULT_LOCAL_MODEL_ID)
                .unwrap()
                .recommended
        );
    }

    #[test]
    fn sha256_matches_known_digest() {
        // "abc" -> ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let path = std::env::temp_dir().join("kivo-sha-test.bin");
        std::fs::write(&path, b"abc").unwrap();
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(&path, expected).unwrap());
        assert!(!verify_sha256(&path, &"0".repeat(64)).unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_and_unknown_models_report_not_downloaded() {
        let store = ModelStore::new(std::env::temp_dir().join("kivo-absent-models"));
        assert!(!store.is_downloaded("whisper-small"));
        assert!(store.path_for("does-not-exist").is_none());
        assert!(store.first_downloaded().is_none());
    }
}
