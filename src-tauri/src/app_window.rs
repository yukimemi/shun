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
///
/// `window` はウィンドウ照合条件 (`[[apps]].window_app` / `window_title` ...)。
/// `window_app` は全 OS 共通、タイトル条件は Windows のみ。
pub fn activate_or_launch(
    item: &LaunchItem,
    window: WindowMatch,
    toggle: bool,
    vars: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    match try_activate(item, window, toggle) {
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

/// ウィンドウ照合条件（config から渡される）。`title` / `title_exclude` は Windows のみ参照する。
#[derive(Clone, Copy)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct WindowMatch<'a> {
    /// 照合するアプリ/プロセス名 (`[[apps]].window_app`)。`None` なら `item.path` の file stem。
    pub app: Option<&'a str>,
    /// タイトルに含まれるべき文字列（大文字小文字無視）。`None` なら条件なし。
    pub title: Option<&'a str>,
    /// タイトルにこの文字列を含むウィンドウは除外する（大文字小文字無視）。
    pub title_exclude: Option<&'a str>,
}

/// activate / toggle の照合対象アプリ名を全 OS 共通の規則で解決する。
///
/// 1. trim 後に空でない `window_app` があればそれ。
/// 2. なければ `path` の file stem（`name` には依存しない）。
///
/// 末尾の `.exe` / `.app` は大文字小文字を無視して除去する（大文字小文字自体は保持）。
/// 解決結果が空なら `None`。
pub fn resolve_window_app(window_app: Option<&str>, path: &str) -> Option<String> {
    let base = match window_app.map(str::trim).filter(|s| !s.is_empty()) {
        Some(a) => a.to_string(),
        None => std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim()
            .to_string(),
    };
    let lower = base.to_ascii_lowercase();
    let cut = if lower.ends_with(".exe") || lower.ends_with(".app") {
        base.len() - 4
    } else {
        base.len()
    };
    let name = base[..cut].trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn try_activate(
    item: &LaunchItem,
    window: WindowMatch,
    toggle: bool,
) -> Result<ActivateOutcome, String> {
    let Some(app) = resolve_window_app(window.app, &item.path) else {
        return Ok(ActivateOutcome::NotFound);
    };
    #[cfg(target_os = "windows")]
    return windows_impl::activate(&app, window, toggle);
    #[cfg(target_os = "macos")]
    return macos_impl::activate(&app, toggle);
    #[cfg(target_os = "linux")]
    return linux_impl::activate(&app, toggle);
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn try_activate(
    _item: &LaunchItem,
    _window: WindowMatch,
    _toggle: bool,
) -> Result<ActivateOutcome, String> {
    Ok(ActivateOutcome::Unsupported)
}

/// Windows: `EnumWindows` で対象プロセスの通常ウィンドウを1つ探し、
/// `SetForegroundWindow` / `ShowWindow` で操作する。
///
/// マッチは解決済みアプリ名（`window_app` または `path` の file stem）を
/// 大文字小文字無視で実行ファイル名と比較する。
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
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_MINIMIZE, SW_RESTORE,
        WS_EX_TOOLWINDOW,
    };

    /// 探す対象ウィンドウの条件。exe stem 一致 AND タイトル部分一致（指定時）
    /// AND タイトル除外文字列を含まない（指定時）。
    struct Target {
        stem: String,
        /// 小文字化済み
        title: Option<String>,
        /// 小文字化済み
        title_exclude: Option<String>,
    }

    impl Target {
        unsafe fn matches(&self, hwnd: HWND) -> bool {
            if window_exe_stem(hwnd).as_deref() != Some(self.stem.as_str()) {
                return false;
            }
            if self.title.is_none() && self.title_exclude.is_none() {
                return true;
            }
            let title = window_title(hwnd).to_lowercase();
            self.title
                .as_ref()
                .is_none_or(|t| title.contains(t.as_str()))
                && self
                    .title_exclude
                    .as_ref()
                    .is_none_or(|t| !title.contains(t.as_str()))
        }
    }

    struct SearchState {
        target: Target,
        found: Option<isize>,
    }

    pub(super) fn activate(
        app: &str,
        window: super::WindowMatch,
        toggle: bool,
    ) -> Result<ActivateOutcome, String> {
        // app は resolve_window_app() で解決済み（.exe 除去済み）。比較用に小文字化する。
        let stem = app.to_lowercase();
        let normalize = |s: Option<&str>| s.filter(|t| !t.is_empty()).map(str::to_lowercase);
        let target = Target {
            stem,
            title: normalize(window.title),
            title_exclude: normalize(window.title_exclude),
        };

        unsafe {
            // toggle: まずフォアグラウンドウィンドウ自体が対象か確認する。
            // 対象プロセスが複数ウィンドウを持つ場合、EnumWindows が最初に見つける
            // ウィンドウとフォアグラウンドウィンドウが別物なことがあるため、
            // 「今アクティブな対象ウィンドウ」は独立して判定する必要がある。
            if toggle {
                let foreground = GetForegroundWindow();
                if !foreground.is_invalid() && target.matches(foreground) {
                    let _ = ShowWindow(foreground, SW_MINIMIZE);
                    return Ok(ActivateOutcome::Minimized);
                }
            }
        }

        let mut state = SearchState {
            target,
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
            if !force_foreground(hwnd) {
                log::warn!("app_window(windows): SetForegroundWindow failed (foreground lock?)");
            }
        }
        Ok(ActivateOutcome::Activated)
    }

    /// フォアグラウンドロックを回避して `hwnd` を前面に出す。
    ///
    /// `RegisterHotKey` 経由の呼び出しは OS から前面化の権利を与えられるが、低レベル
    /// キーボードフック (kbhook) 経由では与えられず `SetForegroundWindow` が拒否される。
    /// 現在の前面スレッドに入力キューを一時的にアタッチすると前面化が許可される。
    unsafe fn force_foreground(hwnd: HWND) -> bool {
        if SetForegroundWindow(hwnd).as_bool() {
            return true;
        }
        let fg_tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let cur_tid = GetCurrentThreadId();
        let attached =
            fg_tid != 0 && fg_tid != cur_tid && AttachThreadInput(cur_tid, fg_tid, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let ok = SetForegroundWindow(hwnd).as_bool();
        if attached {
            let _ = AttachThreadInput(cur_tid, fg_tid, false);
        }
        ok
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

    unsafe fn window_title(hwnd: HWND) -> String {
        let cap = GetWindowTextLengthW(hwnd).max(0) as usize + 1;
        let mut buf = vec![0u16; cap];
        let len = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..len])
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

        if state.target.matches(hwnd) {
            state.found = Some(hwnd.0 as isize);
            return false.into();
        }
        true.into()
    }
}

/// macOS: `NSWorkspace` / `NSRunningApplication` (AppKit) を直接呼んで起動中判定・前面化・
/// 「トグルで隠す」を行う。以前は `osascript` 経由で AppleScript を実行していたが、
/// サブプロセス起動 + AppleScript コンパイルのオーバーヘッドがあり、かつ `toggle` の
/// 前面判定・最小化 (`AXMinimized`) には System Events 経由のUI操作が必要で
/// Accessibility 権限が要る上、権限が無いと（ダイアログすら出ずに）黙って失敗し、
/// `activate_or_launch()` が毎回新規起動にフォールバックし続けるという実害が
/// 実機で発生した (2026-09-27)。
///
/// `NSRunningApplication` の `activateWithOptions` / `isActive` / `hide` はどれも
/// Accessibility 権限を必要としない通常の AppKit API なので、代わりにこちらを使う。
/// `toggle` で「フロントなら引っ込める」動作は、ウィンドウ単位の `AXMinimized`（各
/// ウィンドウを個別に最小化）ではなく、アプリ単位の `hide`（Cmd+H 相当、Dock/Cmd+Tab
/// から見えなくなる）に変わる — 複数ウィンドウがあっても一括で退避できる点は同じだが、
/// 個別ウィンドウの最小化状態としては Dock に残らない、という挙動差がある。
///
/// アプリ名は解決済みの `window_app`（または `path` の stem）で、`NSRunningApplication`
/// の `localizedName`（Launch Services 上の表示名、`.app` のベース名相当）と大文字小文字
/// 無視で比較する。見つからなければ `NotFound` を返し、呼び出し側に設定済み `path` を
/// 起動させる。
#[cfg(target_os = "macos")]
mod macos_impl {
    use super::ActivateOutcome;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

    /// アプリが見つかった以上、`hide`/`activateWithOptions` が (稀に) `false` を返しても
    /// `Err` にはしない — `activate_or_launch()` の `Err` 分岐は「起動にフォールバック」
    /// するため、既に起動済みと分かっているアプリに対してそれをやると新規プロセスが
    /// 重複起動してしまう（今回まさに追いかけていた不具合と同じ結果になる）。
    /// 見つからなかった場合のみ `NotFound` を返し、その場合の起動フォールバックは
    /// 正当（本当に未起動なので新規起動が正しい）。
    pub(super) fn activate(app_name: &str, toggle: bool) -> Result<ActivateOutcome, String> {
        let Some(app) = find_running(app_name) else {
            return Ok(ActivateOutcome::NotFound);
        };

        if toggle && app.isActive() {
            if !app.hide() {
                log::warn!("macos_impl: NSRunningApplication::hide() failed for \"{app_name}\"");
            }
            return Ok(ActivateOutcome::Minimized);
        }

        if !app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) {
            log::warn!(
                "macos_impl: NSRunningApplication::activateWithOptions() failed for \"{app_name}\""
            );
        }
        Ok(ActivateOutcome::Activated)
    }

    fn find_running(app_name: &str) -> Option<Retained<NSRunningApplication>> {
        let running = NSWorkspace::sharedWorkspace().runningApplications();
        (0..running.count())
            .map(|i| running.objectAtIndex(i))
            .find(|app| {
                app.localizedName()
                    .is_some_and(|name| name.to_string().eq_ignore_ascii_case(app_name))
            })
    }
}

/// Linux: `wmctrl` があれば `-x -a <app>` でアクティブ化する。`-x` により WM_CLASS
/// (`instance.class`) と部分一致・大文字小文字無視で照合する（タイトルではない）。
/// 部分一致のため短い名前は別アプリに誤マッチし得る。`wmctrl` が無い、または X11 以外
/// （Wayland など）で動作しない環境では `Unsupported` を返し、呼び出し側が起動にフォールバックする。
///
/// 制約: `wmctrl` には「現在フォーカスされているウィンドウか」を安定して判定する手段が
/// ない（追加で `xdotool` 等を要求すると依存が増える）。そのため `toggle` は `activate` と
/// 同じ動作（フォーカスするだけで最小化はしない）にフォールバックする。
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::ActivateOutcome;

    pub(super) fn activate(app_name: &str, _toggle: bool) -> Result<ActivateOutcome, String> {
        let output = match std::process::Command::new("wmctrl")
            .args(["-x", "-a", app_name])
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

#[cfg(test)]
mod tests {
    use super::resolve_window_app as r;

    #[test]
    fn explicit_app_wins() {
        assert_eq!(r(Some("Code"), "/usr/bin/foo").as_deref(), Some("Code"));
    }

    #[test]
    fn falls_back_to_path_stem() {
        assert_eq!(r(None, "C:/tools/todoke.exe").as_deref(), Some("todoke"));
        assert_eq!(r(None, "todoke").as_deref(), Some("todoke"));
    }

    #[test]
    fn blank_app_falls_back() {
        assert_eq!(r(Some("  "), "/bin/todoke").as_deref(), Some("todoke"));
        assert_eq!(r(Some(""), "/bin/todoke").as_deref(), Some("todoke"));
    }

    #[test]
    fn strips_exe_and_app_preserving_case() {
        assert_eq!(
            r(Some("WindowsTerminal.EXE"), "x").as_deref(),
            Some("WindowsTerminal")
        );
        assert_eq!(
            r(Some("Visual Studio Code.app"), "x").as_deref(),
            Some("Visual Studio Code")
        );
        assert_eq!(r(Some("Foo.Bar"), "x").as_deref(), Some("Foo.Bar"));
    }

    #[test]
    fn empty_resolves_to_none() {
        assert_eq!(r(None, ""), None);
        assert_eq!(r(Some(".exe"), ""), None);
    }
}
