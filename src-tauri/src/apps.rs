use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::{AppEntry, CompletionType, Config, ScanDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchItem {
    pub name: String,
    pub path: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub allow_extra_args: bool,
    pub source: ItemSource,
    #[serde(default)]
    pub completion: CompletionType,
    #[serde(default)]
    pub completion_list: Vec<String>,
    pub completion_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemSource {
    Config,
    ScanDir,
    System,
}

pub fn launch_with_extra(item: &LaunchItem, extra_args: Vec<String>) -> Result<(), String> {
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
            c.creation_flags(CREATE_NO_WINDOW);
            c
        } else if p.ends_with(".cmd") || p.ends_with(".bat") {
            // .cmd/.bat はコンソール表示が必要な場合があるのでそのまま
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", &path]);
            if !item.args.is_empty() {
                c.args(&item.args);
            }
            if let Some(workdir) = &item.workdir {
                c.current_dir(crate::utils::expand_path(workdir));
            }
            c
        } else {
            cmd
        }
    };

    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
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

    items
}

fn launch_item_from_entry(app: &AppEntry) -> LaunchItem {
    LaunchItem {
        name: app.name.clone(),
        path: app.path.clone(),
        args: app.args.clone(),
        workdir: app.workdir.clone(),
        allow_extra_args: app.allow_extra_args,
        source: ItemSource::Config,
        completion: app.completion.clone(),
        completion_list: app.completion_list.clone(),
        completion_command: app.completion_command.clone(),
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
                allow_extra_args: false,
                source: ItemSource::ScanDir,
                completion: CompletionType::Path,
                completion_list: vec![],
                completion_command: None,
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
                allow_extra_args: false,
                source: ItemSource::System,
                completion: CompletionType::Path,
                completion_list: vec![],
                completion_command: None,
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
                        allow_extra_args: false,
                        source: ItemSource::System,
                        completion: CompletionType::Path,
                        completion_list: vec![],
                        completion_command: None,
                    });
                }
            }
        }
    }
    items
}

#[cfg(target_os = "linux")]
fn collect_system_apps() -> Vec<LaunchItem> {
    use std::io::{BufRead, BufReader};

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

    for line in reader.lines().flatten() {
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
        allow_extra_args: false,
        source: ItemSource::System,
        completion: CompletionType::Path,
        completion_list: vec![],
        completion_command: None,
    })
}
