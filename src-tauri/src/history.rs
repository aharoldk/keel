//! Request history: append-only JSONL in `.keel/history.jsonl` (git-ignored).
//! The URL is stored unresolved (template form) so secrets never leak.

use std::io::{BufRead, Write};
use std::path::Path;

use crate::model::{HistoryPin, HistoryRecord};

fn history_path(root: &Path) -> std::path::PathBuf {
    root.join(".keel").join("history.jsonl")
}

pub fn append(root: &Path, record: &HistoryRecord) -> Result<(), String> {
    let _ = std::fs::create_dir_all(root.join(".keel"));
    let line = serde_json::to_string(record).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path(root))
        .map_err(|e| format!("open history: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("append history: {e}"))
}

/// Newest first.
pub fn list(root: &Path, limit: usize) -> Vec<HistoryRecord> {
    let Ok(file) = std::fs::File::open(history_path(root)) else {
        return vec![];
    };
    let reader = std::io::BufReader::new(file);
    let mut records: Vec<HistoryRecord> = reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    records.reverse();
    records.truncate(limit);
    records
}

pub fn clear(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join(".keel")).map_err(|e| e.to_string())?;
    std::fs::write(history_path(root), "").map_err(|e| format!("clear history: {e}"))?;
    let _ = std::fs::remove_file(pins_path(root));
    Ok(())
}

fn pins_path(root: &Path) -> std::path::PathBuf {
    root.join(".keel").join("history-pins.json")
}

/// Reads pins, dropping any whose `ts` no longer appears in the current history.
pub fn pins(root: &Path) -> Vec<HistoryPin> {
    let Ok(text) = std::fs::read_to_string(pins_path(root)) else {
        return vec![];
    };
    let Ok(all) = serde_json::from_str::<Vec<HistoryPin>>(&text) else {
        return vec![];
    };
    let existing: std::collections::HashSet<(String, Option<String>)> = std::fs::File::open(history_path(root))
        .map(|f| {
            std::io::BufReader::new(f)
                .lines()
                .map_while(Result::ok)
                .filter_map(|l| serde_json::from_str::<HistoryRecord>(&l).ok())
                .map(|r| (r.ts, r.request_path))
                .collect()
        })
        .unwrap_or_default();
    all.into_iter()
        .filter(|p| existing.contains(&(p.ts.clone(), p.request_path.clone())))
        .collect()
}

pub fn pin(root: &Path, pin: HistoryPin) -> Result<(), String> {
    let mut pins = pins(root);
    if !pins.iter().any(|p| p.ts == pin.ts && p.request_path == pin.request_path) {
        pins.push(pin);
    }
    write_pins(root, &pins)
}

pub fn unpin(root: &Path, ts: &str, request_path: Option<&str>) -> Result<(), String> {
    let mut pins = pins(root);
    pins.retain(|p| !(p.ts == ts && p.request_path.as_deref() == request_path));
    write_pins(root, &pins)
}

fn write_pins(root: &Path, pins: &[HistoryPin]) -> Result<(), String> {
    let _ = std::fs::create_dir_all(root.join(".keel"));
    let text = serde_json::to_string_pretty(pins).map_err(|e| e.to_string())?;
    std::fs::write(pins_path(root), text).map_err(|e| format!("write pins: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn record(url: &str) -> HistoryRecord {
        HistoryRecord {
            ts: "2026-01-01T00:00:00Z".into(),
            method: "GET".into(),
            url: url.into(),
            status: Some(200),
            ok: true,
            time_ms: 10.0,
            env: Some("local".into()),
            request_path: Some("users/get-user.yaml".into()),
        }
    }

    #[test]
    fn append_list_clear() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        append(root, &record("{{baseUrl}}/a")).expect("append");
        append(root, &record("{{baseUrl}}/b")).expect("append");
        let records = list(root, 10);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].url, "{{baseUrl}}/b");
        assert_eq!(records[1].url, "{{baseUrl}}/a");
        clear(root).expect("clear");
        assert!(list(root, 10).is_empty());
    }

    #[test]
    fn limit_returns_newest() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        for i in 0..5 {
            append(root, &record(&format!("/{i}"))).expect("append");
        }
        let records = list(root, 2);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].url, "/4");
    }

    #[test]
    fn pins_survive_list_and_drop_on_clear() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        append(root, &record("{{baseUrl}}/a")).expect("append");
        let p = HistoryPin {
            ts: "2026-01-01T00:00:00Z".into(),
            request_path: Some("users/get-user.yaml".into()),
        };
        pin(root, p.clone()).expect("pin");
        pin(root, p.clone()).expect("pin idempotent");
        assert_eq!(pins(root).len(), 1);
        // Unknown pins are dropped on read.
        pin(root, HistoryPin { ts: "ghost".into(), request_path: None }).expect("ghost");
        assert_eq!(pins(root).len(), 1);
        unpin(root, &p.ts, p.request_path.as_deref()).expect("unpin");
        assert!(pins(root).is_empty());
        pin(root, p.clone()).expect("re-pin");
        clear(root).expect("clear");
        assert!(pins(root).is_empty());
    }
}
