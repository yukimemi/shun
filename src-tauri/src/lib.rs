use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use log::{debug, info};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;
use tokio::io::AsyncBufReadExt;

mod apps;
mod complete;
mod config;
mod history;
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
    let config = config::load_config();
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

#[tauri::command]
fn get_config() -> config::Config {
    config::load_config()
}

#[tauri::command]
fn get_apps() -> Vec<apps::LaunchItem> {
    let config = config::load_config();
    let hist = history::load();
    let mut items = apps::collect_items(&config);
    sort_items(&mut items, &hist, &config);
    items
}

#[tauri::command]
fn search_items(query: String, state: tauri::State<CacheState>) -> Vec<apps::LaunchItem> {
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

    // history は軽いので毎回ロード（起動直後も正確な順序に）
    let hist = history::load();
    sort_items(&mut items, &hist, &config);

    if query.is_empty() {
        return items;
    }
    search::filter(&items, &query, &config.search_mode)
}

fn sort_items(items: &mut [apps::LaunchItem], hist: &history::History, config: &config::Config) {
    items.sort_by(|a, b| {
        let a_key = a.history_key.as_deref().unwrap_or(&a.path);
        let b_key = b.history_key.as_deref().unwrap_or(&b.path);
        let (ac, at) = history::sort_key(hist, a_key);
        let (bc, bt) = history::sort_key(hist, b_key);
        if ac == 0 && bc == 0 {
            return a.name.to_lowercase().cmp(&b.name.to_lowercase());
        }
        match config.sort_order {
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
    let ctx = apps::build_template_context(&[], vars);
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
            let ctx = apps::build_template_context(&extra, &vars);
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
        let ctx = apps::build_template_context(&template_args, &vars);
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
    let config = config::load_config();
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

#[tauri::command]
fn rescan(state: tauri::State<CacheState>) {
    refresh_cache_bg(Arc::clone(state.inner()));
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
    elapsed.as_secs() > interval_secs
}

fn record_update_check() {
    let _ = std::fs::write(last_update_check_path(), "");
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

    // portable.txt が exe の隣にあればポータブルモード
    if exe
        .as_ref()
        .and_then(|p| p.parent().map(|d| d.join("portable.txt")))
        .map(|p| p.exists())
        .unwrap_or(false)
    {
        return InstallMethod::Portable;
    }

    let exe_str = exe
        .as_ref()
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if exe_str.contains("\\scoop\\apps\\") {
        InstallMethod::Scoop
    } else if exe_str.contains("/homebrew/") || exe_str.contains("/cellar/") {
        InstallMethod::Homebrew
    } else {
        InstallMethod::Standard
    }
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
fn open_config(_app: tauri::AppHandle) -> Result<(), String> {
    let path = config::config_path();
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
fn set_theme_preset(preset: String) -> Result<(), String> {
    info!("set_theme_preset: preset={preset}");
    let local_path = config::local_config_path();

    // 既存の config.local.toml を読み込み（なければ空）— toml_edit でコメント・書式を保持
    let content = if local_path.exists() {
        std::fs::read_to_string(&local_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut doc: toml_edit::DocumentMut = content.parse().unwrap_or_default();

    // [theme] テーブルがなければ作成し、preset キーだけ更新
    if !doc.contains_key("theme") {
        doc["theme"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["theme"]["preset"] = toml_edit::value(&preset);

    if let Some(parent) = local_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&local_path, doc.to_string()).map_err(|e| e.to_string())?;
    info!("set_theme_preset: saved to {:?}", local_path);

    // テーマ変更はアプリ一覧キャッシュに影響しないため refresh_cache_bg は不要
    Ok(())
}

fn center_on_cursor_monitor(window: &tauri::WebviewWindow) {
    let cursor = match window.cursor_position() {
        Ok(p) => p,
        Err(_) => return,
    };
    let monitors = match window.available_monitors() {
        Ok(m) => m,
        Err(_) => return,
    };
    let monitor = monitors.iter().find(|m| {
        let pos = m.position();
        let size = m.size();
        cursor.x >= pos.x as f64
            && cursor.x < (pos.x + size.width as i32) as f64
            && cursor.y >= pos.y as f64
            && cursor.y < (pos.y + size.height as i32) as f64
    });
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

    let win_w = 620.0_f64;
    let x = mon_x + (mon_w - win_w) / 2.0;
    let y = mon_y + mon_h * 0.25;

    window.set_position(tauri::LogicalPosition::new(x, y)).ok();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = config::load_config();
    let launch_shortcut = config.keybindings.launch.clone();

    // 起動時にキャッシュを初期構築（バックグラウンド）
    let cache: CacheState = Arc::new(Mutex::new(None));
    refresh_cache_bg(Arc::clone(&cache));

    let log_cfg = config::load_config().log;
    let log_level = log_cfg.to_level_filter();
    let log_rotation = log_cfg.to_rotation_strategy();
    let log_max_size = log_cfg.max_file_size_kb * 1024;

    tauri::Builder::default()
        .manage(cache)
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log_level)
                .rotation_strategy(log_rotation)
                .max_file_size(log_max_size.into())
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let window = app.get_webview_window("main").unwrap();
            window.hide().ok();

            let cache = Arc::clone(app.state::<CacheState>().inner());

            // hide_on_blur: フォーカスが外れたら自動非表示
            if config.hide_on_blur {
                let window_blur = window.clone();
                let cache_blur = Arc::clone(&cache);
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        window_blur.hide().ok();
                        refresh_cache_bg(Arc::clone(&cache_blur));
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

            // バックグラウンドでアップデートチェック（設定した間隔で）
            let interval = config.update_check_interval;
            let app_for_update = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if !should_check_update(interval) {
                    return;
                }
                record_update_check();
                if let Ok(updater) = app_for_update.updater() {
                    if let Ok(Some(update)) = updater.check().await {
                        let _ = app_for_update.emit("update-available", update.version.clone());
                    }
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
                            center_on_cursor_monitor(&win);
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

            let shortcut: Shortcut = launch_shortcut.parse().expect("invalid shortcut");
            app.global_shortcut()
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if window.is_visible().unwrap_or(false) {
                            debug!("shortcut: window visible → hide");
                            window.hide().ok();
                            // 非表示になったタイミングでキャッシュを更新（次回表示時に即座に使える）
                            refresh_cache_bg(Arc::clone(&cache));
                        } else {
                            debug!("shortcut: window hidden → show");
                            center_on_cursor_monitor(&window);
                            window.show().ok();
                            window.set_focus().ok();
                            window.emit("show-launcher", ()).ok();
                        }
                    }
                })?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_apps,
            search_items,
            launch_item,
            complete_path,
            exit_app,
            open_config,
            open_history,
            delete_history_item,
            set_theme_preset,
            rescan,
            get_last_args,
            get_args_history,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
