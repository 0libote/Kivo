//! First-class local AI support: detect commonly used OpenAI-compatible
//! servers (Ollama, LM Studio, llama.cpp). Installation stays in the user’s
//! browser through the official vendor download page.
//!
//! Kivo does not bundle an LLM runtime. Running one is a user choice with real
//! disk/memory cost, so instead this module makes the common local servers
//! one-click to detect and configure. Kivo never downloads or executes an installer.

use std::time::Duration;

use futures_util::future::join_all;
use serde::Serialize;

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
    join_all(CANDIDATES.iter().map(|(id, name, base_url)| async {
        let models = list_models(&client, base_url).await;
        LocalAiServer {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            base_url: (*base_url).to_owned(),
            running: models.is_some(),
            models: models.unwrap_or_default(),
        }
    }))
    .await
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
