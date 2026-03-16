use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

mod apps;
mod complete;
mod config;
mod history;
mod search;
mod utils;

#[tauri::command]
fn get_config() -> config::Config {
    config::load_config()
}

#[tauri::command]
fn get_apps() -> Vec<apps::LaunchItem> {
    let config = config::load_config();
    let hist = history::load();
    let mut items = apps::collect_items(&config);

    items.sort_by(|a, b| {
        let (ac, at) = history::sort_key(&hist, &a.path);
        let (bc, bt) = history::sort_key(&hist, &b.path);
        if ac == 0 && bc == 0 {
            return a.name.to_lowercase().cmp(&b.name.to_lowercase());
        }
        match config.sort_order {
            config::SortOrder::CountFirst  => bc.cmp(&ac).then(bt.cmp(&at)),
            config::SortOrder::RecentFirst => bt.cmp(&at).then(bc.cmp(&ac)),
        }
    });

    items
}

#[tauri::command]
fn search_items(query: String) -> Vec<apps::LaunchItem> {
    let config = config::load_config();
    let hist = history::load();
    let mut items = apps::collect_items(&config);

    // 履歴ソート
    items.sort_by(|a, b| {
        let (ac, at) = history::sort_key(&hist, &a.path);
        let (bc, bt) = history::sort_key(&hist, &b.path);
        if ac == 0 && bc == 0 {
            return a.name.to_lowercase().cmp(&b.name.to_lowercase());
        }
        match config.sort_order {
            config::SortOrder::CountFirst  => bc.cmp(&ac).then(bt.cmp(&at)),
            config::SortOrder::RecentFirst => bt.cmp(&at).then(bc.cmp(&ac)),
        }
    });

    if query.is_empty() {
        return items;
    }

    search::filter(&items, &query, &config.search_mode)
}

#[derive(serde::Serialize)]
struct CompleteResult {
    prefix: String,
    completions: Vec<String>,
}

#[tauri::command]
fn complete_path(
    input: String,
    completion_type: config::CompletionType,
    completion_list: Vec<String>,
    completion_command: Option<String>,
    workdir: Option<String>,
) -> CompleteResult {
    let (prefix, completions) =
        complete::complete(&input, &completion_type, &completion_list, &completion_command, &workdir);
    CompleteResult { prefix, completions }
}

#[tauri::command]
fn launch_item(item: apps::LaunchItem, extra_args: Option<Vec<String>>) -> Result<(), String> {
    history::record(&item.path);
    apps::launch_with_extra(&item, extra_args.unwrap_or_default())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn open_config(_app: tauri::AppHandle) -> Result<(), String> {
    let path = config::config_path();
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = config::load_config();
    let launch_shortcut = config.keybindings.launch.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let window = app.get_webview_window("main").unwrap();
            window.hide().ok();

            let shortcut: Shortcut = launch_shortcut.parse().expect("invalid shortcut");
            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    if window.is_visible().unwrap_or(false) {
                        window.hide().ok();
                    } else {
                        window.show().ok();
                        window.set_focus().ok();
                        window.emit("show-launcher", ()).ok();
                    }
                }
            })?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_config, get_apps, search_items, launch_item, complete_path, exit_app, open_config])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
