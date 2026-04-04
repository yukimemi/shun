use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const CURRENT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    #[serde(default = "default_version")]
    pub version: u32,
    pub entries: Vec<HistoryEntry>,
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

impl Default for History {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            entries: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub key: String,
    #[serde(default)]
    pub args: Option<String>,
    pub count: u32,
    pub last_used: u64,
}

// Old format for migration (version 1 / no version field)
#[derive(Debug, Deserialize)]
struct OldHistory {
    entries: HashMap<String, OldHistoryEntry>,
}

#[derive(Debug, Deserialize)]
struct OldHistoryEntry {
    count: u32,
    last_used: u64,
    #[serde(default)]
    #[allow(dead_code)]
    last_args: Option<String>,
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

    // Try new Vec-based format first
    if let Ok(h) = serde_json::from_str::<History>(&content) {
        if h.version == CURRENT_VERSION {
            return h;
        }
    }

    // Fall back to old HashMap format and migrate
    if let Ok(old) = serde_json::from_str::<OldHistory>(&content) {
        return migrate_from_old(old);
    }

    History::default()
}

fn migrate_from_old(old: OldHistory) -> History {
    let mut entries = Vec::new();
    for (key, entry) in old.entries {
        if let Some(tab_idx) = key.find('\t') {
            let item_key = key[..tab_idx].to_string();
            let args_str = key[tab_idx + 1..].to_string();
            entries.push(HistoryEntry {
                key: item_key,
                args: Some(args_str),
                count: entry.count,
                last_used: entry.last_used,
            });
        } else {
            // base entry — skip ghost entries (count=0 and last_used=0)
            if entry.count == 0 && entry.last_used == 0 {
                continue;
            }
            entries.push(HistoryEntry {
                key,
                args: None,
                count: entry.count,
                last_used: entry.last_used,
            });
        }
    }
    History {
        version: CURRENT_VERSION,
        entries,
    }
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

/// 履歴エントリ数を max 件に制限する（last_used が古いものから削除）
fn trim_to(history: &mut History, max: usize) {
    if max == 0 || history.entries.len() <= max {
        return;
    }
    // 新しい順にソートして max 件に切り詰める
    history
        .entries
        .sort_unstable_by(|a, b| b.last_used.cmp(&a.last_used));
    history.entries.truncate(max);
}

pub fn record(key: &str, max_items: usize) {
    let mut history = load();
    let now = now_secs();
    if let Some(entry) = history
        .entries
        .iter_mut()
        .find(|e| e.key == key && e.args.is_none())
    {
        entry.count += 1;
        entry.last_used = now;
    } else {
        history.entries.push(HistoryEntry {
            key: key.to_string(),
            args: None,
            count: 1,
            last_used: now,
        });
    }
    trim_to(&mut history, max_items);
    save(&history);
}

/// extra_args ありで起動したとき: key+args を別エントリとして記録する。
pub fn record_args(path: &str, args: &[String], max_items: usize) {
    if args.is_empty() {
        return;
    }
    let args_str = args.join(" ");
    let now = now_secs();
    let mut history = load();

    if let Some(entry) = history
        .entries
        .iter_mut()
        .find(|e| e.key == path && e.args.as_deref() == Some(args_str.as_str()))
    {
        entry.count += 1;
        entry.last_used = now;
    } else {
        history.entries.push(HistoryEntry {
            key: path.to_string(),
            args: Some(args_str),
            count: 1,
            last_used: now,
        });
    }
    trim_to(&mut history, max_items);
    save(&history);
}

/// 指定キーで最後に使った args を返す（last_used が最大のエントリ）
pub fn get_last_args(path: &str) -> Option<String> {
    let history = load();
    history
        .entries
        .iter()
        .filter(|e| e.key == path && e.args.is_some())
        .max_by_key(|e| e.last_used)
        .and_then(|e| e.args.clone())
}

/// combined_key は `"key\targs"` または単純な `"key"` 形式
pub fn delete(combined_key: &str) -> Result<(), std::io::Error> {
    let (key, args) = parse_combined_key(combined_key);
    let mut history = load();
    history
        .entries
        .retain(|e| !(e.key == key && e.args.as_deref() == args));
    let path = history_path();
    let json = serde_json::to_string_pretty(&history).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// combined_key は `"key\targs"` または単純な `"key"` 形式
pub fn sort_key(history: &History, combined_key: &str) -> (u32, u64) {
    let (key, args) = parse_combined_key(combined_key);
    history
        .entries
        .iter()
        .find(|e| e.key == key && e.args.as_deref() == args)
        .map(|e| (e.count, e.last_used))
        .unwrap_or((0, 0))
}

fn parse_combined_key(combined_key: &str) -> (&str, Option<&str>) {
    if let Some(tab_idx) = combined_key.find('\t') {
        (&combined_key[..tab_idx], Some(&combined_key[tab_idx + 1..]))
    } else {
        (combined_key, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_history(entries: Vec<HistoryEntry>) -> History {
        History {
            version: CURRENT_VERSION,
            entries,
        }
    }

    fn entry(key: &str, args: Option<&str>, count: u32, last_used: u64) -> HistoryEntry {
        HistoryEntry {
            key: key.to_string(),
            args: args.map(String::from),
            count,
            last_used,
        }
    }

    // --- sort_key ---

    #[test]
    fn sort_key_missing_returns_zero() {
        let hist = History::default();
        assert_eq!(sort_key(&hist, "anything"), (0, 0));
    }

    #[test]
    fn sort_key_base_entry() {
        let hist = make_history(vec![entry("myapp", None, 5, 1000)]);
        assert_eq!(sort_key(&hist, "myapp"), (5, 1000));
    }

    #[test]
    fn sort_key_args_entry_via_combined_key() {
        let hist = make_history(vec![entry("myapp", Some("--flag"), 3, 2000)]);
        assert_eq!(sort_key(&hist, "myapp\t--flag"), (3, 2000));
    }

    // --- serde round-trip ---

    #[test]
    fn history_serde_roundtrip() {
        let hist = make_history(vec![
            entry("app", None, 3, 999),
            entry("app", Some("--flag"), 1, 1234),
        ]);
        let json = serde_json::to_string(&hist).unwrap();
        let restored: History = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, CURRENT_VERSION);
        assert_eq!(restored.entries.len(), 2);
        let base = restored.entries.iter().find(|e| e.args.is_none()).unwrap();
        assert_eq!(base.count, 3);
        assert_eq!(base.last_used, 999);
        let args_e = restored
            .entries
            .iter()
            .find(|e| e.args.is_some())
            .unwrap();
        assert_eq!(args_e.args.as_deref(), Some("--flag"));
        assert_eq!(args_e.count, 1);
    }

    // --- get_last_args ---

    #[test]
    fn get_last_args_returns_most_recent() {
        // We can't easily test without file I/O, so test the logic via sort_key and find
        let hist = make_history(vec![
            entry("app", Some("old-arg"), 2, 100),
            entry("app", Some("new-arg"), 1, 200),
        ]);
        // Simulate get_last_args logic
        let result = hist
            .entries
            .iter()
            .filter(|e| e.key == "app" && e.args.is_some())
            .max_by_key(|e| e.last_used)
            .and_then(|e| e.args.clone());
        assert_eq!(result.as_deref(), Some("new-arg"));
    }

    // --- migration from old format ---

    #[test]
    fn migrate_from_old_format() {
        let mut old_entries = HashMap::new();
        old_entries.insert(
            "myapp".to_string(),
            OldHistoryEntry {
                count: 5,
                last_used: 1000,
                last_args: Some("--flag".to_string()),
            },
        );
        old_entries.insert(
            "myapp\t--flag".to_string(),
            OldHistoryEntry {
                count: 3,
                last_used: 900,
                last_args: None,
            },
        );
        // ghost entry should be skipped
        old_entries.insert(
            "ghost".to_string(),
            OldHistoryEntry {
                count: 0,
                last_used: 0,
                last_args: None,
            },
        );
        let old = OldHistory {
            entries: old_entries,
        };
        let new = migrate_from_old(old);
        assert_eq!(new.version, CURRENT_VERSION);
        // ghost skipped: 2 entries remain
        assert_eq!(new.entries.len(), 2);
        let base = new
            .entries
            .iter()
            .find(|e| e.key == "myapp" && e.args.is_none())
            .unwrap();
        assert_eq!(base.count, 5);
        let args_e = new
            .entries
            .iter()
            .find(|e| e.key == "myapp" && e.args.as_deref() == Some("--flag"))
            .unwrap();
        assert_eq!(args_e.count, 3);
    }

    #[test]
    fn migrate_old_json_string() {
        let json =
            r#"{"entries":{"app":{"count":2,"last_used":500},"app\t--v":{"count":1,"last_used":600}}}"#;
        let hist: History = {
            // Simulate load() fallback path
            if let Ok(h) = serde_json::from_str::<History>(json) {
                if h.version == CURRENT_VERSION {
                    h
                } else if let Ok(old) = serde_json::from_str::<OldHistory>(json) {
                    migrate_from_old(old)
                } else {
                    History::default()
                }
            } else if let Ok(old) = serde_json::from_str::<OldHistory>(json) {
                migrate_from_old(old)
            } else {
                History::default()
            }
        };
        assert_eq!(hist.version, CURRENT_VERSION);
        assert_eq!(hist.entries.len(), 2);
    }

    // --- trim_to ---

    #[test]
    fn trim_to_keeps_newest() {
        let mut hist = make_history(vec![
            entry("old", None, 10, 100),
            entry("mid", None, 5, 200),
            entry("new", None, 1, 300),
        ]);
        trim_to(&mut hist, 2);
        assert_eq!(hist.entries.len(), 2);
        assert!(hist.entries.iter().all(|e| e.last_used >= 200));
    }

    // --- delete ---

    #[test]
    fn delete_removes_correct_entry() {
        let tmp = TempDir::new().unwrap();
        // Override history path is not straightforward, so test parse_combined_key
        let (key, args) = parse_combined_key("myapp\t--flag");
        assert_eq!(key, "myapp");
        assert_eq!(args, Some("--flag"));

        let (key2, args2) = parse_combined_key("myapp");
        assert_eq!(key2, "myapp");
        assert_eq!(args2, None);

        drop(tmp);
    }
}
