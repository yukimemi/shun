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
    /// None = base entry (no args). Some = args entry, stored losslessly as Vec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
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
        if h.version > CURRENT_VERSION {
            // Written by a newer version of shun — use as-is and warn
            log::warn!(
                "history.json version {} is newer than supported ({}); loading as-is",
                h.version,
                CURRENT_VERSION
            );
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
    // 2パスで処理する:
    // 1) 全エントリを変換 (last_args も保持)
    // 2) last_args から args エントリが存在しない場合に補完する

    struct TmpEntry {
        key: String,
        args: Option<Vec<String>>,
        count: u32,
        last_used: u64,
        last_args: Option<String>, // base エントリ用: 補完判定に使う
    }

    let mut tmp: Vec<TmpEntry> = Vec::new();
    for (key, entry) in old.entries {
        // skip ghost entries regardless of type
        if entry.count == 0 && entry.last_used == 0 {
            continue;
        }
        if let Some(tab_idx) = key.find('\t') {
            let item_key = key[..tab_idx].to_string();
            let args_str = &key[tab_idx + 1..];
            if args_str.is_empty() {
                // malformed key ("app\t") — treat as base entry
                tmp.push(TmpEntry {
                    key: item_key,
                    args: None,
                    count: entry.count,
                    last_used: entry.last_used,
                    last_args: None,
                });
            } else {
                // best-effort split: old format had no lossless encoding
                let args_vec: Vec<String> = args_str.split_whitespace().map(String::from).collect();
                tmp.push(TmpEntry {
                    key: item_key,
                    args: Some(args_vec),
                    count: entry.count,
                    last_used: entry.last_used,
                    last_args: None,
                });
            }
        } else {
            tmp.push(TmpEntry {
                key: key.clone(),
                args: None,
                count: entry.count,
                last_used: entry.last_used,
                last_args: entry.last_args,
            });
        }
    }

    // base エントリの last_args を参照し、対応する args エントリが存在しなければ補完する
    let mut synthetic: Vec<HistoryEntry> = Vec::new();
    for t in &tmp {
        if t.args.is_none() {
            if let Some(ref la) = t.last_args {
                if la.is_empty() {
                    continue;
                }
                let args_vec: Vec<String> = la.split_whitespace().map(String::from).collect();
                let already_exists = tmp
                    .iter()
                    .any(|e| e.key == t.key && e.args.as_deref() == Some(args_vec.as_slice()));
                if !already_exists {
                    synthetic.push(HistoryEntry {
                        key: t.key.clone(),
                        args: Some(args_vec),
                        count: 1,
                        last_used: t.last_used,
                    });
                }
            }
        }
    }

    let mut entries: Vec<HistoryEntry> = tmp
        .into_iter()
        .map(|t| HistoryEntry {
            key: t.key,
            args: t.args,
            count: t.count,
            last_used: t.last_used,
        })
        .collect();
    entries.extend(synthetic);

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
        .sort_unstable_by_key(|e| std::cmp::Reverse(e.last_used));
    history.entries.truncate(max);
}

/// `entry.args` と combined key から取り出した args 文字列を比較するヘルパー。
/// args は Vec で保持するが combined key では join(" ") した文字列として扱う。
fn args_matches(entry_args: &Option<Vec<String>>, args_str: Option<&str>) -> bool {
    match (entry_args, args_str) {
        (None, None) => true,
        (Some(v), Some(s)) => v.join(" ") == s,
        _ => false,
    }
}

/// combined_key は `"key\targs_joined"` または単純な `"key"` 形式。
/// History args アイテムの再実行でも combined key がそのまま渡されるため、両方を正しく処理する。
pub fn record(combined_key: &str, max_items: usize) {
    let (key, args_str) = parse_combined_key(combined_key);
    let mut history = load();
    // 将来バージョンのファイルを上書きすると未知フィールドが失われるため書き込みを拒否する
    if history.version > CURRENT_VERSION {
        return;
    }
    let now = now_secs();
    if let Some(entry) = history
        .entries
        .iter_mut()
        .find(|e| e.key == key && args_matches(&e.args, args_str))
    {
        entry.count += 1;
        entry.last_used = now;
    } else {
        history.entries.push(HistoryEntry {
            key: key.to_string(),
            // combined key からの復元は best-effort split
            args: args_str.map(|s| s.split_whitespace().map(String::from).collect()),
            count: 1,
            last_used: now,
        });
    }
    trim_to(&mut history, max_items);
    save(&history);
}

/// extra_args ありで起動したとき: key+args を別エントリとして lossless に記録する。
pub fn record_args(path: &str, args: &[String], max_items: usize) {
    if args.is_empty() {
        return;
    }
    let now = now_secs();
    let mut history = load();
    // 将来バージョンのファイルを上書きすると未知フィールドが失われるため書き込みを拒否する
    if history.version > CURRENT_VERSION {
        return;
    }

    if let Some(entry) = history
        .entries
        .iter_mut()
        .find(|e| e.key == path && e.args.as_deref() == Some(args))
    {
        entry.count += 1;
        entry.last_used = now;
    } else {
        history.entries.push(HistoryEntry {
            key: path.to_string(),
            args: Some(args.to_vec()),
            count: 1,
            last_used: now,
        });
    }
    trim_to(&mut history, max_items);
    save(&history);
}

/// 指定キーで最後に使った args を返す（last_used が最大のエントリ）。
/// 呼び出し元がゴーストテキストとして表示するため join(" ") した文字列で返す。
pub fn get_last_args(path: &str) -> Option<String> {
    let history = load();
    history
        .entries
        .iter()
        .filter(|e| e.key == path && e.args.is_some())
        .max_by_key(|e| e.last_used)
        .and_then(|e| e.args.as_ref().map(|v| v.join(" ")))
}

/// combined_key は `"key\targs_joined"` または単純な `"key"` 形式
pub fn delete(combined_key: &str) -> Result<(), std::io::Error> {
    let (key, args_str) = parse_combined_key(combined_key);
    let mut history = load();
    // 将来バージョンのファイルを上書きすると未知フィールドが失われるため書き込みを拒否する
    if history.version > CURRENT_VERSION {
        return Ok(());
    }
    history
        .entries
        .retain(|e| !(e.key == key && args_matches(&e.args, args_str)));
    let path = history_path();
    let json = serde_json::to_string_pretty(&history).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// combined_key は `"key\targs_joined"` または単純な `"key"` 形式
pub fn sort_key(history: &History, combined_key: &str) -> (u32, u64) {
    let (key, args_str) = parse_combined_key(combined_key);
    history
        .entries
        .iter()
        .find(|e| e.key == key && args_matches(&e.args, args_str))
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

    fn make_history(entries: Vec<HistoryEntry>) -> History {
        History {
            version: CURRENT_VERSION,
            entries,
        }
    }

    fn entry(key: &str, args: Option<&[&str]>, count: u32, last_used: u64) -> HistoryEntry {
        HistoryEntry {
            key: key.to_string(),
            args: args.map(|a| a.iter().map(|s| s.to_string()).collect()),
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
        let hist = make_history(vec![entry("myapp", Some(&["--flag"]), 3, 2000)]);
        assert_eq!(sort_key(&hist, "myapp\t--flag"), (3, 2000));
    }

    #[test]
    fn sort_key_args_with_spaces_in_value() {
        // args with an internal space stored losslessly as Vec
        let hist = make_history(vec![entry(
            "myapp",
            Some(&["--title", "hello world"]),
            2,
            500,
        )]);
        assert_eq!(sort_key(&hist, "myapp\t--title hello world"), (2, 500));
    }

    // --- record_args lossless storage ---

    #[test]
    fn record_args_stores_vec_losslessly() {
        let args = vec!["--title".to_string(), "hello world".to_string()];
        let hist = make_history(vec![HistoryEntry {
            key: "app".to_string(),
            args: Some(args.clone()),
            count: 1,
            last_used: 100,
        }]);
        // LaunchItem should reconstruct from Vec directly
        let reconstructed = hist.entries[0].args.as_deref().unwrap();
        assert_eq!(reconstructed, args.as_slice());
    }

    // --- serde round-trip ---

    #[test]
    fn history_serde_roundtrip() {
        let hist = make_history(vec![
            entry("app", None, 3, 999),
            entry("app", Some(&["--flag"]), 1, 1234),
        ]);
        let json = serde_json::to_string(&hist).unwrap();
        let restored: History = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, CURRENT_VERSION);
        assert_eq!(restored.entries.len(), 2);
        let base = restored.entries.iter().find(|e| e.args.is_none()).unwrap();
        assert_eq!(base.count, 3);
        assert_eq!(base.last_used, 999);
        let args_e = restored.entries.iter().find(|e| e.args.is_some()).unwrap();
        assert_eq!(
            args_e.args.as_deref(),
            Some(vec!["--flag".to_string()].as_slice())
        );
        assert_eq!(args_e.count, 1);
    }

    // --- get_last_args ---

    #[test]
    fn get_last_args_returns_most_recent() {
        let hist = make_history(vec![
            entry("app", Some(&["old-arg"]), 2, 100),
            entry("app", Some(&["new-arg"]), 1, 200),
        ]);
        let result = hist
            .entries
            .iter()
            .filter(|e| e.key == "app" && e.args.is_some())
            .max_by_key(|e| e.last_used)
            .and_then(|e| e.args.as_ref().map(|v| v.join(" ")));
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
            .find(|e| e.key == "myapp" && e.args.is_some())
            .unwrap();
        assert_eq!(args_e.args.as_deref(), Some(&["--flag".to_string()][..]));
        assert_eq!(args_e.count, 3);
    }

    #[test]
    fn migrate_last_args_only_creates_synthetic_entry() {
        // base エントリに last_args があるが explicit な "key\targs" エントリが存在しない場合、
        // マイグレーションで合成 args エントリが作られること
        let mut old_entries = HashMap::new();
        old_entries.insert(
            "myapp".to_string(),
            OldHistoryEntry {
                count: 3,
                last_used: 800,
                last_args: Some("--verbose".to_string()),
            },
        );
        // explicit args エントリはなし
        let old = OldHistory {
            entries: old_entries,
        };
        let new = migrate_from_old(old);
        // base + synthetic args = 2 entries
        assert_eq!(new.entries.len(), 2);
        let args_e = new
            .entries
            .iter()
            .find(|e| e.key == "myapp" && e.args.is_some())
            .expect("synthetic args entry should be created from last_args");
        assert_eq!(args_e.args.as_deref(), Some(&["--verbose".to_string()][..]));
    }

    #[test]
    fn migrate_last_args_no_duplicate_when_explicit_exists() {
        // explicit args エントリがすでにある場合、last_args から重複エントリを作らないこと
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
        let old = OldHistory {
            entries: old_entries,
        };
        let new = migrate_from_old(old);
        // base + 1 args (no duplicate)
        assert_eq!(new.entries.len(), 2);
        let args_count = new.entries.iter().filter(|e| e.args.is_some()).count();
        assert_eq!(args_count, 1);
    }

    #[test]
    fn migrate_old_json_string() {
        let json = r#"{"entries":{"app":{"count":2,"last_used":500},"app\t--v":{"count":1,"last_used":600}}}"#;
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

    // --- parse_combined_key ---

    #[test]
    fn parse_combined_key_extracts_key_and_args() {
        let (key, args) = parse_combined_key("myapp\t--flag");
        assert_eq!(key, "myapp");
        assert_eq!(args, Some("--flag"));

        let (key2, args2) = parse_combined_key("myapp");
        assert_eq!(key2, "myapp");
        assert_eq!(args2, None);
    }
}
