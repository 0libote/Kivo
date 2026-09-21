//! Link content fetching for non-Gemini providers.
//!
//! Gemini summarizes links server-side (URL context for webpages, video input
//! for YouTube). Zen, Go, and Custom endpoints have no equivalent retrieval
//! primitive, so Kivo fetches the readable content itself and summarizes the
//! fetched text with the configured provider.
//!
//! Only invoked for an explicit user action (`Summarize link…` or a selected
//! URL). `LinkSource::parse` already rejects private hosts (localhost, IPs,
//! `.local`/`.internal`, credentials, non-http(s)), and fetching stays within
//! those validated URLs. Fetched content is treated as untrusted data by the
//! summarize prompt, never as instructions.

use std::time::Duration;

use super::{LinkSource, LinkSourceKind};

/// Cap on downloaded bodies so a huge page or transcript cannot blow up memory.
const MAX_BODY_BYTES: usize = 2_000_000;
/// Cap on extracted text sent to the model (well under the 200k-char summary
/// limit, keeping token spend predictable).
pub const MAX_FETCHED_CHARS: usize = 30_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkFetchError;

impl std::fmt::Display for LinkFetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("could not read link content")
    }
}

impl std::error::Error for LinkFetchError {}

fn fetch_client() -> Result<reqwest::Client, LinkFetchError> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|_| LinkFetchError)
}

fn user_agent() -> String {
    format!(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36 Kivo/{}",
        env!("CARGO_PKG_VERSION")
    )
}

/// Fetch readable text for any validated link source.
pub async fn fetch_link_text(source: &LinkSource) -> Result<String, LinkFetchError> {
    // Revalidate even if a caller constructed LinkSource directly.
    let validated = LinkSource::parse(&source.url).map_err(|_| LinkFetchError)?;
    if validated.kind != source.kind {
        return Err(LinkFetchError);
    }
    match validated.kind {
        LinkSourceKind::Website => fetch_website_text(&validated.url).await,
        LinkSourceKind::Youtube => fetch_youtube_transcript(&validated.url).await,
    }
}

pub async fn fetch_website_text(url: &str) -> Result<String, LinkFetchError> {
    let client = fetch_client()?;
    let response = client
        .get(url)
        .header("user-agent", user_agent())
        .header(
            "accept",
            "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.1",
        )
        .header("accept-language", "en-US,en;q=0.9")
        // reqwest is built without decompression features, so request identity
        // explicitly rather than receiving gzip bytes we cannot decode.
        .header("accept-encoding", "identity")
        .send()
        .await
        .map_err(|_| LinkFetchError)?;
    if !response.status().is_success() {
        return Err(LinkFetchError);
    }
    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        let content_type = content_type.to_ascii_lowercase();
        if !content_type.contains("html")
            && !content_type.contains("text")
            && !content_type.contains("xml")
            && !content_type.contains("xhtml")
        {
            return Err(LinkFetchError);
        }
    }
    let bytes = response.bytes().await.map_err(|_| LinkFetchError)?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(LinkFetchError);
    }
    let html = String::from_utf8_lossy(&bytes);
    let text = html_to_text(&html);
    truncate_text(&text).ok_or(LinkFetchError)
}

pub async fn fetch_youtube_transcript(video_url: &str) -> Result<String, LinkFetchError> {
    let video_id = video_id_from_url(video_url).ok_or(LinkFetchError)?;
    let client = fetch_client()?;
    let watch_url = format!("https://www.youtube.com/watch?v={video_id}");
    let response = client
        .get(&watch_url)
        .header("user-agent", user_agent())
        .header("accept-language", "en-US,en;q=0.9")
        .header("accept-encoding", "identity")
        .send()
        .await
        .map_err(|_| LinkFetchError)?;
    if !response.status().is_success() {
        return Err(LinkFetchError);
    }
    let bytes = response.bytes().await.map_err(|_| LinkFetchError)?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(LinkFetchError);
    }
    let watch_html = String::from_utf8_lossy(&bytes);
    let track_urls = extract_caption_track_urls(&watch_html);
    if track_urls.is_empty() {
        return Err(LinkFetchError);
    }
    // Manual tracks are listed before auto-generated ones; try in order.
    for base_url in track_urls.iter().take(4) {
        if let Ok(transcript) = fetch_caption_track(&client, base_url).await
            && !transcript.trim().is_empty()
        {
            return truncate_text(&transcript).ok_or(LinkFetchError);
        }
    }
    Err(LinkFetchError)
}

async fn fetch_caption_track(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<String, LinkFetchError> {
    // Prefer structured JSON when the endpoint supports it, then fall back to
    // the XML / srv3 payload.
    for suffix in ["&fmt=json3", "&fmt=vtt", ""] {
        let url = if suffix.is_empty() || base_url.contains("fmt=") {
            base_url.to_owned()
        } else {
            format!("{base_url}{suffix}")
        };
        let response = client
            .get(&url)
            .header("user-agent", user_agent())
            .header("accept-language", "en-US,en;q=0.9")
            .header("accept-encoding", "identity")
            .send()
            .await
            .map_err(|_| LinkFetchError)?;
        if !response.status().is_success() {
            continue;
        }
        let bytes = response.bytes().await.map_err(|_| LinkFetchError)?;
        if bytes.is_empty() || bytes.len() > MAX_BODY_BYTES {
            continue;
        }
        let body = String::from_utf8_lossy(&bytes);
        if let Some(transcript) = parse_json3_transcript(&body)
            .or_else(|| parse_timedtext_xml(&body))
            .or_else(|| parse_vtt_transcript(&body))
        {
            let transcript = transcript.trim().to_owned();
            if transcript.chars().count() >= 50 {
                return Ok(transcript);
            }
        }
    }
    Err(LinkFetchError)
}

pub fn video_id_from_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !matches!(
        parsed.host_str()?,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com"
    ) {
        return None;
    }
    if parsed.path() != "/watch" {
        return None;
    }
    for (key, value) in parsed.query_pairs() {
        if key == "v"
            && value.len() == 11
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Some(value.into_owned());
        }
    }
    None
}

/// Collect `timedtext` caption track URLs from a YouTube watch page.
/// Manual tracks precede auto-generated ones in the page; order is preserved.
pub fn extract_caption_track_urls(watch_html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut search_from = 0;
    while let Some(relative) = watch_html[search_from..].find("\"baseUrl\":\"") {
        let start = search_from + relative + "\"baseUrl\":\"".len();
        let rest = &watch_html[start..];
        let mut end = None;
        let mut escaped = false;
        for (index, byte) in rest.bytes().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == b'"' {
                end = Some(index);
                break;
            }
        }
        let Some(end) = end else { break };
        let raw = &rest[..end];
        search_from = start + end + 1;
        if !raw.contains("timedtext") {
            continue;
        }
        let url = raw
            .replace("\\u0026", "&")
            .replace("\\/", "/")
            .replace("\\\\", "\\");
        if url.starts_with("https://www.youtube.com/api/timedtext")
            && !urls.contains(&url)
            && urls.len() < 8
        {
            urls.push(url);
        }
    }
    urls
}

/// YouTube `fmt=json3` payload: `{"events":[{"segs":[{"utf8":"…"}]}]}`.
pub fn parse_json3_transcript(body: &str) -> Option<String> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let events = value.get("events")?.as_array()?;
    let mut parts = Vec::new();
    for event in events {
        let Some(segs) = event.get("segs").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for seg in segs {
            if let Some(text) = seg.get("utf8").and_then(serde_json::Value::as_str) {
                let text = text.replace('\n', " ");
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(collapse_whitespace(&parts.join(" ")))
}

/// YouTube XML / srv3 payload: `<text …>…</text>` cues (entities escaped).
pub fn parse_timedtext_xml(body: &str) -> Option<String> {
    if !body.contains("<text") && !body.contains("<p") {
        return None;
    }
    let mut parts = Vec::new();
    for tag in ["text", "p"] {
        let mut search_from = 0;
        while let Some(relative) = body[search_from..].find(&format!("<{tag}")) {
            let cue_start = search_from + relative;
            let Some(content_start) = body[cue_start..].find('>').map(|i| cue_start + i + 1) else {
                break;
            };
            let close = format!("</{tag}>");
            let Some(relative_end) = body[content_start..].find(&close) else {
                break;
            };
            let cue_end = content_start + relative_end;
            let cue = strip_tags(&body[content_start..cue_end]);
            let cue = decode_html_entities(&cue);
            let cue = collapse_whitespace(cue.trim());
            if !cue.is_empty() {
                parts.push(cue);
            }
            search_from = cue_end + close.len();
        }
        if !parts.is_empty() {
            break;
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" "))
}

/// WebVTT payload (`WEBVTT` header, `00:00.000 --> 00:02.000` cues).
pub fn parse_vtt_transcript(body: &str) -> Option<String> {
    if !body.trim_start().starts_with("WEBVTT") {
        return None;
    }
    let mut parts = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("WEBVTT")
            || line.starts_with("NOTE")
            || line.contains("-->")
            || line.starts_with("STYLE")
            || line.starts_with("REGION")
        {
            continue;
        }
        let cue = collapse_whitespace(&decode_html_entities(&strip_tags(line)));
        if !cue.is_empty() && !parts.last().is_some_and(|last| last == &cue) {
            parts.push(cue);
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" "))
}

fn truncate_text(text: &str) -> Option<String> {
    let trimmed = collapse_whitespace(text.trim());
    if trimmed.chars().count() < 50 {
        return None;
    }
    if trimmed.chars().count() <= MAX_FETCHED_CHARS {
        return Some(trimmed);
    }
    let truncated: String = trimmed.chars().take(MAX_FETCHED_CHARS).collect();
    Some(truncated)
}

/// Best-effort readable-text extraction without an HTML parser dependency.
pub fn html_to_text(html: &str) -> String {
    let without_comments = remove_html_comments(html);
    let without_blocks = remove_tag_blocks(
        &without_comments,
        &["script", "style", "noscript", "svg", "nav", "footer"],
    );
    let without_tags = strip_tags(&without_blocks);
    collapse_whitespace(&decode_html_entities(&without_tags))
}

fn remove_html_comments(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        let after = &rest[start + "<!--".len()..];
        match after.find("-->") {
            Some(end) => rest = &after[end + "-->".len()..],
            None => return output,
        }
    }
    output.push_str(rest);
    output
}

fn remove_tag_blocks(html: &str, tags: &[&str]) -> String {
    let mut output = html.to_owned();
    for tag in tags {
        loop {
            let lower = output.to_lowercase();
            let Some(open) = lower.find(&format!("<{tag}")) else {
                break;
            };
            let Some(open_end) = output[open..].find('>').map(|i| open + i + 1) else {
                break;
            };
            let close = format!("</{tag}>");
            let Some(relative_close) = lower[open_end..].find(&close) else {
                output.truncate(open);
                break;
            };
            let close_end = open_end + relative_close + close.len();
            output.replace_range(open..close_end, " ");
        }
    }
    output
}

fn strip_tags(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_quote: Option<char> = None;
    for ch in html.chars() {
        if in_tag {
            if let Some(quote) = in_quote {
                if ch == quote {
                    in_quote = None;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_quote = Some(ch);
                continue;
            }
            if ch == '>' {
                in_tag = false;
                output.push(' ');
            }
            continue;
        }
        if ch == '<' {
            in_tag = true;
            continue;
        }
        output.push(ch);
    }
    output
}

fn decode_html_entities(text: &str) -> String {
    let mut output = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    // Numeric character references: &#123; and &#x1F;.
    while let Some(start) = output.find("&#") {
        let Some(end) = output[start..].find(';').map(|i| start + i) else {
            break;
        };
        let entity = output[start + 2..end].to_owned();
        let decoded = if let Some(hex) = entity
            .strip_prefix('x')
            .or_else(|| entity.strip_prefix('X'))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else {
            entity.parse::<u32>().ok().and_then(char::from_u32)
        };
        let Some(decoded) = decoded else {
            break;
        };
        output.replace_range(start..=end, &decoded.to_string());
    }
    output
}

fn collapse_whitespace(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !output.is_empty() {
            output.push(' ');
        }
        pending_space = false;
        output.push(ch);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_readable_text_without_scripts_or_tags() {
        let html = r#"<!doctype html><html><head><title>Example</title><style>body{color:red}</style></head><body><!-- comment --><h1>Hello &amp; welcome</h1><script>alert(1)</script><p>This is <a href="/x">an article</a> with&nbsp;entities &#33;</p></body></html>"#;
        let text = html_to_text(html);
        assert!(text.contains("Hello & welcome"));
        assert!(text.contains("an article"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("comment"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn rejects_thin_pages_and_truncates_long_ones() {
        assert!(truncate_text("  short  ").is_none());
        let long = "word ".repeat(10_000);
        let truncated = truncate_text(&long).unwrap();
        assert_eq!(truncated.chars().count(), MAX_FETCHED_CHARS);
    }

    #[test]
    fn extracts_video_id_from_canonical_watch_urls() {
        assert_eq!(
            video_id_from_url("https://www.youtube.com/watch?v=9hE5-98ZeCg"),
            Some("9hE5-98ZeCg".into())
        );
        assert!(video_id_from_url("https://www.youtube.com/watch?v=short").is_none());
        assert!(video_id_from_url("https://youtu.be/9hE5-98ZeCg").is_none());
    }

    #[test]
    fn extracts_caption_track_urls_in_page_order() {
        let html = r#"{"captions":{"playerCaptionsTracklistRenderer":{"captionTracks":[{"baseUrl":"https://www.youtube.com/api/timedtext?v=abc\u0026lang=en","languageCode":"en"},{"baseUrl":"https://www.youtube.com/api/timedtext?v=abc\u0026kind=asr\u0026lang=en","languageCode":"en","kind":"asr"}]}}}"#;
        let urls = extract_caption_track_urls(html);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("lang=en"));
        assert!(!urls[0].contains("\\u0026"));
        assert!(urls[1].contains("kind=asr"));
    }

    #[test]
    fn parses_json3_xml_and_vtt_transcripts() {
        let json3 = r#"{"events":[{"segs":[{"utf8":"Hello "},{"utf8":"world"}]},{"segs":[{"utf8":"Second line"}]}]}"#;
        assert_eq!(
            parse_json3_transcript(json3).unwrap(),
            "Hello world Second line"
        );
        let xml = r#"<?xml version="1.0"?><transcript><text start="0" dur="2">Hello &amp; welcome</text><text start="2" dur="2">Second line</text></transcript>"#;
        assert_eq!(
            parse_timedtext_xml(xml).unwrap(),
            "Hello & welcome Second line"
        );
        let vtt = "WEBVTT\n\n00:00.000 --> 00:02.000\nHello world\n\n00:02.000 --> 00:04.000\nSecond line\n";
        assert_eq!(
            parse_vtt_transcript(vtt).unwrap(),
            "Hello world Second line"
        );
        assert!(parse_json3_transcript("not json").is_none());
    }

    #[tokio::test]
    async fn fetches_and_extracts_article_text_over_http() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let body = "<html><head><title>Test</title><script>evil()</script></head><body><article><h1>Readable headline</h1><p>This is a sufficiently long article body with real content for extraction.</p></article></body></html>";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        let text = fetch_website_text(&format!("http://127.0.0.1:{port}/article"))
            .await
            .unwrap();
        let _ = worker.join();
        assert!(text.contains("Readable headline"));
        assert!(text.contains("sufficiently long article"));
        assert!(!text.contains("evil"));
    }
}
