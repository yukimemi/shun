//! 特定アプリのウィンドウをアクティブ化/トグルするための OS 抽象レイヤ。
//!
//! 公開契約はプラットフォームに関わらず `activate_or_launch()` 一つだけ。
//! ウィンドウが見つからない・OS が未対応の場合は `apps::launch()` へフォールバックする
//! （安全側に倒す）。呼び出し側はこのフォールバックを意識する必要がなく、OS ごとの実装差は
//! ここに閉じ込める。例外: macOS は「見つかったが操作(`hide`/`activate`)自体が失敗した」場合は
//! フォールバックしない — 既に起動済みと分かっているアプリを起動フォールバックすると
//! 新規プロセスが重複してしまうため（`macos_impl` 参照）。

use crate::apps::{self, LaunchItem};

/// ウィンドウ検索・操作の結果。
/// `Unsupported` はビルド対象 OS によっては構築されない（未対応 OS 向けの
/// catch-all フォールバック実装でのみ使われる）ため dead_code を許容する。
#[allow(dead_code)]
enum ActivateOutcome {
    /// 対象アプリをフォアグラウンドへ持ってきた（最小化からの復元も含む）。
    Activated,
    /// フォアグラウンドだった対象アプリを引っ込めた（toggle 時のみ）。Windows/Linux は
    /// ウィンドウ単位の最小化、macOS はアプリ単位の `hide`（Cmd+H 相当）— `macos_impl` 参照。
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
/// - `toggle = true` (`hotkey_mode = "toggle"`): 対象アプリが現在フォアグラウンドなら
///   引っ込める（Windows/Linux はウィンドウ最小化、macOS は `hide`）。そうでなければ
///   `toggle = false` と同じ（アクティブ化 or 起動）。
///
/// ウィンドウが見つからない・OS 未対応のいずれの場合も `apps::launch_with_extra()` に
/// フォールバックする（`launch_with_extra` を使うのは `Launch` モードと同じく `path` / `args` /
/// `workdir` の `{{ vars.* }}` テンプレートを展開するため）。macOS は例外として、見つかった
/// アプリに対する操作自体の失敗ではフォールバックしない（`macos_impl` のドキュメント参照）。
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

/// macOS: `NSWorkspace` / `NSRunningApplication` (AppKit) で起動中判定・`isActive` 判定・
/// `hide`/`unhide` を行うが、実際に前面へ持ってくる操作だけは `open` コマンド（サブプロセス）
/// に任せる。
///
/// 経緯: 当初は前面化も `NSRunningApplication.activateWithOptions()` で行っていたが、
/// 実機で「見つかった・起動中・非表示でもない」アプリに対してすら常に `false` を返し
/// 何も起きない不具合が発生した (2026-09-27)。メインスレッド/バックグラウンドスレッドの
/// どちらから呼んでも・Accessibility 権限や「App管理」権限を与えても変化なし
/// （切り分け用の使い捨てバイナリで確認済み: 素の CLI プロセスからは同じ呼び出しが
/// バックグラウンドスレッドからでも成功する — つまりスレッド一般の問題ではなく、
/// shun という実行コンテキスト固有の何かが `activateWithOptions` を黙って拒否している）。
/// 一方、shun 自身のランチャーUIで同じアプリ（`System` ソースの候補、`apps::launch()` が
/// `.app` アイテムに既に `open` を使っている）を選んで Enter する操作は確実に前面化できる
/// （`Config` ソースの候補 = `[[apps]].path` を直接 spawn する方は同じく効かないことも実機で
/// 確認済み — `open` を経由するかどうかが分岐点）。そのため前面化そのものは
/// `open <解決済み .app バンドルパス>` に統一した — `open` は対象が既に起動中なら
/// ウィンドウを前面化・非表示解除する（Finder でダブルクリックするのと同じ）。
///
/// `toggle` で「フロントなら引っ込める」動作はウィンドウ単位の最小化ではなく、アプリ単位の
/// `hide`（Cmd+H 相当）— 複数ウィンドウがあっても一括で退避できる点は同じだが、Dock/Cmd+Tab
/// 上のアプリ自体は引き続き見える（ウィンドウだけが隠れる）という挙動差がある。
///
/// アプリ名は解決済みの `window_app`（または `path` の stem）。`NSRunningApplication` の
/// `localizedName`（Launch Services 上のローカライズ済み表示名。例: 日本語環境では
/// Preview.app が "プレビュー" になる）と、`bundleURL` から取った拡張子抜きファイル名
/// （ロケールに依存しない `.app` のベース名、例: "Preview"）の両方と大文字小文字無視で
/// 比較する — 前者だけだと非英語ロケールで多くのアプリがマッチしなくなる。見つからなければ
/// `NotFound` を返し、呼び出し側に設定済み `path` を起動させる。
///
/// `hide()` 自体が `false` を返しても `Err` にはしない — `activate_or_launch()` の `Err`
/// 分岐は「起動にフォールバック」するため、既に起動済みと分かっているアプリに対してそれを
/// やると新規プロセスが重複起動してしまう。見つからなかった場合のみ `NotFound` を返し、
/// その場合の起動フォールバックは正当（本当に未起動なので新規起動が正しい）。
#[cfg(target_os = "macos")]
mod macos_impl {
    use super::ActivateOutcome;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSRunningApplication, NSWorkspace};

    /// `on_app_hotkey`/`on_launch_hotkey` always call into this from a background thread.
    /// `NSRunningApplication`'s time-varying properties (`isActive`, `isHidden`) are documented
    /// by Apple as stale/race-prone when read off the main thread ("its time-varying properties
    /// may change from under you as the main run loop runs (or not)") — confirmed on real
    /// hardware: `isActive()` reported `true` for an app that was not actually frontmost when
    /// read from this background thread, causing the wrong branch (`hide` instead of activating)
    /// to run (2026-09-27). So the whole check-then-act sequence below is marshaled onto the
    /// main thread via `DispatchQueue::main().exec_sync()`, which blocks the calling background
    /// thread until it's done. This would deadlock if ever called from the main thread itself —
    /// it never is, by construction above.
    pub(super) fn activate(app_name: &str, toggle: bool) -> Result<ActivateOutcome, String> {
        let mut result = None;
        DispatchQueue::main().exec_sync(|| {
            result = Some(activate_on_main_thread(app_name, toggle));
        });
        result.expect("DispatchQueue::main().exec_sync always runs its closure before returning")
    }

    fn activate_on_main_thread(app_name: &str, toggle: bool) -> Result<ActivateOutcome, String> {
        let Some(app) = find_running(app_name) else {
            return Ok(ActivateOutcome::NotFound);
        };

        if toggle && app.isActive() {
            if !app.hide() {
                log::warn!("macos_impl: NSRunningApplication::hide() failed for \"{app_name}\"");
            }
            return Ok(ActivateOutcome::Minimized);
        }

        match app.bundleURL().and_then(|url| url.path()) {
            Some(path) => {
                let path = path.to_string();
                if let Err(e) = std::process::Command::new("open").arg(&path).spawn() {
                    log::warn!("macos_impl: failed to spawn `open {path}`: {e}");
                }
            }
            None => {
                log::warn!("macos_impl: could not resolve a bundle path for \"{app_name}\"");
            }
        }
        Ok(ActivateOutcome::Activated)
    }

    fn find_running(app_name: &str) -> Option<Retained<NSRunningApplication>> {
        let running = NSWorkspace::sharedWorkspace().runningApplications();
        (0..running.count())
            .map(|i| running.objectAtIndex(i))
            .find(|app| matches(app, app_name))
    }

    fn matches(app: &NSRunningApplication, app_name: &str) -> bool {
        if app
            .localizedName()
            .is_some_and(|name| name.to_string().eq_ignore_ascii_case(app_name))
        {
            return true;
        }
        app.bundleURL()
            .and_then(|url| url.URLByDeletingPathExtension())
            .and_then(|url| url.lastPathComponent())
            .is_some_and(|stem| stem.to_string().eq_ignore_ascii_case(app_name))
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
