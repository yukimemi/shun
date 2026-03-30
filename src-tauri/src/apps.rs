use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::{AppEntry, CompletionType, Config, ScanDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchItem {
    pub name: String,
    pub path: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub source: ItemSource,
    #[serde(default)]
    pub completion: CompletionType,
    #[serde(default)]
    pub completion_list: Vec<String>,
    pub completion_command: Option<String>,
    pub completion_search_mode: Option<crate::config::SearchMode>,
    /// history での sort キー。`path\targs` 形式。None なら path を使う。
    #[serde(default)]
    pub history_key: Option<String>,
    /// override で path が差し替えられた場合の元ファイルパス。
    /// テンプレート内で {{ file_path }} 等として参照できる。
    #[serde(default)]
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemSource {
    Config,
    ScanDir,
    System,
    Url,
    Path,
    History,
}

pub fn render_template(template: &str, ctx: &tera::Context) -> String {
    tera::Tera::one_off(template, ctx, false).unwrap_or_else(|_| template.to_string())
}

pub fn build_template_context(
    extra_args: &[String],
    vars: &std::collections::HashMap<String, String>,
    source_file: Option<&str>,
) -> tera::Context {
    let mut ctx = tera::Context::new();
    ctx.insert("args", &extra_args.join(" "));
    ctx.insert("args_list", extra_args);
    // 環境変数を {{ env.VAR_NAME }} として使えるようにする
    let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
    ctx.insert("env", &env_map);
    // ユーザー定義変数を {{ vars.xxx }} として使えるようにする
    ctx.insert("vars", vars);
    // override で差し替えられた元ファイルのパス変数
    if let Some(sf) = source_file {
        let p = std::path::Path::new(sf);
        ctx.insert("file_path", sf);
        ctx.insert(
            "file_name",
            p.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        );
        ctx.insert(
            "file_stem",
            p.file_stem().and_then(|n| n.to_str()).unwrap_or(""),
        );
        ctx.insert(
            "file_ext",
            p.extension().and_then(|n| n.to_str()).unwrap_or(""),
        );
        ctx.insert(
            "file_dir",
            p.parent().and_then(|d| d.to_str()).unwrap_or(""),
        );
    }
    ctx
}

pub fn launch_with_extra(
    item: &LaunchItem,
    extra_args: Vec<String>,
    vars: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    // path / args / workdir にテンプレートマーカーがあれば展開
    let has_template = item.path.contains("{{")
        || item.args.iter().any(|a| a.contains("{{"))
        || item.workdir.as_deref().is_some_and(|w| w.contains("{{"));
    if has_template || !extra_args.is_empty() {
        let ctx = build_template_context(&extra_args, vars, item.source_file.as_deref());
        let rendered_path = render_template(&item.path, &ctx);
        let rendered_args: Vec<String> =
            item.args.iter().map(|a| render_template(a, &ctx)).collect();
        let rendered_workdir = item.workdir.as_deref().map(|w| render_template(w, &ctx));

        let path_rendered = rendered_path != item.path;
        let args_rendered = rendered_args != item.args;
        let workdir_rendered = rendered_workdir.as_deref() != item.workdir.as_deref();

        if path_rendered || args_rendered || workdir_rendered {
            let mut item_rendered = item.clone();
            item_rendered.path = rendered_path;
            // path か args がテンプレートで変化した → extra_args はテンプレート経由で渡り済み
            // workdir だけ変化した（args テンプレートなし）→ extra_args は args 末尾に追加
            item_rendered.args = if path_rendered || args_rendered {
                rendered_args
            } else {
                let mut a = rendered_args;
                a.extend(extra_args);
                a
            };
            item_rendered.workdir = rendered_workdir.or_else(|| item.workdir.clone());
            return launch(&item_rendered);
        }
    }

    // テンプレートなし: 従来どおり extra_args を末尾に追加
    let mut all_args = item.args.clone();
    all_args.extend(extra_args);
    let mut item_with_args = item.clone();
    item_with_args.args = all_args;
    launch(&item_with_args)
}

pub fn launch(item: &LaunchItem) -> Result<(), String> {
    let path = crate::utils::expand_path(&item.path);
    // Windows: 内部正規化（/ 統一）を OS ネイティブ形式（\）に戻す
    #[cfg(target_os = "windows")]
    let path = path.replace('/', "\\");
    let expanded_args: Vec<String> = item
        .args
        .iter()
        .map(|a| crate::utils::expand_path(a))
        .collect();

    // 共通処理: ユーザー引数と作業ディレクトリを cmd に追加するクロージャ
    let add_common = |c: &mut std::process::Command| {
        if !expanded_args.is_empty() {
            c.args(&expanded_args);
        }
        if let Some(workdir) = &item.workdir {
            c.current_dir(crate::utils::expand_path(workdir));
        }
    };

    let mut cmd = std::process::Command::new(&path);
    add_common(&mut cmd);

    // Windows の .lnk / .cmd / .bat ファイルは cmd /c で起動
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        let p = path.to_lowercase();
        if p.ends_with(".lnk") {
            // .lnk は start 経由: cmd 自体は非表示でよい
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", "start", "", &path]);
            c.creation_flags(CREATE_NO_WINDOW);
            add_common(&mut c);
            c
        } else if p.ends_with(".cmd") || p.ends_with(".bat") {
            // .cmd/.bat は新しいコンソールウィンドウで起動
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", &path]);
            c.creation_flags(CREATE_NEW_CONSOLE);
            add_common(&mut c);
            c
        } else if p.ends_with(".ps1") {
            // .ps1 は新しいコンソールウィンドウで powershell 起動
            let mut c = std::process::Command::new("powershell");
            c.args(["-NoProfile", "-ExecutionPolicy", "ByPass", "-File", &path]);
            c.creation_flags(CREATE_NEW_CONSOLE);
            add_common(&mut c);
            c
        } else {
            // 拡張子なしのコマンド（scoop, npm, git など）は PATHEXT で解決
            match resolve_windows_cmd(&path) {
                ResolvedCmd::Cmd(resolved) | ResolvedCmd::Bat(resolved) => {
                    let mut c = std::process::Command::new("cmd");
                    c.args(["/c", &resolved]);
                    c.creation_flags(CREATE_NEW_CONSOLE);
                    add_common(&mut c);
                    c
                }
                ResolvedCmd::Ps1(resolved) => {
                    let mut c = std::process::Command::new("powershell");
                    c.args([
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "ByPass",
                        "-File",
                        &resolved,
                    ]);
                    c.creation_flags(CREATE_NEW_CONSOLE);
                    add_common(&mut c);
                    c
                }
                ResolvedCmd::Other => {
                    // .exe 以外の非スクリプトファイル (.xlsx, .pdf, .py 等) かつ
                    // args/workdir 指定なし → OS 関連付けで開く
                    if expanded_args.is_empty() && item.workdir.is_none() {
                        return tauri_plugin_opener::open_path(&path, None::<&str>)
                            .map_err(|e| e.to_string());
                    }
                    cmd
                }
            }
        }
    };

    // macOS: System アイテム (.app バンドル等) と Path アイテム (ファイル/ディレクトリ) は
    // `open` コマンド経由で起動する。
    // 実行bit なし かつ args/workdir なしのファイル (.xlsx, .pdf 等) も OS 関連付けで開く。
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let use_open = matches!(item.source, ItemSource::System | ItemSource::Path)
            || path.to_lowercase().ends_with(".app");
        if use_open {
            // `open` は `--args` セパレータが必要なので add_common は使えない
            let mut c = std::process::Command::new("open");
            c.arg(&path);
            if !expanded_args.is_empty() {
                c.arg("--args");
                c.args(&expanded_args);
            }
            if let Some(workdir) = &item.workdir {
                c.current_dir(crate::utils::expand_path(workdir));
            }
            c
        } else {
            // 実行bit なし かつ args/workdir なし → OS 関連付けで開く
            use std::os::unix::fs::PermissionsExt;
            let is_executable = std::fs::metadata(&path)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(true);
            if !is_executable && expanded_args.is_empty() && item.workdir.is_none() {
                return tauri_plugin_opener::open_path(&path, None::<&str>)
                    .map_err(|e| e.to_string());
            }
            cmd
        }
    };

    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "windows")]
enum ResolvedCmd {
    Cmd(String),
    Bat(String),
    Ps1(String),
    Other,
}

/// 拡張子なしのコマンド名を PATHEXT で解決する
#[cfg(target_os = "windows")]
fn resolve_windows_cmd(name: &str) -> ResolvedCmd {
    use std::path::Path;
    // すでに拡張子がある or パス区切りを含む場合はそのまま
    let p = Path::new(name);
    if p.extension().is_some() || name.contains('/') || name.contains('\\') {
        return ResolvedCmd::Other;
    }
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.PS1".to_string());
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        for ext in pathext.split(';') {
            let full = dir.join(format!("{}{}", name, ext));
            if full.exists() {
                let resolved = full.to_string_lossy().to_string();
                let ext_lower = ext.to_lowercase();
                return if ext_lower == ".cmd" {
                    ResolvedCmd::Cmd(resolved)
                } else if ext_lower == ".bat" {
                    ResolvedCmd::Bat(resolved)
                } else if ext_lower == ".ps1" {
                    ResolvedCmd::Ps1(resolved)
                } else {
                    ResolvedCmd::Other
                };
            }
        }
    }
    ResolvedCmd::Other
}

pub fn collect_items(config: &Config) -> Vec<LaunchItem> {
    let mut items = vec![];

    // config [[apps]] から追加
    for app in &config.apps {
        items.push(launch_item_from_entry(app));
    }

    // [[scan_dirs]] をスキャン
    for scan_dir in &config.scan_dirs {
        items.extend(scan_directory(scan_dir));
    }

    // OS 標準アプリ
    items.extend(collect_system_apps());

    // 履歴にある URL / Path アイテムを復元
    items.extend(history_items(config));

    // [[overrides]] を name (stem, 大文字小文字無視) または ext (拡張子) でマッチして上書き
    for item in &mut items {
        let item_ext = std::path::Path::new(&item.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if let Some(ov) = config.overrides.iter().find(|o| {
            let name_match =
                !o.name.is_empty() && o.name.to_lowercase() == item.name.to_lowercase();
            let ext_match = o
                .ext
                .as_deref()
                .is_some_and(|e| e.to_lowercase() == item_ext);
            name_match || ext_match
        }) {
            // path が指定されていれば元ファイルを source_file に保存して差し替え
            if let Some(ref v) = ov.path {
                item.source_file = Some(item.path.clone());
                item.path = v.clone();
            }
            if let Some(ref v) = ov.completion {
                item.completion = v.clone();
            }
            if !ov.completion_list.is_empty() {
                item.completion_list = ov.completion_list.clone();
            }
            if ov.completion_command.is_some() {
                item.completion_command = ov.completion_command.clone();
            }
            if let Some(ref v) = ov.args {
                item.args = v.clone();
            }
            if ov.workdir.is_some() {
                item.workdir = ov.workdir.clone();
            }
        }
    }

    items
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn is_path(s: &str) -> bool {
    s == "~"
        || s.starts_with("~/")
        || s.starts_with("~\\")
        || s.starts_with('/')
        || s.starts_with("\\\\")  // UNC path: \\server\share
        || (s.len() >= 3 && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s[1..].starts_with(":/"))
        || (s.len() >= 3 && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s[1..].starts_with(":\\"))
}

fn history_items(config: &Config) -> Vec<LaunchItem> {
    let history = crate::history::load();
    history
        .entries
        .keys()
        .filter_map(|key| {
            if let Some(tab_idx) = key.find('\t') {
                // `path\targs` 形式 → History アイテムとして復元
                let exe_path = &key[..tab_idx];
                let args_str = &key[tab_idx + 1..];
                let args: Vec<String> = args_str.split_whitespace().map(String::from).collect();
                // まず name で逆引き（Config アイテムは name をキーに記録）、
                // 次に path で検索（旧形式との互換性）
                let app_entry = config
                    .apps
                    .iter()
                    .find(|a| a.name == exe_path)
                    .or_else(|| config.apps.iter().find(|a| a.path == exe_path));
                let (app_name, launch_path) = if let Some(entry) = app_entry {
                    (entry.name.clone(), entry.path.clone())
                } else {
                    let name = std::path::Path::new(exe_path)
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or(exe_path)
                        .to_string();
                    (name, exe_path.to_string())
                };
                Some(LaunchItem {
                    name: format!("{} › {}", app_name, args_str),
                    path: launch_path,
                    args,
                    workdir: None,
                    source: ItemSource::History,
                    completion: CompletionType::None,
                    completion_list: vec![],
                    completion_command: None,
                    completion_search_mode: None,
                    history_key: Some(key.clone()),
                    source_file: None,
                })
            } else if is_url(key) && !key.contains("{{") {
                // テンプレート URL（{{ }} を含む）は直接開けないのでスキップ
                Some(LaunchItem {
                    name: key.clone(),
                    path: key.clone(),
                    args: vec![],
                    workdir: None,
                    source: ItemSource::Url,
                    completion: CompletionType::None,
                    completion_list: vec![],
                    completion_command: None,
                    completion_search_mode: None,
                    history_key: None,
                    source_file: None,
                })
            } else if is_path(key) {
                Some(LaunchItem {
                    name: key.clone(),
                    path: key.clone(),
                    args: vec![],
                    workdir: None,
                    source: ItemSource::Path,
                    completion: CompletionType::None,
                    completion_list: vec![],
                    completion_command: None,
                    completion_search_mode: None,
                    history_key: None,
                    source_file: None,
                })
            } else {
                None
            }
        })
        .collect()
}

fn launch_item_from_entry(app: &AppEntry) -> LaunchItem {
    LaunchItem {
        name: app.name.clone(),
        path: app.path.clone(),
        args: app.args.clone(),
        workdir: app.workdir.clone(),
        source: ItemSource::Config,
        completion: app.completion.clone(),
        completion_list: app.completion_list.clone(),
        completion_command: app.completion_command.clone(),
        completion_search_mode: app.completion_search_mode.clone(),
        history_key: None,
        source_file: None,
    }
}

fn scan_directory(scan_dir: &ScanDir) -> Vec<LaunchItem> {
    let path = crate::utils::expand_path(&scan_dir.path);
    let path = Path::new(&path);
    if !path.exists() {
        return vec![];
    }

    let mut items = vec![];
    collect_files(path, scan_dir.recursive, &scan_dir.extensions, &mut items);
    items
}

fn collect_files(
    dir: &Path,
    recursive: bool,
    extensions: &Option<Vec<String>>,
    items: &mut Vec<LaunchItem>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && recursive {
            collect_files(&path, recursive, extensions, items);
        } else if path.is_file() {
            if let Some(exts) = extensions {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !exts.iter().any(|e| e.to_lowercase() == ext) {
                    continue;
                }
            }
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            items.push(LaunchItem {
                name,
                path: path.to_string_lossy().to_string(),
                args: vec![],
                workdir: None,
                source: ItemSource::ScanDir,
                completion: CompletionType::Path,
                completion_list: vec![],
                completion_command: None,
                completion_search_mode: None,
                history_key: None,
                source_file: None,
            });
        }
    }
}

#[cfg(target_os = "windows")]
fn collect_system_apps() -> Vec<LaunchItem> {
    let mut items = vec![];
    let dirs = [
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("Microsoft/Windows/Start Menu/Programs")),
        Some(PathBuf::from(
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs",
        )),
    ];
    for dir in dirs.into_iter().flatten() {
        collect_lnk_files(&dir, &mut items);
    }
    items
}

#[cfg(target_os = "windows")]
fn collect_lnk_files(dir: &Path, items: &mut Vec<LaunchItem>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lnk_files(&path, items);
        } else if path.extension().and_then(|e| e.to_str()) == Some("lnk") {
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            items.push(LaunchItem {
                name,
                path: path.to_string_lossy().to_string(),
                args: vec![],
                workdir: None,
                source: ItemSource::System,
                completion: CompletionType::Path,
                completion_list: vec![],
                completion_command: None,
                completion_search_mode: None,
                history_key: None,
                source_file: None,
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_system_apps() -> Vec<LaunchItem> {
    let mut items = vec![];
    // macOS 10.15+ ではシステムアプリが /System/Applications に移動した
    let dirs: Vec<PathBuf> = {
        let mut v = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/Applications/Utilities"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
        ];
        // ~/Applications (ユーザーインストール)
        if let Some(home) = dirs_next::home_dir() {
            v.push(home.join("Applications"));
        }
        v
    };
    for dir in &dirs {
        collect_app_bundles(dir, &mut items);
    }
    items
}

#[cfg(target_os = "macos")]
fn collect_app_bundles(dir: &Path, items: &mut Vec<LaunchItem>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                items.push(LaunchItem {
                    name,
                    path: path.to_string_lossy().to_string(),
                    args: vec![],
                    workdir: None,
                    source: ItemSource::System,
                    completion: CompletionType::None,
                    completion_list: vec![],
                    completion_command: None,
                    completion_search_mode: None,
                    history_key: None,
                    source_file: None,
                });
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_system_apps() -> Vec<LaunchItem> {
    let xdg_dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        dirs_next::data_local_dir()
            .unwrap_or_default()
            .join("applications"),
    ];

    let mut items = vec![];
    for dir in xdg_dirs {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                if let Some(item) = parse_desktop_file(&path) {
                    items.push(item);
                }
            }
        }
    }
    items
}

#[cfg(target_os = "linux")]
fn parse_desktop_file(path: &Path) -> Option<LaunchItem> {
    use std::io::BufRead;

    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut name = None;
    let mut exec = None;
    let mut no_display = false;

    for line in reader.lines().map_while(|l| l.ok()) {
        if line.starts_with("Name=") && name.is_none() {
            name = Some(line[5..].to_string());
        } else if line.starts_with("Exec=") && exec.is_none() {
            // %u %f %U %F などの field codes を除去
            let cmd = line[5..].to_string();
            let cmd = cmd
                .split_whitespace()
                .filter(|s| !s.starts_with('%'))
                .collect::<Vec<_>>()
                .join(" ");
            exec = Some(cmd);
        } else if line == "NoDisplay=true" {
            no_display = true;
        }
    }

    if no_display {
        return None;
    }

    Some(LaunchItem {
        name: name?,
        path: exec?,
        args: vec![],
        workdir: None,
        source: ItemSource::System,
        completion: CompletionType::Path,
        completion_list: vec![],
        completion_command: None,
        completion_search_mode: None,
        history_key: None,
        source_file: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_url ---

    #[test]
    fn url_http() {
        assert!(is_url("http://example.com"));
    }

    #[test]
    fn url_https() {
        assert!(is_url("https://example.com/path?q=1"));
    }

    #[test]
    fn url_rejects_ftp() {
        assert!(!is_url("ftp://example.com"));
    }

    #[test]
    fn url_rejects_plain() {
        assert!(!is_url("example.com"));
    }

    // --- is_path ---

    #[test]
    fn path_tilde_alone() {
        assert!(is_path("~"));
    }

    #[test]
    fn path_tilde_slash() {
        assert!(is_path("~/documents"));
    }

    #[test]
    fn path_tilde_backslash() {
        assert!(is_path("~\\AppData"));
    }

    #[test]
    fn path_unix_absolute() {
        assert!(is_path("/usr/bin/bash"));
    }

    #[test]
    fn path_windows_drive_forward_slash() {
        assert!(is_path("C:/Users"));
        assert!(is_path("D:/"));
    }

    #[test]
    fn path_windows_drive_backslash() {
        assert!(is_path("C:\\Users"));
    }

    #[test]
    fn path_rejects_plain_command() {
        assert!(!is_path("notepad"));
    }

    #[test]
    fn path_rejects_drive_without_separator() {
        assert!(!is_path("C:"));
    }

    #[test]
    fn path_rejects_relative() {
        assert!(!is_path("usr/bin/bash"));
    }

    #[test]
    fn path_unc() {
        assert!(is_path("\\\\server\\share"));
        assert!(is_path("\\\\server\\share\\folder"));
        // 正規化済み UNC（to_slash 後）は starts_with('/') で検出される
        assert!(is_path("//server/share"));
        assert!(is_path("//server/share/folder"));
    }

    // --- render_template ---

    #[test]
    fn template_args_substitution() {
        let ctx = build_template_context(&["hello world".to_string()], &Default::default(), None);
        assert_eq!(render_template("{{ args }}", &ctx), "hello world");
    }

    #[test]
    fn template_args_urlencode() {
        let ctx = build_template_context(&["hello world".to_string()], &Default::default(), None);
        let result = render_template("{{ args | urlencode }}", &ctx);
        assert_eq!(result, "hello%20world");
    }

    #[test]
    fn template_args_list() {
        let ctx = build_template_context(
            &["foo".to_string(), "bar".to_string()],
            &Default::default(),
            None,
        );
        assert_eq!(
            render_template("{{ args_list | join(sep=',') }}", &ctx),
            "foo,bar"
        );
    }

    #[test]
    fn template_no_placeholder_unchanged() {
        let ctx = build_template_context(&["something".to_string()], &Default::default(), None);
        assert_eq!(
            render_template("https://example.com", &ctx),
            "https://example.com"
        );
    }

    #[test]
    fn template_url_search() {
        let ctx = build_template_context(
            &["rust borrow checker".to_string()],
            &Default::default(),
            None,
        );
        let result = render_template(
            "https://www.google.com/search?q={{ args | urlencode }}",
            &ctx,
        );
        assert_eq!(
            result,
            "https://www.google.com/search?q=rust%20borrow%20checker"
        );
    }

    #[test]
    fn template_env_var() {
        std::env::set_var("SHUN_TEST_VAR", "hello");
        let ctx = build_template_context(&[], &Default::default(), None);
        let result = render_template("{{ env.SHUN_TEST_VAR }}/world", &ctx);
        assert_eq!(result, "hello/world");
    }

    #[test]
    fn template_vars_substitution() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("src_dir".to_string(), "/home/user/src".to_string());
        let ctx = build_template_context(&["myproject".to_string()], &vars, None);
        let result = render_template("{{ vars.src_dir }}/{{ args }}", &ctx);
        assert_eq!(result, "/home/user/src/myproject");
    }

    #[test]
    fn template_vars_empty_by_default() {
        let ctx = build_template_context(&[], &Default::default(), None);
        // 未定義 vars は Tera エラーになるが render_template はそのまま返す
        let result = render_template("{{ vars.missing | default(value=\"fallback\") }}", &ctx);
        assert_eq!(result, "fallback");
    }

    // --- args tilde expansion ---

    #[test]
    fn tilde_in_args_expands_to_home() {
        // regression: args = ["~/.memolist/todo.md"] must be tilde-expanded before spawning
        let home = dirs_next::home_dir().expect("home dir must exist");
        let expanded = crate::utils::expand_path("~/.memolist/todo.md");
        let expected = home
            .join(".memolist/todo.md")
            .to_string_lossy()
            .replace('\\', "/");
        assert_eq!(expanded.replace('\\', "/"), expected);
    }

    #[test]
    fn plain_args_unchanged() {
        // non-tilde args must pass through expand_path unmodified
        let expanded = crate::utils::expand_path("/absolute/path.md");
        assert_eq!(expanded, "/absolute/path.md");
    }

    #[test]
    fn vars_tilde_via_template_expands() {
        // [vars] memo_path = "~/.memolist/todo.md"
        // args = ["{{ vars.memo_path }}"]
        // → Tera renders to "~/.memolist/todo.md", then launch() expand_path expands ~
        let mut vars = std::collections::HashMap::new();
        vars.insert("memo_path".to_string(), "~/.memolist/todo.md".to_string());
        let ctx = build_template_context(&[], &vars, None);
        let rendered = render_template("{{ vars.memo_path }}", &ctx);
        // rendered is still "~/.memolist/todo.md" — expand_path in launch() finishes the job
        assert_eq!(rendered, "~/.memolist/todo.md");
        let expanded = crate::utils::expand_path(&rendered);
        assert!(!expanded.starts_with('~'), "tilde not expanded: {expanded}");
    }

    #[test]
    fn vars_nested_template_not_rendered() {
        // [vars] memo_path = "{{ env.USERPROFILE }}/.memolist/todo.md"
        // Tera does NOT recursively render substituted values,
        // so {{ vars.memo_path }} outputs the literal "{{ env.USERPROFILE }}/..."
        // Users should use ~ or direct {{ env.USERPROFILE }} in args instead.
        let mut vars = std::collections::HashMap::new();
        vars.insert(
            "memo_path".to_string(),
            "{{ env.USERPROFILE }}/.memolist/todo.md".to_string(),
        );
        let ctx = build_template_context(&[], &vars, None);
        let rendered = render_template("{{ vars.memo_path }}", &ctx);
        // the inner {{ }} is NOT re-rendered
        assert!(
            rendered.contains("{{ env.USERPROFILE }}"),
            "unexpected double-render: {rendered}"
        );
    }

    // --- file template variables ---

    #[test]
    fn template_file_variables() {
        let ctx = build_template_context(
            &[],
            &Default::default(),
            Some("/home/user/docs/report.xlsx"),
        );
        assert_eq!(
            render_template("{{ file_path }}", &ctx),
            "/home/user/docs/report.xlsx"
        );
        assert_eq!(render_template("{{ file_name }}", &ctx), "report.xlsx");
        assert_eq!(render_template("{{ file_stem }}", &ctx), "report");
        assert_eq!(render_template("{{ file_ext }}", &ctx), "xlsx");
        assert_eq!(render_template("{{ file_dir }}", &ctx), "/home/user/docs");
    }

    #[test]
    fn template_file_variables_absent_when_no_source_file() {
        let ctx = build_template_context(&[], &Default::default(), None);
        // undefined variables fall back to the raw template (Tera error → original string)
        let result = render_template("{{ file_path | default(value=\"none\") }}", &ctx);
        assert_eq!(result, "none");
    }

    // --- launch_with_extra merges args ---

    #[test]
    fn launch_with_extra_merges_item_and_extra_args() {
        let item = LaunchItem {
            name: "test".to_string(),
            path: "echo".to_string(),
            args: vec!["--flag".to_string()],
            workdir: None,
            source: ItemSource::Config,
            completion: CompletionType::None,
            completion_list: vec![],
            completion_command: None,
            completion_search_mode: None,
            history_key: None,
            source_file: None,
        };
        // launch_with_extra builds merged args internally; we verify it doesn't panic
        // by using an extra_args that won't actually spawn anything harmful
        // (echo exits cleanly)
        let result = launch_with_extra(&item, vec!["extra".to_string()], &Default::default());
        // On CI echo may or may not be available, so just assert no arg-construction panic
        let _ = result;
    }
}
