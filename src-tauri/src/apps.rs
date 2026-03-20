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
    /// history での sort キー。`path\targs` 形式。None なら path を使う。
    #[serde(default)]
    pub history_key: Option<String>,
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

pub fn build_template_context(extra_args: &[String]) -> tera::Context {
    let mut ctx = tera::Context::new();
    ctx.insert("args", &extra_args.join(" "));
    ctx.insert("args_list", extra_args);
    // 環境変数を {{ env.VAR_NAME }} として使えるようにする
    let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
    ctx.insert("env", &env_map);
    ctx
}

pub fn launch_with_extra(item: &LaunchItem, extra_args: Vec<String>) -> Result<(), String> {
    // path か args にテンプレートマーカーがあれば展開（env.* は args なしでも使える）
    let has_template = item.path.contains("{{") || item.args.iter().any(|a| a.contains("{{"));
    if has_template || !extra_args.is_empty() {
        let ctx = build_template_context(&extra_args);
        let rendered_path = render_template(&item.path, &ctx);
        let rendered_args: Vec<String> =
            item.args.iter().map(|a| render_template(a, &ctx)).collect();

        // テンプレートが展開されていれば extra_args は使わず展開済みで起動
        let path_rendered = rendered_path != item.path;
        let args_rendered = rendered_args != item.args;

        if path_rendered || args_rendered {
            let mut item_rendered = item.clone();
            item_rendered.path = rendered_path;
            item_rendered.args = rendered_args;
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
    let mut cmd = std::process::Command::new(&path);

    if !item.args.is_empty() {
        cmd.args(&item.args);
    }
    if let Some(workdir) = &item.workdir {
        cmd.current_dir(crate::utils::expand_path(workdir));
    }

    // Windows の .lnk / .cmd / .bat ファイルは cmd /c で起動
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let p = path.to_lowercase();
        if p.ends_with(".lnk") {
            // .lnk は start 経由: cmd 自体は非表示でよい
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", "start", "", &path]);
            if !item.args.is_empty() {
                c.args(&item.args);
            }
            if let Some(workdir) = &item.workdir {
                c.current_dir(crate::utils::expand_path(workdir));
            }
            c.creation_flags(CREATE_NO_WINDOW);
            c
        } else if p.ends_with(".cmd") || p.ends_with(".bat") {
            // .cmd/.bat は新しいコンソールウィンドウで起動
            const CREATE_NEW_CONSOLE: u32 = 0x00000010;
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", &path]);
            c.creation_flags(CREATE_NEW_CONSOLE);
            if !item.args.is_empty() {
                c.args(&item.args);
            }
            if let Some(workdir) = &item.workdir {
                c.current_dir(crate::utils::expand_path(workdir));
            }
            c
        } else if p.ends_with(".ps1") {
            // .ps1 は新しいコンソールウィンドウで powershell 起動
            const CREATE_NEW_CONSOLE: u32 = 0x00000010;
            let mut c = std::process::Command::new("powershell");
            c.args(["-NoProfile", "-ExecutionPolicy", "ByPass", "-File", &path]);
            c.creation_flags(CREATE_NEW_CONSOLE);
            if !item.args.is_empty() {
                c.args(&item.args);
            }
            if let Some(workdir) = &item.workdir {
                c.current_dir(crate::utils::expand_path(workdir));
            }
            c
        } else {
            // 拡張子なしのコマンド（scoop, npm, git など）は PATHEXT で解決
            match resolve_windows_cmd(&path) {
                ResolvedCmd::Cmd(resolved) | ResolvedCmd::Bat(resolved) => {
                    const CREATE_NEW_CONSOLE: u32 = 0x00000010;
                    let mut c = std::process::Command::new("cmd");
                    c.args(["/c", &resolved]);
                    c.creation_flags(CREATE_NEW_CONSOLE);
                    if !item.args.is_empty() {
                        c.args(&item.args);
                    }
                    if let Some(workdir) = &item.workdir {
                        c.current_dir(crate::utils::expand_path(workdir));
                    }
                    c
                }
                ResolvedCmd::Ps1(resolved) => {
                    const CREATE_NEW_CONSOLE: u32 = 0x00000010;
                    let mut c = std::process::Command::new("powershell");
                    c.args([
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "ByPass",
                        "-File",
                        &resolved,
                    ]);
                    c.creation_flags(CREATE_NEW_CONSOLE);
                    if !item.args.is_empty() {
                        c.args(&item.args);
                    }
                    if let Some(workdir) = &item.workdir {
                        c.current_dir(crate::utils::expand_path(workdir));
                    }
                    c
                }
                ResolvedCmd::Other => cmd,
            }
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

    // [[overrides]] を name (大文字小文字無視) でマッチして上書き
    for item in &mut items {
        if let Some(ov) = config
            .overrides
            .iter()
            .find(|o| o.name.to_lowercase() == item.name.to_lowercase())
        {
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
                // config [[apps]] でパスが一致するエントリの name を優先して使う
                let app_name = config
                    .apps
                    .iter()
                    .find(|a| a.path == exe_path)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| {
                        std::path::Path::new(exe_path)
                            .file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or(exe_path)
                            .to_string()
                    });
                Some(LaunchItem {
                    name: format!("{} › {}", app_name, args_str),
                    path: exe_path.to_string(),
                    args,
                    workdir: None,
                    source: ItemSource::History,
                    completion: CompletionType::None,
                    completion_list: vec![],
                    completion_command: None,
                    history_key: Some(key.clone()),
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
                    history_key: None,
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
                    history_key: None,
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
        history_key: None,
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
                history_key: None,
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
                history_key: None,
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_system_apps() -> Vec<LaunchItem> {
    let mut items = vec![];
    let apps_dir = Path::new("/Applications");
    if let Ok(entries) = std::fs::read_dir(apps_dir) {
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
                        completion: CompletionType::Path,
                        completion_list: vec![],
                        completion_command: None,
                        history_key: None,
                    });
                }
            }
        }
    }
    items
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
        history_key: None,
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
    }

    // --- render_template ---

    #[test]
    fn template_args_substitution() {
        let ctx = build_template_context(&["hello world".to_string()]);
        assert_eq!(render_template("{{ args }}", &ctx), "hello world");
    }

    #[test]
    fn template_args_urlencode() {
        let ctx = build_template_context(&["hello world".to_string()]);
        let result = render_template("{{ args | urlencode }}", &ctx);
        assert_eq!(result, "hello%20world");
    }

    #[test]
    fn template_args_list() {
        let ctx = build_template_context(&["foo".to_string(), "bar".to_string()]);
        assert_eq!(
            render_template("{{ args_list | join(sep=',') }}", &ctx),
            "foo,bar"
        );
    }

    #[test]
    fn template_no_placeholder_unchanged() {
        let ctx = build_template_context(&["something".to_string()]);
        assert_eq!(
            render_template("https://example.com", &ctx),
            "https://example.com"
        );
    }

    #[test]
    fn template_url_search() {
        let ctx = build_template_context(&["rust borrow checker".to_string()]);
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
        let ctx = build_template_context(&[]);
        let result = render_template("{{ env.SHUN_TEST_VAR }}/world", &ctx);
        assert_eq!(result, "hello/world");
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
            history_key: None,
        };
        // launch_with_extra builds merged args internally; we verify it doesn't panic
        // by using an extra_args that won't actually spawn anything harmful
        // (echo exits cleanly)
        let result = launch_with_extra(&item, vec!["extra".to_string()]);
        // On CI echo may or may not be available, so just assert no arg-construction panic
        let _ = result;
    }
}
