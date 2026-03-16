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
}

fn history_path() -> PathBuf {
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

pub fn record(key: &str) {
    let mut history = load();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let entry = history.entries.entry(key.to_string()).or_insert(HistoryEntry {
        count: 0,
        last_used: 0,
    });
    entry.count += 1;
    entry.last_used = now;

    let path = history_path();
    if let Ok(json) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(path, json);
    }
}

pub fn sort_key(history: &History, item_path: &str) -> (u32, u64) {
    history
        .entries
        .get(item_path)
        .map(|e| (e.count, e.last_used))
        .unwrap_or((0, 0))
}
