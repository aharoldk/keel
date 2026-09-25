//! Runner data files. CSV (header row) or a JSON array of objects.
//! Not a Keel document — never written as request YAML.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ROWS: usize = 1000;

/// One empty row when `path` is `None`, so a run with no data file is a single pass.
pub fn iterations(path: Option<&str>) -> Result<Vec<BTreeMap<String, String>>, String> {
    let Some(path) = path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(vec![BTreeMap::new()]);
    };
    load(Path::new(path))
}

pub fn load(path: &Path) -> Result<Vec<BTreeMap<String, String>>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("data file: {e}"))?;
    if !meta.is_file() {
        return Err(format!("`{}` is not a file", path.display()));
    }
    if meta.len() > MAX_BYTES {
        return Err("data file is larger than 5 MB".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("read data file: {e}"))?;
    let rows = if looks_like_json(&text) {
        parse_json(&text)?
    } else {
        parse_csv(&text)?
    };
    if rows.is_empty() {
        return Err("data file has no rows".into());
    }
    if rows.len() > MAX_ROWS {
        return Err(format!("data file has more than {MAX_ROWS} rows"));
    }
    Ok(rows)
}

/// Drops keys that name an environment secret so a cell cannot shadow the keychain.
pub fn without_secrets(
    row: &BTreeMap<String, String>,
    secret_names: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    row.iter()
        .filter(|(k, _)| !secret_names.contains(k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn looks_like_json(text: &str) -> bool {
    text.trim_start().starts_with('[') || text.trim_start().starts_with('{')
}

fn parse_json(text: &str) -> Result<Vec<BTreeMap<String, String>>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("invalid JSON data file: {e}"))?;
    let rows = value
        .as_array()
        .ok_or("JSON data file must be an array of objects")?;
    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let obj = row
            .as_object()
            .ok_or_else(|| format!("row {i} is not an object"))?;
        let mut map = BTreeMap::new();
        for (k, v) in obj {
            map.insert(k.clone(), json_cell(v).map_err(|e| format!("row {i}: {e}"))?);
        }
        out.push(map);
    }
    Ok(out)
}

fn json_cell(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Ok(String::new()),
        _ => Err("cells must be strings, numbers, booleans, or null".into()),
    }
}

fn parse_csv(text: &str) -> Result<Vec<BTreeMap<String, String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_quotes {
            if c == '"' {
                if bytes.get(i + 1) == Some(&b'"') {
                    field.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            in_quotes = true;
        } else if c == ',' {
            record.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            record.push(std::mem::take(&mut field));
            if record.iter().any(|s| !s.is_empty()) {
                records.push(std::mem::take(&mut record));
            } else {
                record.clear();
            }
        } else {
            field.push(c);
        }
        i += 1;
    }
    if in_quotes {
        return Err("data file CSV has an unterminated quote".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        if record.iter().any(|s| !s.is_empty()) {
            records.push(record);
        }
    }
    let (header, rows) = records.split_first().ok_or("data file CSV is empty")?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut map = BTreeMap::new();
        for (i, name) in header.iter().enumerate() {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            map.insert(name.to_string(), row.get(i).cloned().unwrap_or_default());
        }
        out.push(map);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_header_and_quotes() {
        let rows = parse_csv("id,name\n1,\"a,b\"\n2,c\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("name").map(String::as_str), Some("a,b"));
        assert_eq!(rows[1].get("id").map(String::as_str), Some("2"));
    }

    #[test]
    fn json_array() {
        let rows = parse_json(r#"[{"id":1,"ok":true}]"#).unwrap();
        assert_eq!(rows[0].get("id").map(String::as_str), Some("1"));
        assert_eq!(rows[0].get("ok").map(String::as_str), Some("true"));
    }

    #[test]
    fn secrets_are_dropped() {
        let mut row = BTreeMap::new();
        row.insert("id".into(), "1".into());
        row.insert("apiToken".into(), "nope".into());
        let secrets = BTreeSet::from(["apiToken".to_string()]);
        let clean = without_secrets(&row, &secrets);
        assert!(clean.contains_key("id"));
        assert!(!clean.contains_key("apiToken"));
    }
}
