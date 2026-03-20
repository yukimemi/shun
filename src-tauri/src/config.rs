use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Fuzzy,  // ファジー検索 (デフォルト)
    Exact,  // 部分一致
}

impl Default for SearchMode {
    fn default() -> Self {
        SearchMode::Fuzzy
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    CountFirst,   // 回数 -> 直近 -> アルファベット (デフォルト)
    RecentFirst,  // 直近 -> 回数 -> アルファベット
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::CountFirst
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub keybindings: Keybindings,
    #[serde(default)]
    pub search_mode: SearchMode,
    #[serde(default)]
    pub sort_order: SortOrder,
    #[serde(default)]
    pub hide_on_blur: bool,
    #[serde(default = "default_update_check_interval")]
    pub update_check_interval: u64,
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    #[serde(default = "default_max_completions")]
    pub max_completions: usize,
    #[serde(default)]
    pub apps: Vec<AppEntry>,
    #[serde(default)]
    pub scan_dirs: Vec<ScanDir>,
    #[serde(default)]
    pub overrides: Vec<AppOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybindings {
    #[serde(default = "default_launch")]
    pub launch: String,
    #[serde(default = "default_next")]
    pub next: String,
    #[serde(default = "default_prev")]
    pub prev: String,
    #[serde(default = "default_confirm")]
    pub confirm: String,
    #[serde(default = "default_arg_mode")]
    pub arg_mode: String,
    #[serde(default = "default_accept_word")]
    pub accept_word: String,
    #[serde(default = "default_accept_line")]
    pub accept_line: String,
    #[serde(default = "default_delete_word")]
    pub delete_word: String,
    #[serde(default = "default_delete_line")]
    pub delete_line: String,
    #[serde(default = "default_run_query")]
    pub run_query: String,
    #[serde(default = "default_close")]
    pub close: String,
}

fn default_launch() -> String {
    "Alt+Space".to_string()
}
fn default_next() -> String {
    "Ctrl+n".to_string()
}
fn default_prev() -> String {
    "Ctrl+p".to_string()
}
fn default_confirm() -> String {
    "Enter".to_string()
}
fn default_arg_mode() -> String {
    "Tab".to_string()
}
fn default_accept_word() -> String {
    "Ctrl+f".to_string()
}
fn default_accept_line() -> String {
    "Ctrl+e".to_string()
}
fn default_delete_word() -> String {
    "Ctrl+w".to_string()
}
fn default_delete_line() -> String {
    "Ctrl+u".to_string()
}
fn default_run_query() -> String {
    "Shift+Enter".to_string()
}
fn default_close() -> String {
    "Escape".to_string()
}
fn default_update_check_interval() -> u64 {
    3600
}
fn default_window_width() -> u32 {
    620
}
fn default_max_items() -> usize {
    8
}
fn default_max_completions() -> usize {
    6
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            launch: default_launch(),
            next: default_next(),
            prev: default_prev(),
            confirm: default_confirm(),
            arg_mode: default_arg_mode(),
            accept_word: default_accept_word(),
            accept_line: default_accept_line(),
            delete_word: default_delete_word(),
            delete_line: default_delete_line(),
            run_query: default_run_query(),
            close: default_close(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionType {
    Path,     // ファイルシステム補完 (デフォルト)
    None,     // 補完なし
    List,     // completion_list から補完
    Command,  // completion_command の出力から補完
}

impl Default for CompletionType {
    fn default() -> Self {
        CompletionType::Path
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub workdir: Option<String>,
    #[serde(default)]
    pub completion: CompletionType,
    #[serde(default)]
    pub completion_list: Vec<String>,
    pub completion_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanDir {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    pub extensions: Option<Vec<String>>,
}

/// スキャンで登録されたアイテムへの上書き設定。name で大文字小文字を無視してマッチする。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppOverride {
    pub name: String,
    pub completion: Option<CompletionType>,
    #[serde(default)]
    pub completion_list: Vec<String>,
    pub completion_command: Option<String>,
    pub args: Option<Vec<String>>,
    pub workdir: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keybindings: Keybindings::default(),
            search_mode: SearchMode::default(),
            sort_order: SortOrder::default(),
            hide_on_blur: false,
            update_check_interval: default_update_check_interval(),
            window_width: default_window_width(),
            max_items: default_max_items(),
            max_completions: default_max_completions(),
            apps: vec![],
            scan_dirs: vec![],
            overrides: vec![],
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("shun").join("config.toml")
}

pub fn local_config_path() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("shun").join("config.local.toml")
}

pub fn load_config() -> Config {
    let path = config_path();

    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let default_toml = default_config_toml();
        let _ = std::fs::write(&path, &default_toml);
        return Config::default();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };

    let mut config: Config = toml::from_str(&content).unwrap_or_default();

    // config.local.toml が存在すればマージする
    let local_path = local_config_path();
    if local_path.exists() {
        if let Ok(local_content) = std::fs::read_to_string(&local_path) {
            merge_local_config(&mut config, &local_content);
        }
    }

    config
}

/// config.local.toml の内容をベースの Config にマージする。
///
/// - Vec 系 (apps, scan_dirs, overrides): ローカルのエントリを追記
/// - スカラー系: ローカルに明示的に記述されている場合のみ上書き
/// - keybindings: フィールド単位でローカルが優先
fn merge_local_config(base: &mut Config, local_content: &str) {
    let local_val: toml::Value = match toml::from_str(local_content) {
        Ok(v) => v,
        Err(_) => return,
    };
    let local: Config = match toml::from_str(local_content) {
        Ok(c) => c,
        Err(_) => return,
    };
    let Some(table) = local_val.as_table() else {
        return;
    };

    // スカラー: ローカルに明示的に書かれている場合のみ上書き
    if table.contains_key("search_mode") {
        base.search_mode = local.search_mode;
    }
    if table.contains_key("sort_order") {
        base.sort_order = local.sort_order;
    }
    if table.contains_key("hide_on_blur") {
        base.hide_on_blur = local.hide_on_blur;
    }
    if table.contains_key("update_check_interval") {
        base.update_check_interval = local.update_check_interval;
    }

    // keybindings: フィールド単位でマージ
    if let Some(kb_val) = table.get("keybindings").and_then(|v| v.as_table()) {
        if kb_val.contains_key("launch") {
            base.keybindings.launch = local.keybindings.launch;
        }
        if kb_val.contains_key("next") {
            base.keybindings.next = local.keybindings.next;
        }
        if kb_val.contains_key("prev") {
            base.keybindings.prev = local.keybindings.prev;
        }
        if kb_val.contains_key("confirm") {
            base.keybindings.confirm = local.keybindings.confirm;
        }
        if kb_val.contains_key("arg_mode") {
            base.keybindings.arg_mode = local.keybindings.arg_mode;
        }
        if kb_val.contains_key("accept_word") {
            base.keybindings.accept_word = local.keybindings.accept_word;
        }
        if kb_val.contains_key("accept_line") {
            base.keybindings.accept_line = local.keybindings.accept_line;
        }
        if kb_val.contains_key("delete_word") {
            base.keybindings.delete_word = local.keybindings.delete_word;
        }
        if kb_val.contains_key("delete_line") {
            base.keybindings.delete_line = local.keybindings.delete_line;
        }
        if kb_val.contains_key("run_query") {
            base.keybindings.run_query = local.keybindings.run_query;
        }
        if kb_val.contains_key("close") {
            base.keybindings.close = local.keybindings.close;
        }
    }

    // Vec 系: ローカルのエントリを追記
    base.apps.extend(local.apps);
    base.scan_dirs.extend(local.scan_dirs);
    base.overrides.extend(local.overrides);
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- defaults ---

    #[test]
    fn config_default_values() {
        let c = Config::default();
        assert_eq!(c.search_mode, SearchMode::Fuzzy);
        assert_eq!(c.sort_order, SortOrder::CountFirst);
        assert!(!c.hide_on_blur);
        assert_eq!(c.update_check_interval, 3600);
        assert_eq!(c.window_width, 620);
        assert_eq!(c.max_items, 8);
        assert_eq!(c.max_completions, 6);
        assert!(c.apps.is_empty());
        assert!(c.scan_dirs.is_empty());
        assert!(c.overrides.is_empty());
    }

    #[test]
    fn parse_window_and_list_settings() {
        let toml = "window_width = 900\nmax_items = 12\nmax_completions = 10";
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.window_width, 900);
        assert_eq!(c.max_items, 12);
        assert_eq!(c.max_completions, 10);
    }

    #[test]
    fn keybindings_default_values() {
        let kb = Keybindings::default();
        assert_eq!(kb.launch, "Alt+Space");
        assert_eq!(kb.next, "Ctrl+n");
        assert_eq!(kb.prev, "Ctrl+p");
        assert_eq!(kb.confirm, "Enter");
        assert_eq!(kb.arg_mode, "Tab");
        assert_eq!(kb.accept_word, "Ctrl+f");
        assert_eq!(kb.accept_line, "Ctrl+e");
        assert_eq!(kb.delete_word, "Ctrl+w");
        assert_eq!(kb.delete_line, "Ctrl+u");
        assert_eq!(kb.run_query, "Shift+Enter");
        assert_eq!(kb.close, "Escape");
    }

    // --- TOML parsing ---

    #[test]
    fn parse_search_mode_and_sort_order() {
        let toml = r#"search_mode = "exact"
sort_order = "recent_first""#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.search_mode, SearchMode::Exact);
        assert_eq!(c.sort_order, SortOrder::RecentFirst);
    }

    #[test]
    fn parse_hide_on_blur() {
        let c: Config = toml::from_str("hide_on_blur = true").unwrap();
        assert!(c.hide_on_blur);
    }

    #[test]
    fn parse_partial_keybindings_keeps_defaults() {
        let toml = r#"
[keybindings]
next = "Ctrl+j"
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.keybindings.next, "Ctrl+j");
        assert_eq!(c.keybindings.prev, "Ctrl+p"); // default intact
        assert_eq!(c.keybindings.close, "Escape");
    }

    #[test]
    fn parse_apps_entry() {
        let toml = r#"
[[apps]]
name = "MyApp"
path = "C:/apps/myapp.exe"
completion = "list"
completion_list = ["start", "stop"]
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.apps.len(), 1);
        assert_eq!(c.apps[0].name, "MyApp");
        assert_eq!(c.apps[0].completion, CompletionType::List);
        assert_eq!(c.apps[0].completion_list, vec!["start", "stop"]);
    }

    #[test]
    fn parse_completion_type_variants() {
        let toml = r#"
[[apps]]
name = "A"
path = "/a"
completion = "none"

[[apps]]
name = "B"
path = "/b"
completion = "command"
completion_command = "echo foo"
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.apps[0].completion, CompletionType::None);
        assert_eq!(c.apps[1].completion, CompletionType::Command);
        assert_eq!(c.apps[1].completion_command.as_deref(), Some("echo foo"));
    }

    #[test]
    fn parse_overrides_entry() {
        let toml = r#"
[[overrides]]
name = "scoop"
completion = "list"
completion_list = ["install", "update"]
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.overrides.len(), 1);
        assert_eq!(c.overrides[0].name, "scoop");
        assert_eq!(c.overrides[0].completion_list, vec!["install", "update"]);
    }

    #[test]
    fn invalid_toml_falls_back_to_default() {
        let bad = "NOT VALID TOML !!!@#$";
        let c: Config = toml::from_str(bad).unwrap_or_default();
        assert_eq!(c.search_mode, SearchMode::Fuzzy);
    }

    // --- merge_local_config ---

    #[test]
    fn merge_local_appends_scan_dirs() {
        let mut base = Config::default();
        base.scan_dirs.push(ScanDir {
            path: "~/base".to_string(),
            recursive: false,
            extensions: None,
        });
        let local = r#"
[[scan_dirs]]
path = "~/local"
recursive = true
"#;
        merge_local_config(&mut base, local);
        assert_eq!(base.scan_dirs.len(), 2);
        assert_eq!(base.scan_dirs[1].path, "~/local");
        assert!(base.scan_dirs[1].recursive);
    }

    #[test]
    fn merge_local_appends_apps() {
        let mut base = Config::default();
        let local = r#"
[[apps]]
name = "LocalApp"
path = "/local/app"
"#;
        merge_local_config(&mut base, local);
        assert_eq!(base.apps.len(), 1);
        assert_eq!(base.apps[0].name, "LocalApp");
    }

    #[test]
    fn merge_local_scalar_overrides_only_when_present() {
        let mut base = Config::default();
        base.search_mode = SearchMode::Exact;
        // hide_on_blur だけ上書き、search_mode はそのまま
        let local = "hide_on_blur = true";
        merge_local_config(&mut base, local);
        assert_eq!(base.search_mode, SearchMode::Exact); // 変わらない
        assert!(base.hide_on_blur);                      // 上書きされた
    }

    #[test]
    fn merge_local_keybinding_partial_override() {
        let mut base = Config::default();
        base.keybindings.next = "Ctrl+j".to_string();
        let local = r#"
[keybindings]
prev = "Ctrl+k"
"#;
        merge_local_config(&mut base, local);
        assert_eq!(base.keybindings.next, "Ctrl+j"); // 変わらない
        assert_eq!(base.keybindings.prev, "Ctrl+k"); // 上書きされた
        assert_eq!(base.keybindings.close, "Escape"); // デフォルトのまま
    }

    #[test]
    fn merge_local_invalid_toml_is_ignored() {
        let mut base = Config::default();
        base.search_mode = SearchMode::Exact;
        merge_local_config(&mut base, "NOT VALID !!!@#$");
        assert_eq!(base.search_mode, SearchMode::Exact); // 変わらない
    }
}

fn default_config_toml() -> String {
    r#"# 検索モード: "fuzzy" (ファジー検索) / "exact" (部分一致)
search_mode = "fuzzy"

# 履歴のソート順: "count_first" (回数→直近→名前) / "recent_first" (直近→回数→名前)
sort_order = "count_first"

# フォーカスが外れたら自動で非表示にする (true / false)
hide_on_blur = false

# アップデートチェック間隔 (秒) / 0 で無効化
update_check_interval = 3600

# ランチャーウィンドウの幅 (px)
window_width = 620

# 候補リストに表示するアイテム数の上限
max_items = 8

# 補完ドロップダウンに表示するアイテム数の上限
max_completions = 6

[keybindings]
launch      = "Alt+Space"
next        = "Ctrl+n"
prev        = "Ctrl+p"
confirm     = "Enter"
arg_mode    = "Tab"
accept_word = "Ctrl+f"
accept_line = "Ctrl+e"
delete_word = "Ctrl+w"
delete_line = "Ctrl+u"
run_query   = "Shift+Enter"
close       = "Escape"

# アプリ・スクリプトの個別登録
# [[apps]]
# name             = "My Script"
# path             = "/path/to/script.sh"
# args             = ["--flag"]
# workdir          = "/path/to/dir"
# completion       = "path"     # "path" | "none" | "list" | "command"
# completion_list  = ["start", "stop", "restart"]   # completion = "list" の時
# completion_command = "git branch --format='%(refname:short)'"  # completion = "command" の時

# ディレクトリスキャンで自動登録
# [[scan_dirs]]
# path       = "~/.local/bin"
# recursive  = false
# extensions = ["sh", "py", "ps1"]
"#
    .to_string()
}
