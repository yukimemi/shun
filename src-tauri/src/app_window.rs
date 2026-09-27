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
/// `window_app` は全 OS 共通、タイトル条件は Windows / macOS のみ（Linux は非対応）。
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

/// ウィンドウ照合条件（config から渡される）。`title` / `title_exclude` は Windows と macOS が
/// 参照する（Windows: `EnumWindows` のタイトル文字列、macOS: `AXTitle`）。Linux (`wmctrl`) は
/// プロセス単位の照合しかできないため未参照。
#[derive(Clone, Copy)]
#[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
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
    return macos_impl::activate(&app, window, toggle);
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
/// `toggle` で「フロントなら引っ込める」動作は、`window_title` 未指定の場合はウィンドウ単位の
/// 最小化ではなく、アプリ単位の `hide`（Cmd+H 相当）— 複数ウィンドウがあっても一括で退避できる
/// 点は同じだが、Dock/Cmd+Tab 上のアプリ自体は引き続き見える（ウィンドウだけが隠れる）という
/// 挙動差がある。`window_title` 指定時は下記の通りウィンドウ単位の最小化になる。
///
/// `window_title` が指定されている場合は Accessibility API (`AXUIElement`) を使い、対象アプリの
/// ウィンドウ一覧 (`AXWindows`) からタイトルが一致する1つを探して個別に操作する
/// (`AXRaise` で前面化 / `AXMinimized` 属性で最小化)。同じアプリ内の他のウィンドウには影響しない
/// ため、例えば WezTerm 内の特定タブだけを activate/toggle でき、F12（アプリ全体の hide）が
/// 巻き込むこともない。`AXIsProcessTrusted()` が false（Accessibility 権限未許可）の場合は
/// ログを出してアプリ単位の動作にフォールバックする（`window_title` を使わないエントリは
/// この API に一切触れない — Accessibility 権限が無くても今まで通り動く）。
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
    use super::{ActivateOutcome, WindowMatch};
    use accessibility_sys::{
        kAXErrorSuccess, kAXFrontmostAttribute, kAXMainAttribute, kAXMinimizedAttribute,
        kAXRaiseAction, kAXTitleAttribute, kAXWindowsAttribute, AXError, AXIsProcessTrusted,
        AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementPerformAction,
        AXUIElementRef, AXUIElementSetAttributeValue,
    };
    use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
    use core_foundation_sys::base::{CFRelease, CFRetain, CFTypeRef};
    use core_foundation_sys::number::{
        kCFBooleanFalse, kCFBooleanTrue, CFBooleanGetValue, CFBooleanRef,
    };
    use core_foundation_sys::string::CFStringRef;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplicationActivationOptions, NSApplicationActivationPolicy, NSRunningApplication,
        NSWorkspace,
    };
    use objc2_foundation::NSString;

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
    ///
    /// After an `Activated`/`Minimized` outcome, this sleeps briefly on the (background) calling
    /// thread — not the main thread, so the app stays responsive — before returning. Confirmed on
    /// real hardware (2026-09-27): the window server needs a moment after `AXRaise`/
    /// `activateWithOptions`/`AXMinimized` actually completes before `isActive()`/`AXMain` reflect
    /// it; repeat-pressing the same toggle hotkey faster than that (~1.5-2s in testing) reads
    /// stale state and either fails to toggle at all, or (worse) reports the window "not found"
    /// and launches a duplicate. A real user's repeat presses are almost always slower than this,
    /// but the delay costs little and removes the failure mode outright.
    pub(super) fn activate(
        app_name: &str,
        window: WindowMatch,
        toggle: bool,
    ) -> Result<ActivateOutcome, String> {
        // `WindowMatch` borrows from the caller's config strings; copy out the two fields we
        // need so the closure below can be `'static` (`exec_sync` blocks until it returns, but
        // its closure type still isn't allowed to borrow past this call in dispatch2's API).
        let title = window.title.map(str::to_string);
        let title_exclude = window.title_exclude.map(str::to_string);
        let window_mode = title.is_some() || title_exclude.is_some();
        let mut result = None;
        DispatchQueue::main().exec_sync(|| {
            result = Some(activate_on_main_thread(
                app_name,
                title.as_deref(),
                title_exclude.as_deref(),
                toggle,
            ));
        });
        let result = result
            .expect("DispatchQueue::main().exec_sync always runs its closure before returning");
        // 設定待ちは window_title/window_title_exclude 使用時のみ — アプリ単位の `open`/`hide()`
        // 経路には元々このスレッド遅延の根拠となった AX 反映ラグはなく、無条件に付けると
        // 最も使われる（window_title 未指定の）ホットキーの応答を毎回無駄に遅らせてしまう。
        if window_mode
            && matches!(
                result,
                Ok(ActivateOutcome::Activated) | Ok(ActivateOutcome::Minimized)
            )
        {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        result
    }

    fn activate_on_main_thread(
        app_name: &str,
        title: Option<&str>,
        title_exclude: Option<&str>,
        toggle: bool,
    ) -> Result<ActivateOutcome, String> {
        let window_mode = title.is_some() || title_exclude.is_some();
        let trusted = window_mode && unsafe { AXIsProcessTrusted() };
        // 別プロセス登録（`wezterm-gui` 等）はウィンドウ検索でしか使えないので、
        // Accessibility 権限がある場合だけ候補に含める（無ければ従来の名前/bundle 一致のみ）。
        let (apps, strict_count) = find_running(app_name, trusted);
        let Some(app) = apps.first() else {
            return Ok(ActivateOutcome::NotFound);
        };

        if window_mode {
            if trusted {
                return activate_window(
                    &apps,
                    strict_count,
                    app_name,
                    title,
                    title_exclude,
                    toggle,
                );
            }
            log::warn!(
                "macos_impl: \"{app_name}\" has window_title/window_title_exclude set but \
                 Accessibility permission is not granted (System Settings > Privacy & Security > \
                 Accessibility) — falling back to whole-app activate/toggle"
            );
        }

        activate_whole_app(app, app_name, toggle)
    }

    /// アプリ単位の activate / toggle（`window_title` / `window_title_exclude` いずれも
    /// 未指定、または Accessibility 権限が無い場合のフォールバック）。
    fn activate_whole_app(
        app: &NSRunningApplication,
        app_name: &str,
        toggle: bool,
    ) -> Result<ActivateOutcome, String> {
        if toggle && app.isActive() {
            if !app.hide() {
                log::warn!("macos_impl: NSRunningApplication::hide() failed for \"{app_name}\"");
            }
            return Ok(ActivateOutcome::Minimized);
        }
        open_bundle(app, app_name);
        Ok(ActivateOutcome::Activated)
    }

    /// ウィンドウ単位の activate / toggle（`window_title` / `window_title_exclude` 指定時、
    /// Accessibility 経由）。条件に合うウィンドウを1つ探し（`title` はタイトルにこの文字列を
    /// 含むことを要求、`title_exclude` は含まないことを要求 — どちらか一方だけの指定も可）、
    /// toggle 時にそれが現在メイン & アプリがフォアグラウンドなら最小化、そうでなければ
    /// `AXRaise` でアプリ内の最前面にしつつ `open <bundle path>` でアプリ自体も前面化する。
    /// 一致するウィンドウが無ければ `NotFound`（呼び出し側で新規起動 = 新しいウィンドウを
    /// 作る、が正しい挙動）。
    fn activate_window(
        apps: &[Retained<NSRunningApplication>],
        strict_count: usize,
        app_name: &str,
        title: Option<&str>,
        title_exclude: Option<&str>,
        toggle: bool,
    ) -> Result<ActivateOutcome, String> {
        let title_lower = title.map(str::to_lowercase);
        let exclude_lower = title_exclude.map(str::to_lowercase);
        let describe = || format!("{:?}/exclude {:?}", title, title_exclude);

        // 一致するアプリインスタンスが複数ある場合（別プロセスとして登録された子プロセス等）、
        // 全インスタンスからウィンドウを集める。複数一致したときは、現在アクティブな
        // インスタンスのウィンドウを優先する（toggle で前面側を最小化できるように）。
        let mut enumeration_failed = false;
        let mut found: Option<(&Retained<NSRunningApplication>, AxWindow)> = None;
        for (i, app) in apps.iter().enumerate() {
            let pid = app.processIdentifier();
            match unsafe { find_window(pid, title_lower.as_deref(), exclude_lower.as_deref()) } {
                Ok(Some(w)) => {
                    if found.is_none() || (app.isActive() && !found.as_ref().unwrap().0.isActive())
                    {
                        found = Some((app, w));
                    }
                }
                Ok(None) => {}
                Err(()) => {
                    // 名前/bundle 一致のインスタンスの失敗は「列挙失敗」。別プロセス候補は、
                    // 通常の GUI アプリ（Regular）として登録されているものだけ列挙失敗として
                    // 扱う（ウィンドウを持たない helper は常に失敗するので単なる不一致）。
                    enumeration_failed |= i < strict_count
                        || app.activationPolicy() == NSApplicationActivationPolicy::Regular;
                }
            }
        }
        let Some((app, window)) = found else {
            if enumeration_failed {
                // 実行中なのに AXWindows を取得できなかった: 「ウィンドウ無し」と区別する。
                // NotFound を返すと起動フォールバックで重複プロセスが立つので、アプリ単位に倒す。
                log::warn!(
                    "macos_impl: AXWindows enumeration failed for \"{app_name}\" — falling back \
                     to whole-app activate/toggle"
                );
                return activate_whole_app(&apps[0], app_name, toggle);
            }
            return Ok(ActivateOutcome::NotFound);
        };

        // `app.isActive()` を「現在フォアグラウンドか」の判定に使う。既知の制限: 別プロセス
        // 登録（`wezterm-gui` を裸のサブプロセスとして起動した場合など）のウィンドウは、
        // 実際に最前面になっていても `isActive()` / `AXFrontmostAttribute` /
        // `NSWorkspace.frontmostApplication()` のいずれも true を返さないことを実機で確認した
        // （AppKit のフォアグラウンド判定と window server の実際の最前面ウィンドウが食い違う —
        // タイミングの問題ではなく、同じアプリバンドルから複数プロセスが起動している構成に
        // 起因する恒常的な不一致）。この場合 toggle は常に「フォアグラウンドではない」と
        // 判定し、常にアクティブ化のみを行う（最小化はしない）— 最小化に失敗するのではなく、
        // 単に最小化する機会が来ない。ウィンドウを探して前面化する主機能には影響しない。
        if toggle && app.isActive() && unsafe { ax_bool_attr(window.0, kAXMainAttribute) } {
            let err = unsafe { ax_set_bool_attr(window.0, kAXMinimizedAttribute, true) };
            if err != kAXErrorSuccess {
                log::warn!(
                    "macos_impl: AXMinimized=true failed for \"{app_name}\" window {} (err={err})",
                    describe()
                );
            }
            return Ok(ActivateOutcome::Minimized);
        }

        // 以前 toggle で最小化したウィンドウは AXRaise では復帰しないため、先に解除する。
        if unsafe { ax_bool_attr(window.0, kAXMinimizedAttribute) } {
            let err = unsafe { ax_set_bool_attr(window.0, kAXMinimizedAttribute, false) };
            if err != kAXErrorSuccess {
                log::warn!(
                    "macos_impl: AXMinimized=false failed for \"{app_name}\" window {} (err={err})",
                    describe()
                );
            }
        }
        let err = unsafe { ax_perform_action(window.0, kAXRaiseAction) };
        if err != kAXErrorSuccess {
            log::warn!(
                "macos_impl: AXRaise failed for \"{app_name}\" window {} (err={err})",
                describe()
            );
        }
        open_bundle(app, app_name);
        Ok(ActivateOutcome::Activated)
    }

    fn open_bundle(app: &NSRunningApplication, app_name: &str) {
        match app.bundleURL().and_then(|url| url.path()) {
            Some(path) => {
                let path = path.to_string();
                if let Err(e) = std::process::Command::new("open").arg(&path).spawn() {
                    log::warn!("macos_impl: failed to spawn `open {path}`: {e}");
                }
            }
            None => {
                // bundle を持たない別プロセス登録（例: `wezterm-gui`）は `open` できないので、
                // NSRunningApplication 自体を前面化するしかない。`activateWithOptions()` の
                // 戻り値は信用しない — PR #262 で実機確認済みの通り、shun の実行コンテキスト
                // からだと `true` を返すのに実際には何も起きないことがある（原因不明、
                // スレッド/権限とは無関係）。そのため戻り値に関わらず必ず AXFrontmost も試す。
                let _ = app.activateWithOptions(NSApplicationActivationOptions::empty());
                let ok = unsafe {
                    AXIsProcessTrusted() && {
                        let ax_app = AXUIElementCreateApplication(app.processIdentifier());
                        let err = ax_set_bool_attr(ax_app, kAXFrontmostAttribute, true);
                        CFRelease(ax_app as CFTypeRef);
                        err == kAXErrorSuccess
                    }
                };
                if !ok {
                    log::warn!("macos_impl: could not activate \"{app_name}\" (no bundle path)");
                }
            }
        }
    }

    /// 一致する実行中アプリを全て返す。`include_subprocess` が true のときは、実行ファイル名が
    /// `<app_name>-…`（例: `wezterm-gui`）の別プロセス登録も末尾に含める（ウィンドウ検索用）。
    fn find_running(
        app_name: &str,
        include_subprocess: bool,
    ) -> (Vec<Retained<NSRunningApplication>>, usize) {
        let running = NSWorkspace::sharedWorkspace().runningApplications();
        let all: Vec<_> = (0..running.count())
            .map(|i| running.objectAtIndex(i))
            .collect();
        let (mut hits, rest): (Vec<_>, Vec<_>) =
            all.into_iter().partition(|app| matches(app, app_name));
        let strict_count = hits.len();
        if include_subprocess {
            hits.extend(rest.into_iter().filter(|app| is_subprocess(app, app_name)));
        }
        (hits, strict_count)
    }

    /// `app_name` に対する「バンドルを介さず別プロセスとして登録された同じアプリのインスタンス」
    /// かどうかを判定する。`matches()` は `localizedName` / `bundleURL` の一致で判定するが、
    /// bare サブプロセスの `bundleURL` がたまたま解決できない構成もあり得るため、実行ファイル名
    /// の前方一致（`<app_name>-...`、例: `wezterm-gui`）もフォールバックとして見る。
    /// 前方一致だけだと同じ接頭辞を持つ無関係な別アプリを拾う理論上の余地はあるが、
    /// `matches()` で先に弾かれなかった（= 名前にもバンドルにも一致しない）ものだけが対象
    /// になる時点で誤爆の実害は乏しく、`include_subprocess`（Accessibility 権限がある時だけ）
    /// でさらに絞っている。
    fn is_subprocess(app: &NSRunningApplication, app_name: &str) -> bool {
        let prefix = format!("{}-", app_name.to_lowercase());
        app.executableURL()
            .and_then(|url| url.URLByDeletingPathExtension())
            .and_then(|url| url.lastPathComponent())
            .is_some_and(|stem| stem.to_string().to_lowercase().starts_with(&prefix))
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

    /// `CFRelease` が必要な、借用を離れて保持する `AXUIElementRef`（ウィンドウ）の RAII ラッパ。
    struct AxWindow(AXUIElementRef);

    impl Drop for AxWindow {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0 as CFTypeRef) };
        }
    }

    /// `pid` のアプリが持つウィンドウ (`AXWindows`) を列挙し、（指定があれば）タイトルに
    /// `title_lower`（大文字小文字無視の部分一致）を含み、かつ（指定があれば）
    /// `exclude_lower` を含まない最初の1つを返す。両方 `None` なら最初のウィンドウを返す。
    unsafe fn find_window(
        pid: i32,
        title_lower: Option<&str>,
        exclude_lower: Option<&str>,
    ) -> Result<Option<AxWindow>, ()> {
        let ax_app = AXUIElementCreateApplication(pid);
        let windows_ref = ax_copy_attr(ax_app, kAXWindowsAttribute);
        CFRelease(ax_app as CFTypeRef);
        let Some(windows_ref) = windows_ref else {
            return Err(());
        };
        let windows = windows_ref as CFArrayRef;
        let count = CFArrayGetCount(windows);
        let found = (0..count).find_map(|i| {
            let win = CFArrayGetValueAtIndex(windows, i) as AXUIElementRef;
            let title = if title_lower.is_some() || exclude_lower.is_some() {
                let title = ax_copy_attr(win, kAXTitleAttribute).map(|t| {
                    let s = cfstring_to_string(t);
                    CFRelease(t);
                    s
                })?;
                title.to_lowercase()
            } else {
                String::new()
            };
            if title_lower.is_some_and(|t| !title.contains(t)) {
                return None;
            }
            if exclude_lower.is_some_and(|ex| title.contains(ex)) {
                return None;
            }
            Some(win)
        });
        let result = found.map(|win| {
            CFRetain(win as CFTypeRef);
            AxWindow(win)
        });
        CFRelease(windows_ref);
        Ok(result)
    }

    unsafe fn ax_copy_attr(element: AXUIElementRef, attr: &str) -> Option<CFTypeRef> {
        let attr_str = NSString::from_str(attr);
        let attr_ref = Retained::as_ptr(&attr_str) as *const _ as CFStringRef;
        let mut value: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr_ref, &mut value);
        (err == kAXErrorSuccess && !value.is_null()).then_some(value)
    }

    unsafe fn ax_bool_attr(element: AXUIElementRef, attr: &str) -> bool {
        ax_copy_attr(element, attr)
            .map(|v| {
                let b = CFBooleanGetValue(v as CFBooleanRef);
                CFRelease(v);
                b
            })
            .unwrap_or(false)
    }

    unsafe fn ax_set_bool_attr(element: AXUIElementRef, attr: &str, val: bool) -> AXError {
        let attr_str = NSString::from_str(attr);
        let attr_ref = Retained::as_ptr(&attr_str) as *const _ as CFStringRef;
        let b = if val { kCFBooleanTrue } else { kCFBooleanFalse };
        AXUIElementSetAttributeValue(element, attr_ref, b as CFTypeRef)
    }

    unsafe fn ax_perform_action(element: AXUIElementRef, action: &str) -> AXError {
        let action_str = NSString::from_str(action);
        let action_ref = Retained::as_ptr(&action_str) as *const _ as CFStringRef;
        AXUIElementPerformAction(element, action_ref)
    }

    unsafe fn cfstring_to_string(cf: CFTypeRef) -> String {
        (*(cf as *const NSString)).to_string()
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
