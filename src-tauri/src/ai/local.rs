//! First-class local AI support: detect commonly used OpenAI-compatible
//! servers (Ollama, LM Studio, llama.cpp) and, when none is installed, fetch
//! and launch the official Ollama installer.
//!
//! Kivo does not bundle an LLM runtime. Running one is a user choice with real
//! disk/memory cost, so instead this module makes the common local servers
//! one-click to detect and configure, and offers to install Ollama the same
//! way a browser would hand the download to the OS.

use std::{path::PathBuf, process::Command, time::Duration};

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAiServer {
    pub id: String,
    pub name: String,
    /// OpenAI-compatible base URL to store in `aiCustomBaseUrl`.
    pub base_url: String,
    pub running: bool,
    pub models: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAiInstallProgress {
    pub downloaded: u64,
    pub total: u64,
}

/// One candidate per server a user is likely to already run.
const CANDIDATES: &[(&str, &str, &str)] = &[
    ("ollama", "Ollama", "http://localhost:11434/v1"),
    ("lmstudio", "LM Studio", "http://localhost:1234/v1"),
    ("llamacpp", "llama.cpp", "http://localhost:8080/v1"),
];

pub async fn detect_local_servers() -> Vec<LocalAiServer> {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
    else {
        return Vec::new();
    };
    let mut servers = Vec::with_capacity(CANDIDATES.len());
    for (id, name, base_url) in CANDIDATES {
        let models = list_models(&client, base_url).await;
        servers.push(LocalAiServer {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            base_url: (*base_url).to_owned(),
            running: models.is_some(),
            models: models.unwrap_or_default(),
        });
    }
    servers
}

async fn list_models(client: &reqwest::Client, base_url: &str) -> Option<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Entry {
        id: String,
    }
    #[derive(serde::Deserialize)]
    struct Listing {
        data: Vec<Entry>,
    }
    let response = client.get(format!("{base_url}/models")).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let listing: Listing = response.json().await.ok()?;
    Some(listing.data.into_iter().map(|entry| entry.id).collect())
}

/// Official installer URL for the host, or `None` on unsupported platforms.
fn installer_url() -> Option<&'static str> {
    #[cfg(target_os = "windows")]
    return Some("https://ollama.com/download/OllamaSetup.exe");
    #[cfg(target_os = "macos")]
    return Some("https://ollama.com/download/Ollama.dmg");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return None;
}

/// Downloads the official installer and hands it to the OS. The user still
/// approves the install; Kivo never runs it silently.
pub async fn install_runtime(app: &AppHandle) -> Result<(), String> {
    let url = installer_url().ok_or_else(|| {
        "Installing a local AI runtime is only supported on macOS and Windows.".to_owned()
    })?;
    let file_name = if cfg!(target_os = "windows") {
        "kivo-ollama-setup.exe"
    } else {
        "Kivo-Ollama.dmg"
    };
    let path = std::env::temp_dir().join(file_name);

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|_| "The installer could not be downloaded.".to_owned())?;
    if !response.status().is_success() {
        return Err("The installer could not be downloaded.".into());
    }
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|_| "The installer could not be saved.".to_owned())?;
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "The installer download was interrupted.".to_owned())?;
        file.write_all(&chunk)
            .await
            .map_err(|_| "The installer could not be saved.".to_owned())?;
        downloaded += chunk.len() as u64;
        let _ = app.emit(
            "local-ai-install-progress",
            LocalAiInstallProgress { downloaded, total },
        );
    }
    file.flush()
        .await
        .map_err(|_| "The installer could not be saved.".to_owned())?;
    drop(file);
    launch_installer(&path)
}

fn launch_installer(path: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = Command::new(path).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result: std::io::Result<std::process::Child> = Err(std::io::Error::other("unsupported"));
    result
        .map(|_| ())
        .map_err(|_| "The installer could not be opened.".to_owned())
}
