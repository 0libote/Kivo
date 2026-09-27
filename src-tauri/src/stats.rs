//! Local, text-free daily usage statistics.
//!
//! Rows contain aggregate counters only. In particular, transcript and prompt
//! strings are not representable in this module's public data model.
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io::Write, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};

pub const STATS_SCHEMA_VERSION: u32 = 1;

pub fn utc_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400;
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn utc_date_days_ago(days_ago: u16) -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let day = seconds / 86_400;
    let target = day.saturating_sub(u64::from(days_ago));
    let z = target as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatsDocument {
    pub schema_version: u32,
    pub rows: Vec<StatsRow>,
}

impl Default for StatsDocument {
    fn default() -> Self {
        Self {
            schema_version: STATS_SCHEMA_VERSION,
            rows: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatsRow {
    pub date: String,
    pub kind: String,
    pub action: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub engine: Option<String>,
    /// Stable AppCoreError code; never an error message or request text.
    pub error_category: Option<String>,
    pub count: u64,
    pub ok: u64,
    pub fail: u64,
    pub cancel: u64,
    pub failovers_rescued: u64,
    pub chars_in: u64,
    pub chars_out: u64,
    pub ms_sum: u64,
    pub ms_max: u64,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub cost_micro_usd: Option<u64>,
}

pub struct StatsStore {
    path: PathBuf,
    document: Mutex<StatsDocument>,
}

impl StatsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let document = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<StatsDocument>(&bytes).ok())
            .and_then(validate_and_normalize)
            .unwrap_or_default();
        Self {
            path,
            document: Mutex::new(document),
        }
    }

    pub fn snapshot(&self) -> StatsDocument {
        self.document
            .lock()
            .map(|doc| doc.clone())
            .unwrap_or_default()
    }

    pub fn record(&self, row: StatsRow) {
        if let Ok(mut document) = self.document.lock() {
            if let Some(existing) = document.rows.iter_mut().find(|candidate| {
                candidate.date == row.date
                    && candidate.kind == row.kind
                    && candidate.action == row.action
                    && candidate.provider == row.provider
                    && candidate.model == row.model
                    && candidate.engine == row.engine
                    && candidate.error_category == row.error_category
            }) {
                existing.count += row.count;
                existing.ok += row.ok;
                existing.fail += row.fail;
                existing.cancel += row.cancel;
                existing.failovers_rescued += row.failovers_rescued;
                existing.chars_in += row.chars_in;
                existing.chars_out += row.chars_out;
                existing.ms_sum += row.ms_sum;
                existing.ms_max = existing.ms_max.max(row.ms_max);
                existing.tokens_in = add_optional(existing.tokens_in, row.tokens_in);
                existing.tokens_out = add_optional(existing.tokens_out, row.tokens_out);
                existing.cost_micro_usd = add_optional(existing.cost_micro_usd, row.cost_micro_usd);
            } else {
                document.rows.push(row);
            }
            document.rows.sort_by(|a, b| a.date.cmp(&b.date));
            let _ = self.save(&document);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut document) = self.document.lock() {
            *document = StatsDocument {
                schema_version: STATS_SCHEMA_VERSION,
                rows: Vec::new(),
            };
            let _ = self.save(&document);
        }
    }

    pub fn prune_before(&self, date: &str) {
        if let Ok(mut document) = self.document.lock() {
            document.rows.retain(|row| row.date.as_str() >= date);
            let _ = self.save(&document);
        }
    }

    fn save(&self, document: &StatsDocument) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(document)?)?;
        file.sync_all()?;
        fs::rename(tmp, &self.path)
    }
}

fn add_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    }
}

fn validate_and_normalize(mut document: StatsDocument) -> Option<StatsDocument> {
    if document.schema_version > STATS_SCHEMA_VERSION {
        return None;
    }
    document.schema_version = STATS_SCHEMA_VERSION;
    document
        .rows
        .retain(|row| row.date.len() == 10 && row.date.as_bytes().get(4) == Some(&b'-'));
    Some(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "kivo-stats-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn records_coalesce_and_survive_reload_without_text_fields() {
        let path = test_path();
        let store = StatsStore::new(&path);
        store.record(StatsRow {
            date: "2026-09-27".into(),
            kind: "writing".into(),
            action: "rewrite".into(),
            count: 1,
            ok: 1,
            ms_sum: 300,
            tokens_in: Some(20),
            ..Default::default()
        });
        store.record(StatsRow {
            date: "2026-09-27".into(),
            kind: "writing".into(),
            action: "rewrite".into(),
            count: 1,
            fail: 1,
            ms_sum: 400,
            tokens_in: Some(25),
            ..Default::default()
        });
        let loaded = StatsStore::new(&path).snapshot();
        assert_eq!(loaded.rows.len(), 1);
        assert_eq!(loaded.rows[0].count, 2);
        assert_eq!(loaded.rows[0].ok, 1);
        assert_eq!(loaded.rows[0].fail, 1);
        assert_eq!(loaded.rows[0].ms_sum, 700);
        assert_eq!(loaded.rows[0].tokens_in, Some(45));
        let bytes = fs::read_to_string(&path).unwrap();
        assert!(!bytes.contains("transcript"));
        assert!(!bytes.contains("prompt"));
        // All string-valued fields are bounded identifiers, never request text.
        assert!(
            loaded.rows[0]
                .action
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-')
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_future_schema_and_prunes_by_iso_date() {
        let path = test_path();
        fs::write(&path, r#"{"schemaVersion":99,"rows":[]}"#).unwrap();
        let store = StatsStore::new(&path);
        assert_eq!(store.snapshot().schema_version, STATS_SCHEMA_VERSION);
        store.record(StatsRow {
            date: "2026-01-01".into(),
            count: 1,
            ..Default::default()
        });
        store.record(StatsRow {
            date: "2026-09-27".into(),
            count: 1,
            ..Default::default()
        });
        store.prune_before("2026-09-01");
        assert_eq!(store.snapshot().rows.len(), 1);
        store.clear();
        assert!(store.snapshot().rows.is_empty());
        let _ = fs::remove_file(path);
    }
}
