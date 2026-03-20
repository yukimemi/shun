use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct History {
    pub entries: HashMap<String, HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub count: u32,
    pub last_used: u64,
    #[serde(default)]
    pub last_args: Option<String>,
}

pub fn history_path() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("shun").join("history.json")
}

pub fn load() -> History {
    let path = history_path();
    if !path.exists() {
        return History::default();
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn save(history: &History) {
    let path = history_path();
    if let Ok(json) = serde_json::to_string_pretty(history) {
        let _ = std::fs::write(path, json);
    }
}

pub fn record(key: &str) {
    let mut history = load();
    let now = now_secs();
    let entry = history.entries.entry(key.to_string()).or_insert(HistoryEntry {
        count: 0,
        last_used: 0,
        last_args: None,
    });
    entry.count += 1;
    entry.last_used = now;
    save(&history);
}

/// extra_args ありで起動したとき: `path\targs` を別エントリとして記録し、
/// base path の last_args も更新する。
pub fn record_args(path: &str, args: &[String]) {
    if args.is_empty() { return; }
    let args_str = args.join(" ");
    let combined_key = format!("{}\t{}", path, args_str);
    let now = now_secs();
    let mut history = load();

    // combined entry
    let combined = history.entries.entry(combined_key).or_insert(HistoryEntry {
        count: 0, last_used: 0, last_args: None,
    });
    combined.count += 1;
    combined.last_used = now;

    // base path の last_args を更新
    let base = history.entries.entry(path.to_string()).or_insert(HistoryEntry {
        count: 0, last_used: 0, last_args: None,
    });
    base.last_args = Some(args_str);

    save(&history);
}

pub fn get_last_args(path: &str) -> Option<String> {
    load().entries.get(path).and_then(|e| e.last_args.clone())
}

pub fn delete(key: &str) -> Result<(), std::io::Error> {
    let mut history = load();
    history.entries.remove(key);
    let path = history_path();
    let json = serde_json::to_string_pretty(&history)
        .map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

pub fn sort_key(history: &History, item_path: &str) -> (u32, u64) {
    history
        .entries
        .get(item_path)
        .map(|e| (e.count, e.last_used))
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- sort_key ---

    #[test]
    fn sort_key_missing_returns_zero() {
        let hist = History::default();
        assert_eq!(sort_key(&hist, "anything"), (0, 0));
    }

    #[test]
    fn sort_key_returns_count_and_timestamp() {
        let mut hist = History::default();
        hist.entries.insert("myapp".to_string(), HistoryEntry {
            count: 5, last_used: 1000, last_args: None,
        });
        assert_eq!(sort_key(&hist, "myapp"), (5, 1000));
    }

    // --- serde round-trip ---

    #[test]
    fn history_serde_roundtrip() {
        let mut hist = History::default();
        hist.entries.insert("app".to_string(), HistoryEntry {
            count: 3, last_used: 999, last_args: Some("--flag".to_string()),
        });
        let json = serde_json::to_string(&hist).unwrap();
        let restored: History = serde_json::from_str(&json).unwrap();
        let entry = &restored.entries["app"];
        assert_eq!(entry.count, 3);
        assert_eq!(entry.last_used, 999);
        assert_eq!(entry.last_args.as_deref(), Some("--flag"));
    }

    #[test]
    fn history_last_args_defaults_to_none() {
        let json = r#"{"entries":{"app":{"count":1,"last_used":100}}}"#;
        let hist: History = serde_json::from_str(json).unwrap();
        assert!(hist.entries["app"].last_args.is_none());
    }

    // --- combined key format (used by history_items parser) ---

    #[test]
    fn combined_key_tab_separator() {
        let path = "C:/tools/app.exe";
        let args = vec!["--verbose".to_string(), "file.txt".to_string()];
        let key = format!("{}\t{}", path, args.join(" "));
        let tab_idx = key.find('\t').unwrap();
        assert_eq!(&key[..tab_idx], path);
        assert_eq!(&key[tab_idx + 1..], "--verbose file.txt");
    }
}
