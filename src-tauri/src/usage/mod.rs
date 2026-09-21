//! Local-only AI usage ledger.
//!
//! Records per-generation *counts and identifiers only* — provider, model,
//! task kind, token totals, an estimated cost, and success/failure. Prompt
//! text, responses, transcripts, and keys are never stored, so this preserves
//! Kivo's "no telemetry, nothing leaves the machine" promise while still
//! letting the app show a usage dashboard.
//!
//! The ledger is capped and persisted as one JSON file in the app config
//! directory. Writes are best-effort: usage accounting must never be able to
//! break a generation, so every persistence failure is ignored.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// Hard cap on stored entries. At ~120 bytes each this is well under 3 MB;
/// oldest entries are dropped first so the file cannot grow without bound.
pub const MAX_ENTRIES: usize = 20_000;

/// Milliseconds in a day, for date bucketing and range filters.
const DAY_MS: i64 = 86_400_000;

/// A token count reported by the provider, or estimated from character counts
/// when the provider returns no usage metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// True when the numbers are estimated rather than provider-reported.
    #[serde(default)]
    pub estimated: bool,
}

impl TokenUsage {
    /// A rough fallback (~4 characters per token) used only when the provider
    /// does not report usage. Marked estimated so the UI can say so.
    pub fn estimate(input_chars: usize, output_chars: usize) -> Self {
        Self {
            input_tokens: chars_to_tokens(input_chars),
            output_tokens: chars_to_tokens(output_chars),
            estimated: true,
        }
    }
}

fn chars_to_tokens(chars: usize) -> u64 {
    (chars as u64).div_ceil(4)
}

/// One AI generation attempt (including failed failover attempts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntry {
    /// Unix milliseconds.
    pub timestamp_ms: i64,
    pub provider: String,
    pub model: String,
    /// `writing` | `dictation-cleanup` | `link-summary` | `connection-test`.
    pub kind: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub estimated: bool,
    #[serde(default)]
    pub cost_usd: f64,
    pub ok: bool,
}

/// One grouped row (by provider, model, or kind).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageGroup {
    pub key: String,
    pub requests: u64,
    pub failures: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// One UTC day of usage, for the trend chart.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    /// `YYYY-MM-DD` (UTC).
    pub day: String,
    pub requests: u64,
    pub failures: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// Aggregated, serializable view for the dashboard.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub requests: u64,
    pub failures: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    /// How many recorded requests used estimated rather than reported tokens.
    pub estimated_requests: u64,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
    pub days: Vec<UsageDay>,
    pub by_provider: Vec<UsageGroup>,
    pub by_model: Vec<UsageGroup>,
    pub by_kind: Vec<UsageGroup>,
}

impl UsageSummary {
    fn from_entries(entries: &[UsageEntry]) -> Self {
        use std::collections::BTreeMap;

        let mut summary = Self::default();
        let mut days: BTreeMap<String, UsageDay> = BTreeMap::new();
        let mut providers: BTreeMap<String, UsageGroup> = BTreeMap::new();
        let mut models: BTreeMap<String, UsageGroup> = BTreeMap::new();
        let mut kinds: BTreeMap<String, UsageGroup> = BTreeMap::new();

        for entry in entries {
            summary.requests += 1;
            if !entry.ok {
                summary.failures += 1;
            }
            summary.input_tokens += entry.input_tokens;
            summary.output_tokens += entry.output_tokens;
            summary.cost_usd += entry.cost_usd;
            if entry.estimated {
                summary.estimated_requests += 1;
            }
            summary.first_ms = Some(
                summary
                    .first_ms
                    .map_or(entry.timestamp_ms, |v| v.min(entry.timestamp_ms)),
            );
            summary.last_ms = Some(
                summary
                    .last_ms
                    .map_or(entry.timestamp_ms, |v| v.max(entry.timestamp_ms)),
            );

            let day = days
                .entry(iso_day(entry.timestamp_ms))
                .or_insert_with(|| UsageDay {
                    day: iso_day(entry.timestamp_ms),
                    ..UsageDay::default()
                });
            accumulate_day(day, entry);

            accumulate(
                providers
                    .entry(entry.provider.clone())
                    .or_insert_with(|| UsageGroup {
                        key: entry.provider.clone(),
                        ..UsageGroup::default()
                    }),
                entry,
            );
            accumulate(
                models
                    .entry(entry.model.clone())
                    .or_insert_with(|| UsageGroup {
                        key: entry.model.clone(),
                        ..UsageGroup::default()
                    }),
                entry,
            );
            accumulate(
                kinds
                    .entry(entry.kind.clone())
                    .or_insert_with(|| UsageGroup {
                        key: entry.kind.clone(),
                        ..UsageGroup::default()
                    }),
                entry,
            );
        }

        summary.days = days.into_values().collect();
        summary.by_provider = rank(providers);
        summary.by_model = rank(models);
        summary.by_kind = rank(kinds);
        summary
    }
}

fn accumulate(group: &mut UsageGroup, entry: &UsageEntry) {
    group.requests += 1;
    if !entry.ok {
        group.failures += 1;
    }
    group.input_tokens += entry.input_tokens;
    group.output_tokens += entry.output_tokens;
    group.cost_usd += entry.cost_usd;
}

fn accumulate_day(day: &mut UsageDay, entry: &UsageEntry) {
    day.requests += 1;
    if !entry.ok {
        day.failures += 1;
    }
    day.input_tokens += entry.input_tokens;
    day.output_tokens += entry.output_tokens;
    day.cost_usd += entry.cost_usd;
}

/// Sort groups by spend, then requests, then key — stable and deterministic.
fn rank(groups: std::collections::BTreeMap<String, UsageGroup>) -> Vec<UsageGroup> {
    let mut ranked: Vec<UsageGroup> = groups.into_values().collect();
    ranked.sort_by(|a, b| {
        b.cost_usd
            .partial_cmp(&a.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.requests.cmp(&a.requests))
            .then_with(|| a.key.cmp(&b.key))
    });
    ranked
}

/// `YYYY-MM-DD` in UTC, without pulling in a date library.
fn iso_day(timestamp_ms: i64) -> String {
    let days = timestamp_ms.div_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a civil date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Current Unix time in milliseconds; `0` if the clock predates the epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Start-of-range filter helper: `since_days` days before `now`.
pub fn since_ms(now: i64, days: u32) -> i64 {
    now - i64::from(days) * DAY_MS
}

/// Extract provider-reported token usage from a response body. Handles the
/// OpenAI (`prompt_tokens`/`completion_tokens`), Anthropic Messages
/// (`input_tokens`/`output_tokens`), and Gemini (`usageMetadata` with
/// `promptTokenCount`/`candidatesTokenCount`) shapes. Returns `None` when the
/// provider reported nothing usable, so callers fall back to an estimate.
pub fn parse_provider_usage(body: &serde_json::Value) -> Option<TokenUsage> {
    for key in ["usage", "usageMetadata", "usage_metadata"] {
        if let Some(usage) = body.get(key)
            && let Some(parsed) = parse_token_usage_object(usage)
        {
            return Some(parsed);
        }
    }
    None
}

/// Parse a bare usage object (already unwrapped from `usage`/`usageMetadata`).
pub fn parse_token_usage_object(usage: &serde_json::Value) -> Option<TokenUsage> {
    let input = read_count(
        usage,
        &[
            "prompt_tokens",
            "input_tokens",
            "promptTokenCount",
            "total_input_tokens",
        ],
    )?;
    let output = read_count(
        usage,
        &[
            "completion_tokens",
            "output_tokens",
            "candidatesTokenCount",
            "total_output_tokens",
        ],
    )
    .unwrap_or(0);
    Some(TokenUsage {
        input_tokens: input,
        output_tokens: output,
        estimated: false,
    })
}

fn read_count(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_u64))
}

/// Thread-safe usage ledger.
pub struct UsageStore {
    path: Option<PathBuf>,
    entries: Mutex<Vec<UsageEntry>>,
}

impl UsageStore {
    /// A store that keeps entries in memory only (tests, headless setups).
    pub fn in_memory() -> Arc<Self> {
        Arc::new(Self {
            path: None,
            entries: Mutex::new(Vec::new()),
        })
    }

    /// Load the ledger from `path`, keeping any existing entries.
    pub fn load(path: PathBuf) -> Arc<Self> {
        let entries = read_entries(&path);
        Arc::new(Self {
            path: Some(path),
            entries: Mutex::new(entries),
        })
    }

    /// Append an entry, trim the cap, and persist best-effort.
    pub fn record(&self, entry: UsageEntry) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(entry);
            let overflow = entries.len().saturating_sub(MAX_ENTRIES);
            if overflow > 0 {
                entries.drain(0..overflow);
            }
        }
        self.persist();
    }

    /// Aggregate entries newer than `since_ms` (all entries when `None`).
    pub fn summary(&self, since_ms: Option<i64>) -> UsageSummary {
        let entries = match self.entries.lock() {
            Ok(entries) => entries.clone(),
            Err(_) => Vec::new(),
        };
        let filtered: Vec<UsageEntry> = match since_ms {
            Some(since) => entries
                .into_iter()
                .filter(|entry| entry.timestamp_ms >= since)
                .collect(),
            None => entries,
        };
        UsageSummary::from_entries(&filtered)
    }

    /// Remove every entry and persist the empty ledger.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        self.persist();
    }

    fn persist(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let snapshot = match self.entries.lock() {
            Ok(entries) => entries.clone(),
            Err(_) => return,
        };
        let Ok(serialized) = serde_json::to_vec_pretty(&snapshot) else {
            return;
        };
        // Write-then-rename so a crash mid-write cannot truncate the ledger.
        let temporary = path.with_extension("json.tmp");
        if fs::write(&temporary, serialized).is_ok() {
            let _ = fs::rename(&temporary, path);
        }
    }
}

fn read_entries(path: &Path) -> Vec<UsageEntry> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        provider: &str,
        model: &str,
        kind: &str,
        ok: bool,
        input: u64,
        output: u64,
    ) -> UsageEntry {
        UsageEntry {
            timestamp_ms: 1_700_000_000_000,
            provider: provider.into(),
            model: model.into(),
            kind: kind.into(),
            input_tokens: input,
            output_tokens: output,
            estimated: false,
            cost_usd: 0.01,
            ok,
        }
    }

    #[test]
    fn aggregates_totals_and_groups() {
        let store = UsageStore::in_memory();
        store.record(entry(
            "gemini",
            "gemini-3.8-flash",
            "writing",
            true,
            100,
            20,
        ));
        store.record(entry("zen", "kimi-k2", "writing", true, 50, 10));
        store.record(entry("zen", "kimi-k2", "dictation-cleanup", false, 0, 0));
        let summary = store.summary(None);
        assert_eq!(summary.requests, 3);
        assert_eq!(summary.failures, 1);
        assert_eq!(summary.input_tokens, 150);
        assert_eq!(summary.output_tokens, 30);
        assert!((summary.cost_usd - 0.03).abs() < 1e-9);
        assert_eq!(summary.by_provider.len(), 2);
        assert_eq!(summary.by_kind.len(), 2);
        // Ranked by spend then requests: zen (2 requests) ahead of gemini (1).
        assert_eq!(summary.by_provider[0].key, "zen");
        assert_eq!(summary.by_provider[0].requests, 2);
        assert_eq!(summary.by_provider[0].failures, 1);
    }

    #[test]
    fn range_filter_excludes_older_entries() {
        let store = UsageStore::in_memory();
        let mut old = entry("gemini", "m", "writing", true, 1, 1);
        old.timestamp_ms = 1_000;
        let mut recent = entry("gemini", "m", "writing", true, 2, 2);
        recent.timestamp_ms = 5_000;
        store.record(old);
        store.record(recent);
        assert_eq!(store.summary(Some(4_000)).requests, 1);
        assert_eq!(store.summary(Some(4_000)).input_tokens, 2);
    }

    #[test]
    fn clear_removes_every_entry() {
        let store = UsageStore::in_memory();
        store.record(entry("gemini", "m", "writing", true, 1, 1));
        store.clear();
        assert_eq!(store.summary(None).requests, 0);
    }

    #[test]
    fn caps_stored_entries() {
        let store = UsageStore::in_memory();
        for _ in 0..(MAX_ENTRIES + 10) {
            store.record(entry("gemini", "m", "writing", true, 1, 1));
        }
        // The cap is enforced; the in-memory summary sees the trimmed list.
        assert_eq!(store.summary(None).requests, MAX_ENTRIES as u64);
    }

    #[test]
    fn parses_openai_anthropic_and_gemini_usage() {
        let openai = serde_json::json!({"usage": {"prompt_tokens": 12, "completion_tokens": 7}});
        assert_eq!(
            parse_provider_usage(&openai),
            Some(TokenUsage {
                input_tokens: 12,
                output_tokens: 7,
                estimated: false
            })
        );

        let anthropic = serde_json::json!({"usage": {"input_tokens": 3, "output_tokens": 9}});
        assert_eq!(parse_provider_usage(&anthropic).unwrap().output_tokens, 9);

        let gemini = serde_json::json!({"usageMetadata": {"promptTokenCount": 21, "candidatesTokenCount": 4}});
        assert_eq!(
            parse_provider_usage(&gemini),
            Some(TokenUsage {
                input_tokens: 21,
                output_tokens: 4,
                estimated: false
            })
        );

        assert_eq!(parse_provider_usage(&serde_json::json!({"foo": 1})), None);
    }

    #[test]
    fn estimates_tokens_from_characters() {
        let usage = TokenUsage::estimate(8, 3);
        assert_eq!(usage.input_tokens, 2);
        assert_eq!(usage.output_tokens, 1);
        assert!(usage.estimated);
    }

    #[test]
    fn formats_iso_days() {
        assert_eq!(iso_day(0), "1970-01-01");
        assert_eq!(iso_day(1_700_000_000_000), "2023-11-14");
        assert_eq!(iso_day(-86_400_000), "1969-12-31");
    }

    #[test]
    fn persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("kivo-usage-test-{}", now_ms()));
        let path = dir.join("usage.json");
        let store = UsageStore::load(path.clone());
        store.record(entry("gemini", "m", "writing", true, 5, 5));
        let reloaded = UsageStore::load(path);
        assert_eq!(reloaded.summary(None).requests, 1);
        assert_eq!(reloaded.summary(None).input_tokens, 5);
        let _ = fs::remove_dir_all(dir);
    }
}
