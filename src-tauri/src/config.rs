use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// [theme] セクション。preset 名 + 個別カラー上書き。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeConfig {
    /// プリセット名: "catppuccin-mocha" | "catppuccin-latte" | "nord" | "dracula" | "tokyo-night" | "one-half-dark" | "solarized-dark" | "solarized-light"
    #[serde(default)]
    pub preset: String,
    // 個別カラー上書き (省略可)
    pub bg: Option<String>,
    pub surface: Option<String>,
    pub overlay: Option<String>,
    pub muted: Option<String>,
    pub text: Option<String>,
    pub blue: Option<String>,
    pub purple: Option<String>,
    pub green: Option<String>,
    pub red: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    #[default]
    Fuzzy, // ファジー検索 (デフォルト)
    Exact,       // 部分一致
    Migemo,      // migemo 検索（日本語ローマ字入力で日本語ファイル名を検索）
    FuzzyMigemo, // fuzzy と migemo の OR（union）
    ExactMigemo, // exact と migemo の OR（union）
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    CountFirst, // 回数 -> 直近 -> アルファベット (デフォルト)
    RecentFirst, // 直近 -> 回数 -> アルファベット
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IconStyle {
    #[default]
    Unicode, // Unicode symbols (≈ = あ # ⌚)
    Svg, // Inline SVG icons
}

/// ランチャーを表示するモニターの指定
/// - "cursor"  : カーソルのあるモニター (デフォルト)
/// - "primary" : プライマリモニター
/// - "0", "1", … : インデックス指定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MonitorTarget {
    Named(String), // "cursor" | "primary"
    Index(usize),  // 0, 1, 2, …
}

impl Default for MonitorTarget {
    fn default() -> Self {
        MonitorTarget::Named("cursor".to_string())
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
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default = "default_history_max_items")]
    pub history_max_items: usize,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub icon_style: IconStyle,
    #[serde(default)]
    pub monitor: MonitorTarget,
    #[serde(default = "default_preview_width")]
    pub preview_width: u32,
    #[serde(default = "default_max_preview_lines")]
    pub max_preview_lines: usize,
    #[serde(default)]
    pub vars: HashMap<String, String>,
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
    #[serde(default = "default_delete_item")]
    pub delete_item: String,
    #[serde(default = "default_cycle_search_mode")]
    pub cycle_search_mode: String,
    #[serde(default = "default_cycle_sort_order")]
    pub cycle_sort_order: String,
}

pub fn default_launch() -> String {
    "Ctrl+Space".to_string()
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
fn default_delete_item() -> String {
    "Ctrl+d".to_string()
}
fn default_cycle_search_mode() -> String {
    "Ctrl+Shift+m".to_string()
}
fn default_cycle_sort_order() -> String {
    "Ctrl+Shift+o".to_string()
}
fn default_log_level() -> String {
    "warn".to_string()
}
fn default_log_max_file_size_kb() -> u64 {
    1024 // 1 MB
}
fn default_log_rotation() -> String {
    "keep_one".to_string()
}
fn default_update_check_interval() -> u64 {
    3600
}
fn default_window_width() -> u32 {
    620
}
fn default_preview_width() -> u32 {
    400
}
fn default_max_preview_lines() -> usize {
    30
}
fn default_max_items() -> usize {
    8
}
fn default_max_completions() -> usize {
    6
}
fn default_font_size() -> u32 {
    14
}
fn default_opacity() -> f64 {
    1.0
}
fn default_history_max_items() -> usize {
    1000
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
            delete_item: default_delete_item(),
            cycle_search_mode: default_cycle_search_mode(),
            cycle_sort_order: default_cycle_sort_order(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompletionType {
    #[default]
    Path, // ファイルシステム補完 (デフォルト)
    None,    // 補完なし
    List,    // completion_list から補完
    Command, // completion_command の出力から補完
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
    /// アプリ単位の補完検索モード上書き (省略時はグローバルの search_mode を使用)
    pub completion_search_mode: Option<SearchMode>,
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
    /// stem 名マッチ（大文字小文字無視）
    #[serde(default)]
    pub name: String,
    /// 拡張子マッチ（"xlsx", "pdf" など、ドットなし）
    pub ext: Option<String>,
    /// 実行ファイルの上書き（ext マッチ時に元ファイルを {{ file_path }} 等で参照できる）
    pub path: Option<String>,
    pub completion: Option<CompletionType>,
    #[serde(default)]
    pub completion_list: Vec<String>,
    pub completion_command: Option<String>,
    pub args: Option<Vec<String>>,
    pub workdir: Option<String>,
}

/// [log] セクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// ログレベル: "debug" | "info" | "warn" | "error" | "off"
    #[serde(default = "default_log_level")]
    pub level: String,
    /// ローテーション前の最大ファイルサイズ (KB, デフォルト: 1024 = 1MB)
    #[serde(default = "default_log_max_file_size_kb")]
    pub max_file_size_kb: u64,
    /// ローテーション戦略: "keep_one" (デフォルト) | "keep_all" | 数値 (世代数)
    #[serde(default = "default_log_rotation")]
    pub rotation: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            max_file_size_kb: default_log_max_file_size_kb(),
            rotation: default_log_rotation(),
        }
    }
}

impl LogConfig {
    pub fn to_level_filter(&self) -> log::LevelFilter {
        match self.level.to_lowercase().as_str() {
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            "off" => log::LevelFilter::Off,
            _ => log::LevelFilter::Warn,
        }
    }

    pub fn to_rotation_strategy(&self) -> tauri_plugin_log::RotationStrategy {
        match self.rotation.to_lowercase().as_str() {
            "keep_all" => tauri_plugin_log::RotationStrategy::KeepAll,
            s => match s.parse::<usize>() {
                Ok(n) => tauri_plugin_log::RotationStrategy::KeepSome(n),
                Err(_) => tauri_plugin_log::RotationStrategy::KeepOne,
            },
        }
    }
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
            font_size: default_font_size(),
            opacity: default_opacity(),
            history_max_items: default_history_max_items(),
            theme: ThemeConfig::default(),
            log: LogConfig::default(),
            icon_style: IconStyle::default(),
            monitor: MonitorTarget::default(),
            preview_width: default_preview_width(),
            max_preview_lines: default_max_preview_lines(),
            vars: HashMap::new(),
            apps: vec![],
            scan_dirs: vec![],
            overrides: vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// toml::Value レベルの vars 展開・マージ helpers
// ---------------------------------------------------------------------------

/// `{{ vars.* }}` と `{{ env.* }}` を解決する Tera コンテキストを作る。
/// `{{ args }}` は対象外（launch 時に別途処理される）。
fn build_vars_ctx(vars: &HashMap<String, String>) -> tera::Context {
    let mut ctx = tera::Context::new();
    ctx.insert("vars", vars);
    let env_map: HashMap<String, String> = std::env::vars().collect();
    ctx.insert("env", &env_map);
    ctx
}

/// toml::Value ツリーの全 String を Tera で展開する（再帰）。
fn expand_value(val: &mut toml::Value, ctx: &tera::Context) {
    match val {
        toml::Value::String(s) if s.contains("{{") => {
            if let Ok(rendered) = tera::Tera::one_off(s, ctx, false) {
                *s = rendered;
            }
        }
        toml::Value::Array(arr) => arr.iter_mut().for_each(|v| expand_value(v, ctx)),
        toml::Value::Table(t) => t.iter_mut().for_each(|(_, v)| expand_value(v, ctx)),
        _ => {}
    }
}

/// マージ済み toml::Value 全体を vars で展開する。[vars] セクション自体は展開しない。
fn expand_config_vars(root: &mut toml::Value, vars: &HashMap<String, String>) {
    if vars.is_empty() {
        return;
    }
    let ctx = build_vars_ctx(vars);
    if let toml::Value::Table(table) = root {
        for (key, v) in table.iter_mut() {
            if key != "vars" {
                expand_value(v, &ctx);
            }
        }
    }
}

/// 配列を追記するキー（mergeではなくappend）
const APPEND_KEYS: &[&str] = &["apps", "scan_dirs", "overrides"];
/// サブテーブルをキー単位でマージするキー
const TABLE_MERGE_KEYS: &[&str] = &["vars", "keybindings", "theme", "log"];

/// extra を base にマージする（toml::Value レベル）。
/// - APPEND_KEYS: 配列を末尾に追記
/// - TABLE_MERGE_KEYS: サブテーブルをキー単位でマージ（extra が優先）
/// - その他: extra で上書き
fn merge_toml(base: &mut toml::Value, extra: toml::Value) {
    let (toml::Value::Table(base_t), toml::Value::Table(extra_t)) = (base, extra) else {
        return;
    };
    for (key, extra_val) in extra_t {
        if APPEND_KEYS.contains(&key.as_str()) {
            if let Some(toml::Value::Array(base_arr)) = base_t.get_mut(&key) {
                if let toml::Value::Array(extra_arr) = extra_val {
                    base_arr.extend(extra_arr);
                    continue;
                }
            }
        } else if TABLE_MERGE_KEYS.contains(&key.as_str()) {
            if let Some(toml::Value::Table(base_sub)) = base_t.get_mut(&key) {
                if let toml::Value::Table(extra_sub) = extra_val {
                    base_sub.extend(extra_sub);
                    continue;
                }
            }
        }
        base_t.insert(key, extra_val);
    }
}

/// ファイルを toml::Value として読み込む。失敗時は空テーブルを返し warnings に追記。
fn read_toml_value(
    path: &PathBuf,
    name: &str,
    warnings: &mut Vec<(String, String)>,
) -> toml::Value {
    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<toml::Value>(&content) {
            Ok(v) => v,
            Err(e) => {
                warnings.push((name.to_string(), e.to_string()));
                toml::Value::Table(Default::default())
            }
        },
        Err(e) => {
            warnings.push((name.to_string(), format!("Failed to read: {e}")));
            toml::Value::Table(Default::default())
        }
    }
}

/// toml::Value から vars セクションを抽出する。
fn extract_vars(root: &toml::Value) -> HashMap<String, String> {
    root.get("vars")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

pub fn config_path() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("shun").join("config.toml")
}

pub fn local_config_path() -> PathBuf {
    config_dir().join("config.local.toml")
}

pub fn config_dir() -> PathBuf {
    config_path()
        .parent()
        .unwrap_or(&PathBuf::from("."))
        .to_path_buf()
}

/// config.*.toml ファイルをアルファベット順に返す（config.toml 自身は除く）
pub fn extra_config_files() -> Vec<PathBuf> {
    let dir = config_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().to_string();
            if name.starts_with("config.") && name.ends_with(".toml") && name != "config.toml" {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

pub fn load_config() -> (Config, Vec<(String, String)>) {
    let mut warnings: Vec<(String, String)> = Vec::new();
    let path = config_path();

    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, default_config_toml());
        return (Config::default(), warnings);
    }

    // ① 全 config ファイルを toml::Value としてロード・マージ
    let mut merged = read_toml_value(&path, "config.toml", &mut warnings);

    let local_path = local_config_path();
    for extra_path in extra_config_files() {
        if extra_path == local_path {
            continue;
        }
        let fname = extra_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let extra = read_toml_value(&extra_path, &fname, &mut warnings);
        merge_toml(&mut merged, extra);
    }
    if local_path.exists() {
        let fname = local_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "config.local.toml".to_string());
        let extra = read_toml_value(&local_path, &fname, &mut warnings);
        merge_toml(&mut merged, extra);
    }

    // ② [vars] を抽出し、全 String フィールドを Tera 展開（[vars] 自体は除く）
    //    toml::Value レベルで展開することで enum フィールドも含めて全フィールドに対応できる
    let vars = extract_vars(&merged);
    expand_config_vars(&mut merged, &vars);

    // ③ 展開済み toml::Value を Config にパース
    let config = toml::to_string(&merged)
        .ok()
        .and_then(|s| toml::from_str::<Config>(&s).ok())
        .unwrap_or_else(|| {
            warnings.push((
                "config.toml".to_string(),
                "Failed to parse config after vars expansion".to_string(),
            ));
            Config::default()
        });

    (config, warnings)
}

/// config.local.toml の内容をベースの Config にマージする（テスト用 helper）。
/// 内部では merge_toml を使用しているため本番の load_config と同じロジックが走る。
#[cfg(test)]
fn merge_local_config(base: &mut Config, local_content: &str) -> Result<(), String> {
    let extra: toml::Value = toml::from_str(local_content).map_err(|e| e.to_string())?;
    let base_str = toml::to_string(base).map_err(|e| e.to_string())?;
    let mut base_val: toml::Value = toml::from_str(&base_str).map_err(|e| e.to_string())?;
    merge_toml(&mut base_val, extra);
    let merged_str = toml::to_string(&base_val).map_err(|e| e.to_string())?;
    *base = toml::from_str(&merged_str).map_err(|e| e.to_string())?;
    Ok(())
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
        assert!(c.vars.is_empty());
        assert!(c.apps.is_empty());
        assert!(c.scan_dirs.is_empty());
        assert!(c.overrides.is_empty());
    }

    #[test]
    fn parse_vars_section() {
        let toml = r#"
[vars]
src_dir  = "~/src"
work_dir = "C:/work"
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.vars.get("src_dir").map(|s| s.as_str()), Some("~/src"));
        assert_eq!(c.vars.get("work_dir").map(|s| s.as_str()), Some("C:/work"));
    }

    #[test]
    fn parse_theme_preset() {
        let toml = r#"
[theme]
preset = "nord"
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.theme.preset, "nord");
        assert!(c.theme.bg.is_none());
    }

    #[test]
    fn parse_theme_with_overrides() {
        let toml = r##"
[theme]
preset = "dracula"
bg     = "#282a36"
text   = "#f8f8f2"
"##;
        let c: Config = toml::from_str(toml).unwrap();
        assert_eq!(c.theme.preset, "dracula");
        assert_eq!(c.theme.bg.as_deref(), Some("#282a36"));
        assert_eq!(c.theme.text.as_deref(), Some("#f8f8f2"));
        assert!(c.theme.blue.is_none());
    }

    #[test]
    fn theme_default_is_empty_preset() {
        let c = Config::default();
        assert_eq!(c.theme.preset, "");
        assert!(c.theme.bg.is_none());
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
        assert_eq!(kb.launch, "Ctrl+Space");
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
        assert_eq!(kb.delete_item, "Ctrl+d");
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
        merge_local_config(&mut base, local).unwrap();
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
        merge_local_config(&mut base, local).unwrap();
        assert_eq!(base.apps.len(), 1);
        assert_eq!(base.apps[0].name, "LocalApp");
    }

    #[test]
    fn merge_local_scalar_overrides_only_when_present() {
        let mut base = Config::default();
        base.search_mode = SearchMode::Exact;
        // hide_on_blur だけ上書き、search_mode はそのまま
        let local = "hide_on_blur = true";
        merge_local_config(&mut base, local).unwrap();
        assert_eq!(base.search_mode, SearchMode::Exact); // 変わらない
        assert!(base.hide_on_blur); // 上書きされた
    }

    #[test]
    fn merge_local_keybinding_partial_override() {
        let mut base = Config::default();
        base.keybindings.next = "Ctrl+j".to_string();
        let local = r#"
[keybindings]
prev = "Ctrl+k"
"#;
        merge_local_config(&mut base, local).unwrap();
        assert_eq!(base.keybindings.next, "Ctrl+j"); // 変わらない
        assert_eq!(base.keybindings.prev, "Ctrl+k"); // 上書きされた
        assert_eq!(base.keybindings.close, "Escape"); // デフォルトのまま
    }

    #[test]
    fn merge_local_invalid_toml_is_ignored() {
        let mut base = Config::default();
        base.search_mode = SearchMode::Exact;
        assert!(merge_local_config(&mut base, "NOT VALID !!!@#$").is_err());
        assert_eq!(base.search_mode, SearchMode::Exact); // 変わらない
    }

    // --- vars 展開 ---

    fn apply_vars(toml_str: &str) -> Config {
        let mut val: toml::Value = toml::from_str(toml_str).unwrap();
        let vars = extract_vars(&val);
        expand_config_vars(&mut val, &vars);
        toml::to_string(&val)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap()
    }

    #[test]
    fn vars_expanded_in_theme_preset() {
        let c = apply_vars(
            r#"
[vars]
color = "dark"

[theme]
preset = "solarized-{{ vars.color }}"
"#,
        );
        assert_eq!(c.theme.preset, "solarized-dark");
    }

    #[test]
    fn vars_expanded_in_app_name() {
        let c = apply_vars(
            r#"
[vars]
prefix = "My"

[[apps]]
name = "{{ vars.prefix }} Editor"
path = "nvim"
"#,
        );
        assert_eq!(c.apps[0].name, "My Editor");
    }

    #[test]
    fn vars_expanded_in_scan_dirs_path() {
        let c = apply_vars(
            r#"
[vars]
src = "~/src"

[[scan_dirs]]
path = "{{ vars.src }}/scripts"
recursive = false
"#,
        );
        assert_eq!(c.scan_dirs[0].path, "~/src/scripts");
    }

    #[test]
    fn vars_expanded_in_completion_search_mode_enum() {
        // enum フィールドも toml::Value レベルで展開されるので機能する
        let c = apply_vars(
            r#"
[vars]
mode = "migemo"

[[apps]]
name = "Test"
path = "test"
completion_search_mode = "{{ vars.mode }}"
"#,
        );
        assert_eq!(c.apps[0].completion_search_mode, Some(SearchMode::Migemo));
    }

    #[test]
    fn vars_section_not_self_expanded() {
        // [vars] 自体は展開されない（値の中に {{ }} があっても展開しない）
        let toml_str = r#"
[vars]
a = "hello"
b = "{{ vars.a }} world"
"#;
        let mut val: toml::Value = toml::from_str(toml_str).unwrap();
        let vars = extract_vars(&val);
        expand_config_vars(&mut val, &vars);
        // [vars].b は展開されていないことを確認
        let b = val
            .get("vars")
            .and_then(|v| v.get("b"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(b, "{{ vars.a }} world");
    }

    #[test]
    fn vars_expanded_in_workdir() {
        let c = apply_vars(
            r#"
[vars]
src = "~/src"

[[apps]]
name = "Git"
path = "git"
workdir = "{{ vars.src }}/myproject"
"#,
        );
        assert_eq!(c.apps[0].workdir.as_deref(), Some("~/src/myproject"));
    }
}

fn default_config_toml() -> String {
    r##"# Search mode: "fuzzy" (default) | "exact" | "migemo" (romaji → Japanese)
search_mode = "fuzzy"

# Sort order: "count_first" (default) | "recent_first"
sort_order = "count_first"

# Auto-hide when the launcher loses focus
hide_on_blur = false

# Update check interval in seconds (0 to disable)
update_check_interval = 3600

# Launcher window width in pixels
window_width = 620

# Max items shown in the results list
max_items = 8

# Max items shown in the completion dropdown
max_completions = 6

# Font size in pixels (default: 14)
# font_size = 14

# Window opacity 0.0–1.0 (default: 1.0)
# opacity = 1.0

# Maximum number of history entries to keep (default: 1000)
# history_max_items = 1000

# Status badge icon style: "unicode" (default) | "svg"
# icon_style = "unicode"

# Monitor to show the launcher on: "cursor" (default) | "primary" | 0 | 1 | ...
# monitor = "cursor"

[keybindings]
launch            = "Ctrl+Space"   # Global hotkey to show/hide
next              = "Ctrl+n"
prev              = "Ctrl+p"
confirm           = "Enter"
arg_mode          = "Tab"
accept_word       = "Ctrl+f"      # Accept next word of ghost text
accept_line       = "Ctrl+e"      # Accept full ghost text
delete_word       = "Ctrl+w"      # Delete word before cursor
delete_line       = "Ctrl+u"      # Delete to beginning of line
run_query         = "Shift+Enter" # Run typed query directly (skip history results)
close             = "Escape"
delete_item       = "Ctrl+d"      # Delete selected history item
cycle_search_mode = "Ctrl+Shift+m" # Cycle search mode (fuzzy → exact → migemo)
cycle_sort_order  = "Ctrl+Shift+o" # Cycle sort order (count_first ↔ recent_first)

# Theme — preset + optional per-color overrides
# preset: "catppuccin-mocha" (default) | "catppuccin-latte" | "nord" | "dracula" | "tokyo-night" | "one-half-dark" | "solarized-dark" | "solarized-light"
# [theme]
# preset  = "nord"
# bg      = "#1a1a2e"
# surface = "#16213e"
# overlay = "#0f3460"
# muted   = "#533483"
# text    = "#e0e0e0"
# blue    = "#88c0d0"
# purple  = "#b48ead"
# green   = "#a3be8c"
# red     = "#bf616a"

# User-defined variables — reference with {{ vars.my_var }} in path/args
# [vars]
# src_dir  = "~/src/github.com/yourname"
# work_dir = "C:/work"

# Register apps individually
# [[apps]]
# name                   = "My Editor"
# path                   = "nvim"
# args                   = ["--flag"]
# workdir                = "~/src"
# completion             = "path"     # "path" | "none" | "list" | "command"
# completion_list        = ["start", "stop", "restart"]
# completion_command     = "git branch --format='%(refname:short)'"
# completion_search_mode = "fuzzy"    # "fuzzy" | "exact" | "migemo"

# Auto-register scripts from directories (non-existent paths are silently ignored)
# Windows
# [[scan_dirs]]
# path       = "~/bin"
# recursive  = false
# extensions = ["exe", "bat", "ps1", "cmd"]

# macOS / Linux
# [[scan_dirs]]
# path       = "~/.local/bin"
# recursive  = false
# extensions = ["sh", "py"]
"##
    .to_string()
}
