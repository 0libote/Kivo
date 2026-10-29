//! Gemini prompts and a bounded REST call run on a worker thread.

use std::io::Write;
use std::process::{Command, Stdio};

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
        Action::Proofread => {
            "Fix spelling, grammar, and punctuation, changing as little as possible."
        }
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

/// Keep both the key and source text off the process command line.
fn curl_config(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let key = api_key.trim();
    if key.is_empty() || key.chars().any(char::is_control) {
        return Err("Set a valid Gemini API key in Settings first.".to_string());
    }
    if model.trim().is_empty() {
        return Err("Set a Gemini model in Settings first.".to_string());
    }
    let body = serde_json::json!({
        "model": model.trim(),
        "store": false,
        "input": prompt,
    });
    Ok(format!(
        "header = {}\nheader = \"Content-Type: application/json\"\nheader = \"Api-Revision: 2026-05-20\"\ndata = {}\n",
        serde_json::to_string(&format!("x-goog-api-key: {key}")).unwrap(),
        serde_json::to_string(&body.to_string()).unwrap(),
    ))
}

pub fn run_blocking(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let config = curl_config(api_key, model, prompt)?;
    let mut command = Command::new("curl");
    command
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--max-time",
            "20",
            "--config",
            "-",
        ])
        .arg("https://generativelanguage.googleapis.com/v1beta/interactions")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start curl: {e}"))?;
    let written = child.stdin.take().unwrap().write_all(config.as_bytes());
    if let Err(error) = written {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("Could not send request to curl: {error}"));
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Network error: {e}"))?;
    if !output.status.success() {
        let detail = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .ok()
            .and_then(|body| {
                body.get("error")?
                    .get("message")?
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| {
                "Gemini request failed. Check your connection, key, model, and quota.".to_string()
            });
        return Err(detail.chars().take(300).collect());
    }
    let response =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Bad response: {e}"))?;
    parse_response(&response)
}

fn parse_response(response: &serde_json::Value) -> Result<String, String> {
    if response.get("status").and_then(|value| value.as_str()) != Some("completed") {
        return Err("Gemini did not complete the request.".to_string());
    }
    let text: String = response
        .get("steps")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|step| step.get("type").and_then(|value| value.as_str()) == Some("model_output"))
        .filter_map(|step| step.get("content")?.as_array())
        .flatten()
        .filter(|part| part.get("type").and_then(|value| value.as_str()) == Some("text"))
        .filter_map(|part| part.get("text")?.as_str())
        .collect();
    if text.trim().is_empty() {
        return Err("Gemini returned no text.".to_string());
    }
    Ok(text.trim().to_owned())
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

    #[test]
    fn reads_completed_model_text_without_exposing_thoughts_or_input() {
        let response = serde_json::json!({
            "status": "completed",
            "steps": [
                {"type": "user_input", "content": [{"type": "text", "text": "source"}]},
                {"type": "model_output", "content": [
                    {"type": "thought", "text": "private reasoning"},
                    {"type": "text", "text": "  Hello"},
                    {"type": "text", "text": " world  "}
                ]}
            ]
        });
        assert_eq!(parse_response(&response).unwrap(), "Hello world");
        assert!(parse_response(&serde_json::json!({"status": "in_progress"})).is_err());
        assert!(parse_response(&serde_json::json!({"status": "completed", "steps": []})).is_err());
    }

    #[test]
    fn request_config_preserves_large_private_text_and_rejects_header_injection() {
        let prompt = format!("Quotes: \"hello\"\nUnicode: café\0{}", "x".repeat(200_000));
        let config = curl_config("test-key", "new-model", &prompt).unwrap();
        let data = config
            .lines()
            .find_map(|line| line.strip_prefix("data = "))
            .unwrap();
        let encoded_body: String = serde_json::from_str(data).unwrap();
        let body: serde_json::Value = serde_json::from_str(&encoded_body).unwrap();
        assert_eq!(body["input"], prompt);
        assert_eq!(body["model"], "new-model");
        assert_eq!(body["store"], false);
        assert_eq!(config.lines().count(), 4);
        assert!(curl_config("key\r\ninjected: value", MODEL, "text").is_err());
        assert!(curl_config("", MODEL, "text").is_err());
        assert!(curl_config("key", " ", "text").is_err());
    }
}
