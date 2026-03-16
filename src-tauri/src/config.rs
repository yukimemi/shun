use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    pub sort_order: SortOrder,
    #[serde(default)]
    pub apps: Vec<AppEntry>,
    #[serde(default)]
    pub scan_dirs: Vec<ScanDir>,
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
    #[serde(default = "default_word_accept")]
    pub word_accept: String,
    #[serde(default = "default_line_accept")]
    pub line_accept: String,
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
fn default_word_accept() -> String {
    "Ctrl+f".to_string()
}
fn default_line_accept() -> String {
    "Ctrl+e".to_string()
}
fn default_close() -> String {
    "Escape".to_string()
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            launch: default_launch(),
            next: default_next(),
            prev: default_prev(),
            confirm: default_confirm(),
            arg_mode: default_arg_mode(),
            word_accept: default_word_accept(),
            line_accept: default_line_accept(),
            close: default_close(),
        }
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
    pub allow_extra_args: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanDir {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    pub extensions: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keybindings: Keybindings::default(),
            sort_order: SortOrder::default(),
            apps: vec![],
            scan_dirs: vec![],
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("shun").join("config.toml")
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

    toml::from_str(&content).unwrap_or_default()
}

fn default_config_toml() -> String {
    r#"# 履歴のソート順: "count_first" (回数→直近→名前) / "recent_first" (直近→回数→名前)
sort_order = "count_first"

[keybindings]
launch      = "Alt+Space"
next        = "Ctrl+n"
prev        = "Ctrl+p"
confirm     = "Enter"
arg_mode    = "Tab"
word_accept = "Ctrl+f"
line_accept = "Ctrl+e"
close       = "Escape"

# アプリ・スクリプトの個別登録
# [[apps]]
# name             = "My Script"
# path             = "/path/to/script.sh"
# args             = ["--flag"]
# workdir          = "/path/to/dir"
# allow_extra_args = true

# ディレクトリスキャンで自動登録
# [[scan_dirs]]
# path       = "~/.local/bin"
# recursive  = false
# extensions = ["sh", "py", "ps1"]
"#
    .to_string()
}
