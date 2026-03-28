use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use log::{debug, info};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;
use tokio::io::AsyncBufReadExt;

mod apps;
mod complete;
mod config;
mod history;
mod migemo;
mod search;
mod utils;

// --- キャッシュ ---

struct ItemCache {
    config: config::Config,
    items: Vec<apps::LaunchItem>,
}

type CacheState = Arc<Mutex<Option<ItemCache>>>;

fn build_cache() -> ItemCache {
    info!("build_cache: start");
    let (config, _warnings) = config::load_config();
    let items = apps::collect_items(&config);
    info!("build_cache: done ({} items)", items.len());
    ItemCache { config, items }
}

fn refresh_cache_bg(cache: CacheState) {
    info!("refresh_cache_bg: spawning background thread");
    std::thread::spawn(move || {
        let new = build_cache();
        *cache.lock().unwrap() = Some(new);
        info!("refresh_cache_bg: cache updated");
    });
}

// --- コマンド ---

#[derive(serde::Serialize)]
struct ConfigAndWarnings {
    config: config::Config,
    warnings: Vec<(String, String)>,
}

#[tauri::command]
fn get_config_and_warnings(state: tauri::State<WarningsState>) -> ConfigAndWarnings {
    let (config, config_warnings) = config::load_config();
    let runtime_warnings = state.lock().unwrap().clone();

    // launch key の警告を毎回動的チェック（config 修正後に /reload なしで即消えるよう）
    let launch_key = &config.keybindings.launch;
    let launch_warnings: Vec<(String, String)> = match launch_key.parse::<Shortcut>() {
        Err(_) => {
            let fallback = config::default_launch();
            vec![(
                "config.toml".to_string(),
                format!(
                    "keybindings.launch = \"{launch_key}\": invalid shortcut — falling back to \"{fallback}\""
                ),
            )]
        }
        Ok(s) if s.mods.intersects(Modifiers::SUPER | Modifiers::META) => vec![(
            "config.toml".to_string(),
            format!(
                "keybindings.launch = \"{launch_key}\": Windows/Meta key may be intercepted by the OS"
            ),
        )],
        Ok(_) => vec![],
    };

    ConfigAndWarnings {
        config,
        warnings: [config_warnings, runtime_warnings, launch_warnings].concat(),
    }
}

#[tauri::command]
fn get_apps() -> Vec<apps::LaunchItem> {
    let (config, _) = config::load_config();
    let hist = history::load();
    let mut items = apps::collect_items(&config);
    sort_items(&mut items, &hist, &config);
    items
}

#[tauri::command]
fn search_items(
    query: String,
    search_mode: Option<String>,
    sort_order: Option<String>,
    state: tauri::State<CacheState>,
) -> Vec<apps::LaunchItem> {
    let (config, mut items) = {
        let cache = state.lock().unwrap();
        match cache.as_ref() {
            Some(c) => (c.config.clone(), c.items.clone()),
            None => {
                drop(cache);
                let c = build_cache();
                let result = (c.config.clone(), c.items.clone());
                *state.lock().unwrap() = Some(c);
                result
            }
        }
    };

    let effective_search_mode = search_mode
        .as_deref()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
        .unwrap_or(config.search_mode.clone());
    let effective_sort_order = sort_order
        .as_deref()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
        .unwrap_or(config.sort_order.clone());

    // history は軽いので毎回ロード（起動直後も正確な順序に）
    let hist = history::load();
    sort_items_with_order(&mut items, &hist, &effective_sort_order);

    if query.is_empty() {
        return items;
    }
    search::filter(&items, &query, &effective_search_mode)
}

fn sort_items(items: &mut [apps::LaunchItem], hist: &history::History, config: &config::Config) {
    sort_items_with_order(items, hist, &config.sort_order);
}

fn sort_items_with_order(
    items: &mut [apps::LaunchItem],
    hist: &history::History,
    sort_order: &config::SortOrder,
) {
    items.sort_by(|a, b| {
        let a_key = a.history_key.as_deref().unwrap_or(&a.path);
        let b_key = b.history_key.as_deref().unwrap_or(&b.path);
        let (ac, at) = history::sort_key(hist, a_key);
        let (bc, bt) = history::sort_key(hist, b_key);
        if ac == 0 && bc == 0 {
            return a.name.to_lowercase().cmp(&b.name.to_lowercase());
        }
        match sort_order {
            config::SortOrder::CountFirst => bc.cmp(&ac).then(bt.cmp(&at)),
            config::SortOrder::RecentFirst => bt.cmp(&at).then(bc.cmp(&ac)),
        }
    });
}

#[derive(serde::Serialize)]
struct CompleteResult {
    prefix: String,
    completions: Vec<String>,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn complete_path(
    input: String,
    completion_type: config::CompletionType,
    completion_list: Vec<String>,
    completion_command: Option<String>,
    workdir: Option<String>,
    item_args: Option<Vec<String>>,
    completion_search_mode: Option<config::SearchMode>,
    state: tauri::State<CacheState>,
) -> CompleteResult {
    let (vars, global_search_mode) = {
        let cache = state.lock().unwrap();
        let vars = cache
            .as_ref()
            .map(|c| c.config.vars.clone())
            .unwrap_or_default();
        let mode = cache
            .as_ref()
            .map(|c| c.config.search_mode.clone())
            .unwrap_or_default();
        (vars, mode)
    };
    // per-app override があればそちらを、なければグローバル設定を使う
    let search_mode = completion_search_mode
        .as_ref()
        .unwrap_or(&global_search_mode);
    // テンプレート args から {{ args }} 前の固定部分をベースパスとして抽出
    let base_path = item_args
        .as_deref()
        .and_then(|a| extract_template_base_path(a, &vars));
    let (prefix, completions) = complete::complete(
        &input,
        &completion_type,
        &completion_list,
        &completion_command,
        &workdir,
        base_path.as_deref(),
        search_mode,
    );
    CompleteResult {
        prefix,
        completions,
    }
}

/// `args` テンプレートの最初の要素から `{{ args }}` 前の固定プレフィックスを取得・展開する
/// 例: `["{{ vars.src_dir }}/{{ args }}"]` → `Some("~/src/")` (vars展開済み)
fn extract_template_base_path(
    args: &[String],
    vars: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let first = args.first()?;
    let pos = first.find("{{ args }}")?;
    let prefix = &first[..pos];
    if prefix.is_empty() {
        return None;
    }
    let ctx = apps::build_template_context(&[], vars, None);
    let rendered = apps::render_template(prefix, &ctx);
    let rendered = rendered.replace('\\', "/");
    if !rendered.ends_with('/') {
        Some(format!("{}/", rendered))
    } else {
        Some(rendered)
    }
}

#[tauri::command]
fn launch_item(
    item: apps::LaunchItem,
    extra_args: Option<Vec<String>>,
    state: tauri::State<CacheState>,
) -> Result<(), String> {
    let extra = extra_args.unwrap_or_default();

    let (vars, history_max_items) = {
        let cache = state.lock().unwrap();
        let vars = cache
            .as_ref()
            .map(|c| c.config.vars.clone())
            .unwrap_or_default();
        let max = cache
            .as_ref()
            .map(|c| c.config.history_max_items)
            .unwrap_or(1000);
        (vars, max)
    };

    // history 記録
    if !extra.is_empty() && item.history_key.is_none() {
        // args ありで新規実行: combined key のみ記録（base は last_args だけ更新）
        // base も同時に record すると同じ秒になり recent_first の tiebreaker で base が勝ってしまうため
        // item.args にテンプレートが含まれる場合は展開した結果を記録する
        let history_args: Vec<String> = if item.args.iter().any(|a| a.contains("{{")) {
            let ctx = apps::build_template_context(&extra, &vars, item.source_file.as_deref());
            item.args
                .iter()
                .map(|a| apps::render_template(a, &ctx))
                .collect()
        } else {
            item.args.iter().chain(extra.iter()).cloned().collect()
        };
        // Config アイテムは name をキーに記録する（同じ exe を使う別エントリと区別するため）
        let record_key = if matches!(item.source, apps::ItemSource::Config) {
            &item.name
        } else {
            &item.path
        };
        history::record_args(record_key, &history_args, history_max_items);
    } else {
        // args なし or History アイテムの再実行: そのままのキーで記録
        let record_key = item.history_key.as_deref().unwrap_or(&item.path);
        history::record(record_key, history_max_items);
    }

    // 起動後にキャッシュを更新（次回表示時に history items が反映される）
    refresh_cache_bg(Arc::clone(state.inner()));

    // extra_args があればテンプレートを展開してから path を確定
    // History アイテムは extra が空でも item.args にすでに引数が入っているので使う
    let template_args = if !extra.is_empty() {
        extra.clone()
    } else {
        item.args.clone()
    };
    let path = if item.path.contains("{{") {
        let ctx = apps::build_template_context(&template_args, &vars, item.source_file.as_deref());
        apps::render_template(&item.path, &ctx)
    } else {
        item.path.clone()
    };

    if path.starts_with("http://") || path.starts_with("https://") {
        tauri_plugin_opener::open_url(&path, None::<&str>).map_err(|e| e.to_string())
    } else if matches!(item.source, apps::ItemSource::Path) {
        let expanded = utils::expand_path(path.trim_end_matches('/'));
        tauri_plugin_opener::open_path(expanded, None::<&str>).map_err(|e| e.to_string())
    } else {
        apps::launch_with_extra(&item, extra, &vars)
    }
}

#[tauri::command]
fn get_last_args(path: String) -> Option<String> {
    history::get_last_args(&path)
}

#[tauri::command]
fn get_args_history(path: String) -> Vec<String> {
    let hist = history::load();
    let (config, _) = config::load_config();
    let prefix = format!("{}\t", path);

    let mut entries: Vec<(String, u32, u64)> = hist
        .entries
        .iter()
        .filter_map(|(key, entry)| {
            key.strip_prefix(&prefix)
                .map(|args| (args.to_string(), entry.count, entry.last_used))
        })
        .collect();

    entries.sort_by(|(_, ac, at), (_, bc, bt)| match config.sort_order {
        config::SortOrder::CountFirst => bc.cmp(ac).then(bt.cmp(at)),
        config::SortOrder::RecentFirst => bt.cmp(at).then(bc.cmp(ac)),
    });

    entries.into_iter().map(|(args, _, _)| args).collect()
}

/// Registers the launch shortcut. Falls back to the default key if the configured key is invalid.
/// Returns `Err` only when even the fallback fails to register (should never happen).
fn register_launch_shortcut(app: &tauri::AppHandle) -> Result<(), String> {
    let launch_key = config::load_config().0.keybindings.launch;
    let shortcut: Shortcut = match launch_key.parse::<Shortcut>() {
        Ok(s) => s,
        Err(e) => {
            // 無効なキー文字列 → デフォルト (Ctrl+Space) にフォールバックして登録を続行
            // 警告は get_config_and_warnings() で動的に生成するので WarningsState には積まない
            let fallback_key = config::default_launch();
            log::warn!(
                "Invalid launch shortcut '{}': {e}. Falling back to '{fallback_key}'",
                launch_key
            );
            fallback_key.parse().map_err(|fe| {
                format!("Invalid launch shortcut '{launch_key}': {e}. Fallback '{fallback_key}' also invalid: {fe}")
            })?
        }
    };
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let cache = Arc::clone(app.state::<CacheState>().inner());
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        debug!("shortcut: window visible → hide");
                        window.hide().ok();
                        refresh_cache_bg(cache);
                    } else {
                        debug!("shortcut: window hidden → show");
                        let cfg = config::load_config().0;
                        center_on_monitor(&window, &cfg.monitor, cfg.window_width as f64);
                        window.show().ok();
                        window.set_focus().ok();
                        window.emit("show-launcher", ()).ok();
                    }
                }
            }
        })
        .map_err(|e| {
            log::warn!("Failed to register shortcut '{}': {e}", launch_key);
            e.to_string()
        })
}

#[tauri::command]
fn reload(
    app: tauri::AppHandle,
    state: tauri::State<CacheState>,
    warnings_state: tauri::State<WarningsState>,
) -> Result<(), String> {
    let (_, _) = config::load_config(); // config reload (warnings are fetched fresh in get_config_warnings)
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;

    // launch key 警告は get_config_and_warnings() で動的生成するので WarningsState は空にリセット
    // ショートカット登録が完全に失敗した場合のみ Err を返す（呼び出し元がエラー表示する）
    register_launch_shortcut(&app)?;
    *warnings_state.lock().unwrap() = Vec::new();

    refresh_cache_bg(Arc::clone(state.inner()));
    Ok(())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

fn last_update_check_path() -> std::path::PathBuf {
    config::config_path()
        .parent()
        .unwrap_or(&std::path::PathBuf::from("."))
        .join("last_update_check")
}

fn should_check_update(interval_secs: u64) -> bool {
    if interval_secs == 0 {
        return false;
    }
    let path = last_update_check_path();
    let Ok(meta) = std::fs::metadata(&path) else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    let Ok(elapsed) = modified.elapsed() else {
        return true;
    };
    elapsed.as_secs() >= interval_secs
}

fn record_update_check() {
    let path = last_update_check_path();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    match std::fs::write(&path, ts) {
        Ok(_) => log::info!("record_update_check: updated {}", path.display()),
        Err(e) => log::warn!(
            "record_update_check: failed to write {}: {e}",
            path.display()
        ),
    }
}

#[derive(Debug, PartialEq)]
enum InstallMethod {
    Portable,
    Scoop,
    Homebrew,
    Standard,
}

fn detect_install_method() -> InstallMethod {
    let exe = std::env::current_exe().ok();

    let exe_str = exe
        .as_ref()
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // パッケージマネージャーのパスを先に判定（portable.txt より優先）
    if exe_str.contains("\\scoop\\apps\\") {
        return InstallMethod::Scoop;
    }
    if exe_str.contains("/homebrew/") || exe_str.contains("/cellar/") {
        return InstallMethod::Homebrew;
    }

    // portable.txt が exe の隣にあればポータブルモード
    if exe
        .as_ref()
        .and_then(|p| p.parent().map(|d| d.join("portable.txt")))
        .map(|p| p.exists())
        .unwrap_or(false)
    {
        return InstallMethod::Portable;
    }

    InstallMethod::Standard
}

#[cfg(target_os = "windows")]
async fn run_pkg_manager_update(
    app: &tauri::AppHandle,
    program: &str,
    args: &[&str],
) -> Result<(), String> {
    run_pkg_manager_update_env(app, program, args, &[]).await
}

async fn run_pkg_manager_update_env(
    app: &tauri::AppHandle,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to run {program}: {e}"))?;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app.emit("update-log", serde_json::json!({ "line": line }));
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("{program} exited with status {status}"));
    }
    Ok(())
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    match detect_install_method() {
        InstallMethod::Portable => install_update_portable(&app).await,

        InstallMethod::Scoop => {
            #[cfg(target_os = "windows")]
            {
                run_pkg_manager_update(
                    &app,
                    "powershell",
                    &["-NoProfile", "-Command", "scoop update shun"],
                )
                .await
            }
            #[cfg(not(target_os = "windows"))]
            {
                Ok(())
            }
        }

        InstallMethod::Homebrew => {
            run_pkg_manager_update_env(
                &app,
                "brew",
                &["upgrade", "--cask", "shun"],
                &[
                    ("HOMEBREW_NO_AUTO_UPDATE", "1"),
                    ("HOMEBREW_NO_INTERACTIVE", "1"),
                ],
            )
            .await
        }

        InstallMethod::Standard => {
            let updater = app.updater().map_err(|e| e.to_string())?;
            if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
                let app_prog = app.clone();
                let downloaded = Arc::new(AtomicU64::new(0));
                let downloaded_c = Arc::clone(&downloaded);
                update
                    .download_and_install(
                        move |chunk, total| {
                            let d = downloaded_c.fetch_add(chunk as u64, Ordering::SeqCst)
                                + chunk as u64;
                            let _ = app_prog.emit(
                                "update-progress",
                                serde_json::json!({ "downloaded": d, "total": total }),
                            );
                        },
                        || {},
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                app.restart();
            }
            Ok(())
        }
    }
}

async fn install_update_portable(app: &tauri::AppHandle) -> Result<(), String> {
    // まずバージョン確認 — 最新版なら何もしない
    let updater = app.updater().map_err(|e| e.to_string())?;
    if updater.check().await.map_err(|e| e.to_string())?.is_none() {
        return Ok(());
    }

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = current_exe
        .parent()
        .ok_or("cannot find exe dir")?
        .to_path_buf();

    // GitHub の latest release から portable zip をストリーミングダウンロード
    let client = reqwest::Client::builder()
        .user_agent("shun-updater")
        .build()
        .map_err(|e| e.to_string())?;

    let zip_url = "https://github.com/yukimemi/shun/releases/latest/download/shun-windows-x64.zip";
    let response = client
        .get(zip_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let total = response.content_length();
    let mut downloaded: u64 = 0;
    let mut buf = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        buf.extend_from_slice(&chunk);
        let _ = app.emit(
            "update-progress",
            serde_json::json!({ "downloaded": downloaded, "total": total }),
        );
    }
    // zip から shun.exe を取り出す
    let cursor = std::io::Cursor::new(buf);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let exe_index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .map(|f| f.name().ends_with("shun.exe"))
                .unwrap_or(false)
        })
        .ok_or("shun.exe not found in zip")?;

    let new_exe_path = exe_dir.join("shun_update.exe");
    {
        let mut zip_file = archive.by_index(exe_index).map_err(|e| e.to_string())?;
        let mut out = std::fs::File::create(&new_exe_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut zip_file, &mut out).map_err(|e| e.to_string())?;
    }

    // 旧 exe をリネーム（Windows は実行中でもリネーム可）→ 新 exe を配置
    let old_exe_path = exe_dir.join("shun_old.exe");
    let _ = std::fs::remove_file(&old_exe_path); // 前回残留をクリーンアップ
    std::fs::rename(&current_exe, &old_exe_path).map_err(|e| e.to_string())?;
    std::fs::rename(&new_exe_path, &current_exe).map_err(|e| e.to_string())?;

    // 新 exe を起動して自分は終了
    std::process::Command::new(&current_exe)
        .spawn()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn list_config_files() -> Vec<String> {
    let mut names = vec!["config.toml".to_string()];
    for path in config::extra_config_files() {
        if let Some(name) = path.file_name() {
            names.push(name.to_string_lossy().to_string());
        }
    }
    names
}

#[tauri::command]
fn delete_config_file(name: String) -> Result<(), String> {
    if name == "config.toml" {
        return Err("config.toml cannot be deleted".to_string());
    }
    let p = config::config_dir().join(&name);
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_config(name: Option<String>) -> Result<(), String> {
    let path = match name.as_deref() {
        None | Some("config.toml") => config::config_path(),
        Some(n) => {
            let p = config::config_dir().join(n);
            // 存在しなければ空で作成
            if !p.exists() {
                std::fs::write(&p, "").map_err(|e| e.to_string())?;
            }
            p
        }
    };
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_history(_app: tauri::AppHandle) -> Result<(), String> {
    let path = history::history_path();
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_history_item(key: String) -> Result<(), String> {
    history::delete(&key).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_to_local(
    window: tauri::WebviewWindow,
    key: String,
    value: String,
) -> Result<String, String> {
    use toml_edit::DocumentMut;
    let path = config::local_config_path();
    let content = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut doc = content
        .parse::<DocumentMut>()
        .unwrap_or_else(|_| DocumentMut::new());

    let display = match key.as_str() {
        "search_mode" => {
            doc["search_mode"] = toml_edit::value(value.clone());
            format!("search_mode = {:?}", value)
        }
        "sort_order" => {
            doc["sort_order"] = toml_edit::value(value.clone());
            format!("sort_order = {:?}", value)
        }
        "theme" => {
            if !doc.contains_key("theme") {
                doc["theme"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            doc["theme"]["preset"] = toml_edit::value(value.clone());
            format!("theme.preset = {:?}", value)
        }
        "monitor" => {
            let cursor = window.cursor_position().map_err(|e| e.to_string())?;
            let monitors = window.available_monitors().map_err(|e| e.to_string())?;
            let index = monitors
                .iter()
                .position(|m| {
                    let pos = m.position();
                    let size = m.size();
                    cursor.x >= pos.x as f64
                        && cursor.x < (pos.x + size.width as i32) as f64
                        && cursor.y >= pos.y as f64
                        && cursor.y < (pos.y + size.height as i32) as f64
                })
                .unwrap_or(0) as i64;
            doc["monitor"] = toml_edit::value(index);
            format!("monitor = {}", index)
        }
        _ => return Err(format!("unknown setting: {}", key)),
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, doc.to_string()).map_err(|e| e.to_string())?;

    Ok(format!("{} saved to config.local.toml", display))
}

#[tauri::command]
fn read_preview(path: String, max_lines: usize) -> String {
    use std::io::BufRead;
    let expanded = crate::utils::expand_path(&path);
    let Ok(file) = std::fs::File::open(&expanded) else {
        return String::new();
    };
    let reader = std::io::BufReader::new(file);
    let mut lines = Vec::new();
    for line in reader.lines().take(max_lines) {
        match line {
            Ok(l) => lines.push(l),
            Err(_) => return String::new(), // バイナリファイルは空を返す
        }
    }
    lines.join("\n")
}

#[tauri::command]
fn adjust_for_preview(window: tauri::WebviewWindow, show: bool, preview_width: u32) {
    let cfg = config::load_config().0;
    let total_width = if show {
        cfg.window_width as f64 + preview_width as f64
    } else {
        cfg.window_width as f64
    };
    center_on_monitor(&window, &cfg.monitor, total_width);
}

fn center_on_monitor(window: &tauri::WebviewWindow, target: &config::MonitorTarget, win_w: f64) {
    let monitors = match window.available_monitors() {
        Ok(m) => m,
        Err(_) => return,
    };
    if monitors.is_empty() {
        return;
    }

    let monitor = match target {
        config::MonitorTarget::Named(s) if s == "primary" => window
            .primary_monitor()
            .ok()
            .flatten()
            .or_else(|| monitors.into_iter().next()),
        config::MonitorTarget::Index(i) => monitors.into_iter().nth(*i),
        _ => {
            // "cursor" (デフォルト): カーソルのあるモニター
            let cursor = match window.cursor_position() {
                Ok(p) => p,
                Err(_) => return,
            };
            monitors.into_iter().find(|m| {
                let pos = m.position();
                let size = m.size();
                cursor.x >= pos.x as f64
                    && cursor.x < (pos.x + size.width as i32) as f64
                    && cursor.y >= pos.y as f64
                    && cursor.y < (pos.y + size.height as i32) as f64
            })
        }
    };

    let monitor = match monitor {
        Some(m) => m,
        None => return,
    };

    let scale = monitor.scale_factor();
    let pos = monitor.position();
    let size = monitor.size();

    let mon_x = pos.x as f64 / scale;
    let mon_y = pos.y as f64 / scale;
    let mon_w = size.width as f64 / scale;
    let mon_h = size.height as f64 / scale;

    let x = mon_x + (mon_w - win_w) / 2.0;
    let y = mon_y + mon_h * 0.25;

    window.set_position(tauri::LogicalPosition::new(x, y)).ok();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
type WarningsState = Arc<Mutex<Vec<(String, String)>>>;

pub fn run() {
    let (config, _) = config::load_config();
    // WarningsState はランタイムエラー（keybinding 登録失敗など）のみ保持
    // config parse エラーは get_config_warnings() で毎回新鮮に取得する
    let warnings_state: WarningsState = Arc::new(Mutex::new(Vec::new()));

    // 起動時にキャッシュを初期構築（バックグラウンド）
    let cache: CacheState = Arc::new(Mutex::new(None));
    refresh_cache_bg(Arc::clone(&cache));

    let log_cfg = config.log.clone();
    let log_level = log_cfg.to_level_filter();
    let log_rotation = log_cfg.to_rotation_strategy();
    let log_max_size = log_cfg.max_file_size_kb * 1024;

    tauri::Builder::default()
        .manage(cache)
        .manage(warnings_state)
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log_level)
                .rotation_strategy(log_rotation)
                .max_file_size(log_max_size.into())
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: None,
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 2個目の起動を検知 → 既存ウィンドウを前面に表示
            if let Some(window) = app.get_webview_window("main") {
                window.show().ok();
                window.set_focus().ok();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let window = app.get_webview_window("main").unwrap();
            window.hide().ok();

            // Disable WebView2 browser accelerator keys (Ctrl+S, Ctrl+P, etc.)
            // so all key combinations reach JavaScript keybinding handlers.
            #[cfg(target_os = "windows")]
            window
                .with_webview(|webview| {
                    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
                    use windows_core::Interface;
                    let settings = unsafe {
                        webview
                            .controller()
                            .CoreWebView2()
                            .unwrap()
                            .Settings()
                            .unwrap()
                    };
                    if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
                        unsafe { s3.SetAreBrowserAcceleratorKeysEnabled(false).ok() };
                    }
                })
                .ok();

            let cache = Arc::clone(app.state::<CacheState>().inner());

            // hide_on_blur: フォーカスが外れたら自動非表示（設定は毎回 load_config で確認）
            {
                let window_blur = window.clone();
                let cache_blur = Arc::clone(&cache);
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        if config::load_config().0.hide_on_blur {
                            window_blur.hide().ok();
                            refresh_cache_bg(Arc::clone(&cache_blur));
                        }
                    }
                });
            }

            // portable 更新後の残留ファイルをクリーンアップ
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let _ = std::fs::remove_file(dir.join("shun_old.exe"));
                    let _ = std::fs::remove_file(dir.join("shun_update.exe"));
                }
            }

            // バックグラウンドでアップデートチェック（設定した間隔で定期実行）
            let interval = config.update_check_interval;
            let app_for_update = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if interval == 0 {
                    return;
                }
                loop {
                    if should_check_update(interval) {
                        log::info!("update check triggered (interval={interval}s)");
                        match app_for_update.updater() {
                            Err(e) => log::warn!("update check: failed to get updater: {e}"),
                            Ok(updater) => match updater.check().await {
                                Err(e) => log::warn!("update check: check() failed: {e}"),
                                Ok(None) => {
                                    log::info!("update check: no update available");
                                    record_update_check();
                                }
                                Ok(Some(update)) => {
                                    log::info!("update check: new version found: {}", update.version);
                                    record_update_check();
                                    let _ = app_for_update
                                        .emit("update-available", update.version.clone());
                                }
                            },
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });

            // システムトレイ
            let tray_menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "show", "Show shun", true, None::<&str>)?,
                    &MenuItem::with_id(app, "config", "Config", true, None::<&str>)?,
                    &MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .tooltip("shun")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let cfg = config::load_config().0;
                            center_on_monitor(&win, &cfg.monitor, cfg.window_width as f64);
                            win.show().ok();
                            win.set_focus().ok();
                            win.emit("show-launcher", ()).ok();
                        }
                    }
                    "config" => {
                        let path = config::config_path();
                        tauri_plugin_opener::open_path(path, None::<&str>).ok();
                    }
                    "exit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // launch key 警告は get_config_and_warnings() で動的生成するので WarningsState は不要
            // 登録が完全失敗した場合は setup error として伝播する
            if let Err(e) = register_launch_shortcut(app.handle()) {
                log::warn!("Launch shortcut registration failed: {e}. App will start without a global shortcut.");
            }

            // Config にエラーがある場合は起動時にウィンドウを表示して警告を見せる
            // （launch shortcut が変わっていてウィンドウを開けなくなる問題を防ぐ）
            let (config_at_start, startup_warnings) = config::load_config();
            let launch_is_invalid = config_at_start
                .keybindings
                .launch
                .parse::<Shortcut>()
                .is_err();
            let has_warnings = !startup_warnings.is_empty() || launch_is_invalid;
            if has_warnings {
                log::warn!("Config has errors at startup — showing window immediately");
                let window_warn = window.clone();
                // フロントエンドの初期化完了を待ってから表示
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let cfg = config::load_config().0;
                    center_on_monitor(&window_warn, &cfg.monitor, cfg.window_width as f64);
                    window_warn.show().ok();
                    window_warn.set_focus().ok();
                    window_warn.emit("show-launcher", ()).ok();
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config_and_warnings,
            get_apps,
            search_items,
            launch_item,
            complete_path,
            exit_app,
            list_config_files,
            open_config,
            delete_config_file,
            open_history,
            delete_history_item,
            save_to_local,
            reload,
            get_last_args,
            get_args_history,
            install_update,
            read_preview,
            adjust_for_preview
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
