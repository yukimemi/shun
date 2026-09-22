//! 特定アプリのウィンドウをアクティブ化/トグルするための OS 抽象レイヤ。
//!
//! 公開契約はプラットフォームに関わらず `activate_or_launch()` 一つだけ。
//! ウィンドウが見つからない・OS が未対応・操作が失敗した場合は、いずれも
//! `apps::launch()` へフォールバックする（安全側に倒す）。呼び出し側はこの
//! フォールバックを意識する必要がなく、OS ごとの実装差はここに閉じ込める。

use crate::apps::{self, LaunchItem};

/// ウィンドウ検索・操作の結果。
/// `Unsupported` はビルド対象 OS によっては構築されない（未対応 OS 向けの
/// catch-all フォールバック実装でのみ使われる）ため dead_code を許容する。
#[allow(dead_code)]
enum ActivateOutcome {
    /// 対象ウィンドウをフォアグラウンドへ持ってきた（最小化からの復元も含む）。
    Activated,
    /// フォアグラウンドだった対象ウィンドウを最小化した（toggle 時のみ）。
    Minimized,
    /// 対象ウィンドウが見つからなかった → 呼び出し側で起動する。
    NotFound,
    /// この OS / 環境では未対応 → 呼び出し側で起動する。
    Unsupported,
}

/// `item` に対応するウィンドウをアクティブ化する。
///
/// - `toggle = false` (`hotkey_mode = "activate"`): 起動済みならフォアグラウンドへ、
///   未起動なら新規起動する。
/// - `toggle = true` (`hotkey_mode = "toggle"`): 対象ウィンドウが現在フォアグラウンドなら
///   最小化する。そうでなければ `toggle = false` と同じ（アクティブ化 or 起動）。
///
/// ウィンドウが見つからない・OS 未対応・操作が失敗したいずれの場合も `apps::launch_with_extra()`
/// にフォールバックする（`launch_with_extra` を使うのは `Launch` モードと同じく `path` / `args` /
/// `workdir` の `{{ vars.* }}` テンプレートを展開するため）。
pub fn activate_or_launch(
    item: &LaunchItem,
    toggle: bool,
    vars: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    match try_activate(item, toggle) {
        Ok(ActivateOutcome::Activated) | Ok(ActivateOutcome::Minimized) => Ok(()),
        Ok(ActivateOutcome::NotFound) | Ok(ActivateOutcome::Unsupported) => {
            apps::launch_with_extra(item, Vec::new(), vars)
        }
        Err(e) => {
            log::warn!("activate_or_launch: window operation failed ({e}), launching instead");
            apps::launch_with_extra(item, Vec::new(), vars)
        }
    }
}

#[cfg(target_os = "windows")]
fn try_activate(item: &LaunchItem, toggle: bool) -> Result<ActivateOutcome, String> {
    windows_impl::activate(&item.path, toggle)
}

#[cfg(target_os = "macos")]
fn try_activate(item: &LaunchItem, toggle: bool) -> Result<ActivateOutcome, String> {
    macos_impl::activate(&item.name, toggle)
}

#[cfg(target_os = "linux")]
fn try_activate(item: &LaunchItem, toggle: bool) -> Result<ActivateOutcome, String> {
    linux_impl::activate(&item.name, toggle)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn try_activate(_item: &LaunchItem, _toggle: bool) -> Result<ActivateOutcome, String> {
    Ok(ActivateOutcome::Unsupported)
}

/// Windows: `EnumWindows` で対象プロセスの通常ウィンドウを1つ探し、
/// `SetForegroundWindow` / `ShowWindow` で操作する。
///
/// マッチは `item.path` の file stem (拡張子・ディレクトリを除いた実行ファイル名) を
/// 大文字小文字無視で比較する。`path` が PATH 上のコマンド名・`.lnk` などのケースでも
/// 実行中プロセスの exe 名と比較できるようにするため。
///
/// 対象を絞るフィルタ: 可視ウィンドウのみ、オーナーウィンドウを持たない、
/// `WS_EX_TOOLWINDOW` を除外（通常のアプリウィンドウのみを対象にする）。
/// 同一実行ファイルが複数ウィンドウを持つ場合、`activate`（新規に前面化する）は
/// `EnumWindows` が最初に見つけたウィンドウを操作する。`toggle` はまずフォアグラウンド
/// ウィンドウ自体が対象プロセスのものか確認するため、複数ウィンドウのうちどれが
/// アクティブでも正しく最小化できる。
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::ActivateOutcome;
    use std::path::Path;
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER,
        SW_MINIMIZE, SW_RESTORE, WS_EX_TOOLWINDOW,
    };

    struct SearchState {
        target_stem: String,
        found: Option<isize>,
    }

    pub(super) fn activate(target_path: &str, toggle: bool) -> Result<ActivateOutcome, String> {
        let target_stem = exe_stem(target_path);
        if target_stem.is_empty() {
            return Ok(ActivateOutcome::NotFound);
        }

        unsafe {
            // toggle: まずフォアグラウンドウィンドウ自体が対象プロセスのものか確認する。
            // 対象プロセスが複数ウィンドウを持つ場合、EnumWindows が最初に見つける
            // ウィンドウとフォアグラウンドウィンドウが別物なことがあるため、
            // 「今アクティブな対象ウィンドウ」は独立して判定する必要がある。
            if toggle {
                let foreground = GetForegroundWindow();
                if !foreground.is_invalid()
                    && window_exe_stem(foreground).as_deref() == Some(target_stem.as_str())
                {
                    let _ = ShowWindow(foreground, SW_MINIMIZE);
                    return Ok(ActivateOutcome::Minimized);
                }
            }
        }

        let mut state = SearchState {
            target_stem,
            found: None,
        };
        unsafe {
            let _ = EnumWindows(
                Some(enum_proc),
                LPARAM(&mut state as *mut SearchState as isize),
            );
        }

        let Some(raw) = state.found else {
            return Ok(ActivateOutcome::NotFound);
        };
        let hwnd = HWND(raw as *mut std::ffi::c_void);

        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            // フォアグラウンドロックにより失敗することがある（その場合タスクバーの
            // 点滅で終わる）。OS 制約であり回避不能なので、警告に留めて成功扱いにする。
            if !SetForegroundWindow(hwnd).as_bool() {
                log::warn!("app_window(windows): SetForegroundWindow failed (foreground lock?)");
            }
        }
        Ok(ActivateOutcome::Activated)
    }

    fn exe_stem(path: &str) -> String {
        Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase()
    }

    /// `hwnd` を所有するプロセスの実行ファイル名 (file stem, 小文字) を返す。
    /// プロセスハンドルの取得やイメージ名取得に失敗した場合は `None`。
    unsafe fn window_exe_stem(hwnd: HWND) -> Option<String> {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        ok.ok()?;

        let exe_path = String::from_utf16_lossy(&buf[..size as usize]);
        Some(exe_stem(&exe_path))
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut SearchState);

        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
            if !owner.is_invalid() {
                return true.into();
            }
        }
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return true.into();
        }

        if window_exe_stem(hwnd).as_deref() == Some(state.target_stem.as_str()) {
            state.found = Some(hwnd.0 as isize);
            return false.into();
        }
        true.into()
    }
}

/// macOS: `osascript` 経由で AppleScript を実行し、起動/前面化/最小化を行う。
///
/// `tell application "<name>" to activate` は対象が未起動なら起動し、起動済みなら
/// 前面化する、という2つの動作を1コマンドで賄える。`item.name` をそのままアプリ名として
/// 渡すため、config の `[[apps]].name` が実際の macOS アプリ名（`.app` のベース名）と
/// 一致しない場合は動作しない（README 参照）。
///
/// 制約: ウィンドウの前面化・最小化操作にはアクセシビリティ権限が必要。権限が
/// 未許可の場合 `osascript` はエラーを返し、`Err` として呼び出し側に伝播し
/// `activate_or_launch()` が `apps::launch()` にフォールバックする。
#[cfg(target_os = "macos")]
mod macos_impl {
    use super::ActivateOutcome;

    pub(super) fn activate(app_name: &str, toggle: bool) -> Result<ActivateOutcome, String> {
        if app_name.is_empty() {
            return Ok(ActivateOutcome::NotFound);
        }
        let escaped = app_name.replace('\\', "\\\\").replace('"', "\\\"");

        if toggle {
            let frontmost = run_osascript(
                r#"tell application "System Events" to get name of first process whose frontmost is true"#,
            )?;
            if frontmost.trim().eq_ignore_ascii_case(&escaped) {
                run_osascript(&format!(
                    r#"tell application "{escaped}" to set miniaturized of every window to true"#
                ))?;
                return Ok(ActivateOutcome::Minimized);
            }
        }

        run_osascript(&format!(r#"tell application "{escaped}" to activate"#))?;
        Ok(ActivateOutcome::Activated)
    }

    fn run_osascript(script: &str) -> Result<String, String> {
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Linux: `wmctrl` があれば `-a <name>` でアクティブ化する（部分一致・大文字小文字無視で
/// ウィンドウタイトルとマッチする）。`wmctrl` が無い、または X11 以外（Wayland など）で
/// 動作しない環境では `Unsupported` を返し、呼び出し側が `apps::launch()` にフォールバックする。
///
/// 制約: `wmctrl` には「現在フォーカスされているウィンドウか」を安定して判定する手段が
/// ない（追加で `xdotool` 等を要求すると依存が増える）。そのため `toggle` は `activate` と
/// 同じ動作（フォーカスするだけで最小化はしない）にフォールバックする。
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::ActivateOutcome;

    pub(super) fn activate(app_name: &str, _toggle: bool) -> Result<ActivateOutcome, String> {
        if app_name.is_empty() {
            return Ok(ActivateOutcome::NotFound);
        }
        let output = match std::process::Command::new("wmctrl")
            .args(["-a", app_name])
            .output()
        {
            Ok(o) => o,
            Err(_) => return Ok(ActivateOutcome::Unsupported),
        };
        if output.status.success() {
            Ok(ActivateOutcome::Activated)
        } else {
            Ok(ActivateOutcome::NotFound)
        }
    }
}
