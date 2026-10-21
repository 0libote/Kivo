//! Minimal Gemini prompt builders + blocking REST call.
//! Mirrors `src-tauri/src/ai` semantics (model, store:false, deadlines) without Tauri.

pub const MODEL: &str = "gemini-3.8-flash";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Proofread,
    Rewrite,
    Friendly,
    Professional,
    Concise,
    Custom,
    Summarize,
    KeyPoints,
    Chat,
}

impl Action {
    pub const ALL: &'static [(&'static str, Action)] = &[
        ("Proofread", Action::Proofread),
        ("Rewrite", Action::Rewrite),
        ("Friendly", Action::Friendly),
        ("Professional", Action::Professional),
        ("Concise", Action::Concise),
        ("Summarize", Action::Summarize),
        ("Key points", Action::KeyPoints),
        ("Quick chat", Action::Chat),
        ("Custom", Action::Custom),
    ];

    pub fn replaces_selection(self) -> bool {
        !matches!(self, Self::Summarize | Self::KeyPoints | Self::Chat)
    }
}

pub fn prompt_for(action: Action, source: &str, custom: Option<&str>) -> Result<String, String> {
    if source.trim().is_empty() {
        return Err("Nothing to process.".to_string());
    }
    if action == Action::Summarize && source.chars().count() > 200_000 {
        return Err("Text too long for Summarize (200,000 char limit).".to_string());
    }
    let instruction = match action {
        Action::Proofread => "Fix spelling, grammar, and punctuation, changing as little as possible.",
        Action::Rewrite => "Rewrite for clarity keeping meaning.",
        Action::Friendly => "Rewrite in a friendly tone keeping meaning.",
        Action::Professional => "Rewrite in a professional tone keeping meaning.",
        Action::Concise => "Make it concise keeping all key points.",
        Action::Summarize => "Summarize the key points briefly.",
        Action::KeyPoints => "List the key points as bullets.",
        Action::Chat => "Answer helpfully and briefly.",
        Action::Custom => custom.unwrap_or("Follow the instruction."),
    };
    Ok(format!("{instruction}\n\n---\n{source}"))
}

/// Synchronous Gemini call via the Interactions API. Runs on a worker thread
/// from the UI so the frame loop never blocks.
pub fn run_blocking(api_key: &str, prompt: &str, link_url: Option<&str>) -> Result<String, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("Set your Gemini API key in Settings first.".to_string());
    }
    let client = reqwest_blocking_client()?;
    let body = serde_json::json!({
        "model": MODEL,
        "store": false,
        "input": prompt,
        "context": link_url,
    });
    let res: serde_json::Value = client
        .post("https://generativelanguage.googleapis.com/v1beta/interactions")
        .header("x-goog-api-key", key)
        .json(&body)
        .send()
        .map_err(|e| format!("Network error: {e}"))?
        .json()
        .map_err(|e| format!("Bad response: {e}"))?;
    res.get("output_text")
        .or_else(|| res.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Gemini returned no text.".to_string())
}

fn reqwest_blocking_client() -> Result<ReqClient, String> {
    Ok(ReqClient)
}

// Tiny std-only HTTP shim so `cargo check` works without new deps; the real
// network path is behind the `blocking-net` hookup below.
// To keep the one-go port dependency-light we shell out to `curl` when
// available, which every supported macOS/Windows install has.
struct ReqClient;
struct ReqBuilder {
    url: String,
    key: String,
    body: serde_json::Value,
}
impl ReqClient {
    fn post(&self, url: &str) -> ReqBuilder {
        ReqBuilder { url: url.to_string(), key: String::new(), body: serde_json::Value::Null }
    }
}
impl ReqBuilder {
    fn header(mut self, _: &str, key: &str) -> Self {
        self.key = key.to_string();
        self
    }
    fn json(mut self, body: &serde_json::Value) -> Self {
        self.body = body.clone();
        self
    }
    fn send(self) -> Result<ReqResponse, String> {
        let out = std::process::Command::new("curl")
            .args(["-sS", "-m", "20", "-X", "POST"])
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-H")
            .arg(format!("x-goog-api-key: {}", self.key))
            .arg("-d")
            .arg(self.body.to_string())
            .arg(&self.url)
            .output()
            .map_err(|e| format!("Network error: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "Network error: curl failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(ReqResponse { bytes: out.stdout })
    }
}
struct ReqResponse {
    bytes: Vec<u8>,
}
impl ReqResponse {
    fn json(self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.bytes).map_err(|e| format!("Bad response: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_rejects_empty() {
        assert!(prompt_for(Action::Proofread, "   ", None).is_err());
    }

    #[test]
    fn summarize_limit_matches_tauri() {
        let long = "x".repeat(200_001);
        assert!(prompt_for(Action::Summarize, &long, None).is_err());
    }
}
