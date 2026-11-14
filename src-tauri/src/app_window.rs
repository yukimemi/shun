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
    /// 実行中と分かっているアプリのウィンドウ一覧を取得できなかった（macOS の `AXWindows`
    /// 失敗）。起動してはならない（重複プロセス）し、アプリ単位の前面化もしてはならない
    /// （誤ったウィンドウを前面化して成功を装う）。何もせず警告ログだけ残し、ユーザーに
    /// もう一度押してもらう。
    EnumerationFailed,
    /// 対象ウィンドウは見つかったが、それに対する操作自体が失敗した（niri の
    /// `focus-window` 等）。実行中なので起動してはならない（重複プロセス）。警告だけ残す。
    ActionFailed(String),
}

/// Whether the caller must launch a new process for this outcome. `EnumerationFailed` is
/// deliberately *not* a launch trigger: the app is known to be running, so launching would
/// duplicate the process.
fn falls_back_to_launch(outcome: &ActivateOutcome) -> bool {
    matches!(
        outcome,
        ActivateOutcome::NotFound | ActivateOutcome::Unsupported
    )
}

/// Maps an activation result to the final action. `launch` runs only when the outcome (or an
/// error from the window operation) calls for a fresh launch.
fn resolve_outcome(
    result: Result<ActivateOutcome, String>,
    app_name: &str,
    launch: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match result {
        Ok(ActivateOutcome::EnumerationFailed) => {
            log::warn!(
                "activate_or_launch: could not enumerate windows of running \"{app_name}\" — \
                 doing nothing (no launch, no whole-app activate); press the hotkey again"
            );
            Ok(())
        }
        Ok(ActivateOutcome::ActionFailed(e)) => {
            log::warn!(
                "activate_or_launch: window operation on running \"{app_name}\" failed ({e}) — \
                 doing nothing (no launch)"
            );
            Ok(())
        }
        Ok(o) if falls_back_to_launch(&o) => launch(),
        Ok(_) => Ok(()),
        Err(e) => {
            log::warn!("activate_or_launch: window operation failed ({e}), launching instead");
            launch()
        }
    }
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
    #[cfg(target_os = "windows")]
    {
        let Some(app) = resolve_window_app(window.app, &item.path) else {
            return apps::launch_with_extra(item, Vec::new(), vars);
        };
        let spec = windows_impl::spec_for(item, &app, &window);
        let Some(guard) = windows_impl::EntryGuard::acquire(&spec.marker) else {
            return Ok(());
        };
        return resolve_outcome(windows_impl::activate(&spec, toggle), &item.name, || {
            apps::launch_with_extra(item, Vec::new(), vars)?;
            windows_impl::start_discovery(spec, guard);
            Ok(())
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        // macOS also attempts activation after launching.
        let launch_and_activate = || {
            apps::launch_with_extra(item, Vec::new(), vars)?;
            #[cfg(target_os = "macos")]
            macos_impl::activate_after_launch(item, window);
            Ok(())
        };
        resolve_outcome(
            try_activate(item, window, toggle),
            &item.name,
            launch_and_activate,
        )
    }
}

/// ウィンドウ照合条件（config から渡される）。`title` / `title_exclude` は Windows・macOS・
/// Linux (niri) が参照する（Windows: `EnumWindows` のタイトル文字列、macOS: `AXTitle`、
/// niri: IPC のウィンドウタイトル）。Linux の `wmctrl` はプロセス単位の照合しかできないため未参照。
#[derive(Clone, Copy)]
#[cfg_attr(
    not(any(target_os = "windows", target_os = "macos", target_os = "linux")),
    allow(dead_code)
)]
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

/// Delay before the single retry after an `EnumerationFailed` result.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const ENUMERATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

/// Whether a failed window enumeration should be retried: once only (`attempt` counts the
/// retries already done).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn should_retry_enumeration(result: &Result<ActivateOutcome, String>, attempt: u32) -> bool {
    attempt == 0 && matches!(result, Ok(ActivateOutcome::EnumerationFailed))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn try_activate(
    item: &LaunchItem,
    window: WindowMatch,
    toggle: bool,
) -> Result<ActivateOutcome, String> {
    let Some(app) = resolve_window_app(window.app, &item.path) else {
        return Ok(ActivateOutcome::NotFound);
    };
    #[cfg(target_os = "macos")]
    return macos_impl::activate(&item.name, &app, window, toggle);
    #[cfg(target_os = "linux")]
    return linux_impl::activate(&app, window, toggle);
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn try_activate(
    _item: &LaunchItem,
    _window: WindowMatch,
    _toggle: bool,
) -> Result<ActivateOutcome, String> {
    Ok(ActivateOutcome::Unsupported)
}

/// Windows: `EnumWindows` で対象プロセスの通常ウィンドウを探し、
/// `SetForegroundWindow` / `ShowWindow` で操作する。
///
/// マッチは解決済みアプリ名（`window_app` または `path` の file stem）を
/// 大文字小文字無視で実行ファイル名と比較する。
///
/// 対象を絞るフィルタ: 可視ウィンドウのみ、オーナーウィンドウを持たない、
/// `WS_EX_TOOLWINDOW` を除外（通常のアプリウィンドウのみを対象にする）。
///
/// **ウィンドウの関連付け**: タイトルは後から変わる（Windows Terminal のタブ切替など）ため、
/// タイトルだけに頼ると既存ウィンドウを見失い、`wt -w name new-tab` が重複タブを作る。
/// そこで shun は対象ウィンドウに `SetPropW` でエントリ固有のマーカー（エントリ名・
/// 解決済み exe stem のバージョン付き指紋）を付ける。ウィンドウプロパティはウィンドウと
/// 運命を共にするため HWND の再利用で別ウィンドウを掴むことがなく、shun を再起動しても残る。
/// 補完設定やテンプレート変数の編集では関連付けを失わない。
///
/// - マーカー付きウィンドウがちょうど1つ → タイトルの正条件は無視してそれを操作する
///   （`title_exclude` は維持する）。複数なら曖昧なので何もしない（`ActionFailed`）。
/// - マーカーが無ければ従来どおり exe + タイトルで探す。一意に一致したときだけマーカーを付ける。
/// - 見つからず起動した場合、バックグラウンドで最大約 10 秒、exe + タイトルが一意に一致する
///   ウィンドウを探してマーカーを付ける（`wt` が既存プロセスへ転送するため PID は使えない）。
///   その間の同一エントリのホットキーは何もしない（重複起動防止）。タイムアウトしても再起動は
///   しない。関連付け前にタイトルが変わった場合は安全に復旧できない（既知の制限）。
///
/// 関連付けは「ウィンドウ」単位でありタブ単位ではない。
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{ActivateOutcome, LaunchItem};
    use std::collections::HashSet;
    use std::path::Path;
    use std::sync::{LazyLock, Mutex};
    use windows::core::{BOOL, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetForegroundWindow, GetPropW, GetWindow, GetWindowLongPtrW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
        IsWindowVisible, SetForegroundWindow, SetPropW, ShowWindow, GWL_EXSTYLE, GW_OWNER,
        SW_MINIMIZE, SW_RESTORE, WS_EX_TOOLWINDOW,
    };

    const DISCOVERY_POLLS: u32 = 50;
    const DISCOVERY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

    /// 起動後のウィンドウ探索中のエントリ（マーカーキーで識別）。
    static PENDING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

    /// 探す対象ウィンドウの条件。exe stem 一致 AND タイトル部分一致（指定時）
    /// AND タイトル除外文字列を含まない（指定時）。
    pub(super) struct Spec {
        stem: String,
        /// 小文字化済み
        title: Option<String>,
        /// 小文字化済み
        title_exclude: Option<String>,
        /// ウィンドウプロパティ名（指紋入り）
        pub(super) marker: String,
    }

    /// 列挙した通常ウィンドウ1つ分の判定結果。
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Candidate {
        hwnd: isize,
        marked: bool,
        /// タイトル正条件 AND 除外条件を満たす
        strict: bool,
        /// 除外条件を満たす（正条件は問わない）
        not_excluded: bool,
    }

    #[derive(Debug, PartialEq)]
    enum Selection {
        /// マーカー付きウィンドウが一意。
        Tracked(isize),
        /// マーカー付きウィンドウが複数。
        Ambiguous,
        /// 関連付け済みだが除外または操作対象外。重複起動しない。
        Blocked,
        /// マーカー無し・厳密一致が一意 → 関連付けてよい。
        Bind(isize),
        /// マーカー無し・厳密一致が複数 → 従来どおり先頭を操作するが関連付けはしない。
        First(isize),
        None,
    }

    fn select(cands: &[Candidate]) -> Selection {
        let marked: Vec<_> = cands.iter().filter(|c| c.marked).collect();
        match marked.len() {
            1 if marked[0].not_excluded => return Selection::Tracked(marked[0].hwnd),
            1 => return Selection::Blocked,
            0 => {}
            _ => return Selection::Ambiguous,
        }
        let strict: Vec<_> = cands.iter().filter(|c| c.strict).collect();
        match strict.len() {
            0 => Selection::None,
            1 => Selection::Bind(strict[0].hwnd),
            _ => Selection::First(strict[0].hwnd),
        }
    }

    fn foreground_is_target(selection: &Selection, cands: &[Candidate], foreground: isize) -> bool {
        match selection {
            Selection::Tracked(hwnd) | Selection::Bind(hwnd) => foreground == *hwnd,
            Selection::First(_) => cands.iter().any(|c| c.hwnd == foreground && c.strict),
            _ => false,
        }
    }

    /// 起動後探索用: 厳密一致が一意のときだけ返す。
    fn discover(cands: &[Candidate]) -> Option<isize> {
        if cands.iter().any(|c| c.marked) {
            return None;
        }
        let mut strict = cands.iter().filter(|c| c.strict);
        match (strict.next(), strict.next()) {
            (Some(c), None) => Some(c.hwnd),
            _ => None,
        }
    }

    fn fnv1a(parts: &[&str]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for p in parts {
            for b in p.bytes().chain(std::iter::once(0u8)) {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h
    }

    fn marker_name(item: &LaunchItem, stem: &str) -> String {
        // Keep property names bounded; only entry identity, not mutable launch/completion
        // settings or unrelated template variables, determines the association.
        let h = fnv1a(&[&item.name, stem]);
        format!("shun.window.v3.{h:016x}")
    }

    pub(super) fn spec_for(item: &LaunchItem, app: &str, window: &super::WindowMatch) -> Spec {
        // app は resolve_window_app() で解決済み（.exe 除去済み）。比較用に小文字化する。
        let stem = app.to_lowercase();
        let normalize = |s: Option<&str>| s.filter(|t| !t.is_empty()).map(str::to_lowercase);
        Spec {
            marker: marker_name(item, &stem),
            stem,
            title: normalize(window.title),
            title_exclude: normalize(window.title_exclude),
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn has_marker(hwnd: HWND, marker: &str) -> bool {
        let name = wide(marker);
        !GetPropW(hwnd, PCWSTR(name.as_ptr())).is_invalid()
    }

    unsafe fn set_marker(hwnd: HWND, marker: &str) -> Result<(), String> {
        let name = wide(marker);
        SetPropW(
            hwnd,
            PCWSTR(name.as_ptr()),
            Some(HANDLE(1usize as *mut std::ffi::c_void)),
        )
        .map_err(|e| format!("SetPropW failed: {e}"))
    }

    fn pending_lock() -> std::sync::MutexGuard<'static, HashSet<String>> {
        PENDING.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Owns the pending marker through launch and asynchronous discovery.
    pub(super) struct EntryGuard(String);

    impl EntryGuard {
        pub(super) fn acquire(marker: &str) -> Option<Self> {
            pending_lock()
                .insert(marker.to_owned())
                .then(|| Self(marker.to_owned()))
        }
    }

    impl Drop for EntryGuard {
        fn drop(&mut self) {
            pending_lock().remove(&self.0);
        }
    }

    pub(super) fn activate(spec: &Spec, toggle: bool) -> Result<ActivateOutcome, String> {
        let cands = enumerate(spec)?;
        let selection = select(&cands);
        let hwnd = match selection {
            Selection::Tracked(h) | Selection::First(h) => h,
            Selection::Bind(h) => {
                if let Err(e) =
                    unsafe { set_marker(HWND(h as *mut std::ffi::c_void), &spec.marker) }
                {
                    // 関連付けられないだけで、見つかった対象の操作は続行できる。
                    log::warn!("app_window(windows): could not mark window: {e}");
                }
                h
            }
            Selection::Ambiguous => {
                return Ok(ActivateOutcome::ActionFailed(
                    "multiple windows carry this entry's marker".into(),
                ));
            }
            Selection::Blocked => {
                return Ok(ActivateOutcome::ActionFailed(
                    "associated window is excluded or ineligible; not launching".into(),
                ));
            }
            Selection::None => {
                // The caller retains its EntryGuard until discovery ends.
                return Ok(ActivateOutcome::NotFound);
            }
        };
        let hwnd = HWND(hwnd as *mut std::ffi::c_void);

        unsafe {
            // Once associated, only that HWND may be minimized. Before association,
            // retain the foreground preference among multiple strict matches.
            if toggle {
                let foreground = GetForegroundWindow();
                let fg_is_target = !foreground.is_invalid()
                    && foreground_is_target(&selection, &cands, foreground.0 as isize);
                if fg_is_target {
                    let _ = ShowWindow(foreground, SW_MINIMIZE);
                    if !IsIconic(foreground).as_bool() {
                        return Ok(ActivateOutcome::ActionFailed(
                            "window minimization failed".into(),
                        ));
                    }
                    return Ok(ActivateOutcome::Minimized);
                }
            }
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if !force_foreground(hwnd) {
                log::warn!("app_window(windows): SetForegroundWindow failed (foreground lock?)");
            }
        }
        Ok(ActivateOutcome::Activated)
    }

    /// 起動直後にウィンドウを探してマーカーを付けるスレッドを開始する。
    /// The worker owns pending state; failed spawn and unwinding release it too.
    pub(super) fn start_discovery(spec: Spec, guard: EntryGuard) {
        let _ = std::thread::Builder::new()
            .name("shun-window-discovery".into())
            .spawn({
                move || {
                    let _guard = guard;
                    for _ in 0..DISCOVERY_POLLS {
                        std::thread::sleep(DISCOVERY_INTERVAL);
                        let Ok(cands) = enumerate(&spec) else {
                            continue;
                        };
                        if let Some(h) = discover(&cands) {
                            let hwnd = HWND(h as *mut std::ffi::c_void);
                            unsafe {
                                if IsWindow(Some(hwnd)).as_bool() {
                                    if let Err(e) = set_marker(hwnd, &spec.marker) {
                                        log::warn!(
                                            "app_window(windows): could not mark window: {e}"
                                        );
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            });
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

    struct EnumState<'a> {
        spec: &'a Spec,
        out: Vec<Candidate>,
    }

    /// 対象 exe の通常ウィンドウをすべて列挙する。
    fn enumerate(spec: &Spec) -> Result<Vec<Candidate>, String> {
        let mut state = EnumState {
            spec,
            out: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(enum_proc),
                LPARAM(&mut state as *mut EnumState as isize),
            )
            .map_err(|e| format!("EnumWindows failed: {e}"))?;
        }
        Ok(state.out)
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut EnumState);

        let spec = state.spec;
        let marked = has_marker(hwnd, &spec.marker);
        let ownerless = GetWindow(hwnd, GW_OWNER).map_or(true, |owner| owner.is_invalid());
        let eligible = IsWindowVisible(hwnd).as_bool()
            && ownerless
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
            && window_exe_stem(hwnd).as_deref() == Some(spec.stem.as_str());
        // Keep marked windows in the result even when hidden, excluded, or otherwise
        // ineligible: a known association must never become a fresh launch.
        if !eligible {
            if marked {
                state.out.push(Candidate {
                    hwnd: hwnd.0 as isize,
                    marked,
                    strict: false,
                    not_excluded: false,
                });
            }
            return true.into();
        }
        let (strict, not_excluded) = if spec.title.is_none() && spec.title_exclude.is_none() {
            (true, true)
        } else {
            let title = window_title(hwnd).to_lowercase();
            let not_excluded = spec
                .title_exclude
                .as_ref()
                .is_none_or(|t| !title.contains(t.as_str()));
            let positive = spec
                .title
                .as_ref()
                .is_none_or(|t| title.contains(t.as_str()));
            (positive && not_excluded, not_excluded)
        };
        state.out.push(Candidate {
            hwnd: hwnd.0 as isize,
            marked,
            strict,
            not_excluded,
        });
        true.into()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn c(hwnd: isize, marked: bool, strict: bool, not_excluded: bool) -> Candidate {
            Candidate {
                hwnd,
                marked,
                strict,
                not_excluded,
            }
        }

        #[test]
        fn marked_window_wins_despite_title_drift() {
            let cands = [c(1, false, true, true), c(2, true, false, true)];
            assert_eq!(select(&cands), Selection::Tracked(2));
        }

        #[test]
        fn tracked_toggle_ignores_unrelated_foreground_strict_match() {
            let cands = [c(1, false, true, true), c(2, true, false, true)];
            let selection = select(&cands);
            assert!(!foreground_is_target(&selection, &cands, 1));
            assert!(foreground_is_target(&selection, &cands, 2));
            let untracked = [c(1, false, true, true), c(2, false, true, true)];
            assert!(foreground_is_target(&select(&untracked), &untracked, 2));
        }

        #[test]
        fn marked_but_excluded_blocks_even_with_an_unrelated_strict_match() {
            let cands = [c(2, true, false, false), c(3, false, true, true)];
            assert_eq!(select(&cands), Selection::Blocked);
        }

        #[test]
        fn multiple_marked_is_ambiguous() {
            let cands = [c(1, true, true, true), c(2, true, true, true)];
            assert_eq!(select(&cands), Selection::Ambiguous);
        }

        #[test]
        fn unmarked_unique_binds_and_multiple_does_not() {
            assert_eq!(
                select(&[c(1, false, true, true), c(2, false, false, true)]),
                Selection::Bind(1)
            );
            assert_eq!(
                select(&[c(1, false, true, true), c(2, false, true, true)]),
                Selection::First(1)
            );
            assert_eq!(select(&[c(1, false, false, true)]), Selection::None);
        }

        #[test]
        fn discovery_requires_unique_strict_match() {
            assert_eq!(discover(&[c(1, false, true, true)]), Some(1));
            assert_eq!(
                discover(&[c(1, false, true, true), c(2, false, true, true)]),
                None
            );
            assert_eq!(discover(&[c(1, false, false, true)]), None);
        }

        fn item() -> LaunchItem {
            serde_json::from_value(serde_json::json!({
                "name": "window-lifecycle-test", "path": "not-a-real-launch-command",
                "args": [], "workdir": null, "source": "Config"
            }))
            .unwrap()
        }

        #[test]
        fn marker_survives_unrelated_config_edits_but_separates_entries_and_apps() {
            let mut item = item();
            let original = marker_name(&item, "windowsterminal");
            item.completion_list = vec!["other".into()];
            item.history_key = Some("other-history".into());
            assert_eq!(original, marker_name(&item, "windowsterminal"));
            assert_ne!(original, marker_name(&item, "other-app"));
            item.name.push_str("-other");
            assert_ne!(original, marker_name(&item, "windowsterminal"));
        }

        #[test]
        fn pending_guard_releases_after_failed_launch_and_worker_unwind() {
            let item = item();
            let window = super::super::WindowMatch {
                app: Some("shun-nonexistent-window-test"),
                title: None,
                title_exclude: None,
            };
            let vars = std::collections::HashMap::new();
            let spec = spec_for(&item, window.app.unwrap(), &window);
            assert!(super::super::activate_or_launch(&item, window, false, &vars).is_err());
            let guard = EntryGuard::acquire(&spec.marker).expect("failed launch releases pending");
            assert!(EntryGuard::acquire(&spec.marker).is_none());
            let worker = std::thread::spawn(move || {
                let _guard = guard;
                panic!("exercise discovery unwinding");
            });
            assert!(worker.join().is_err());
            assert!(EntryGuard::acquire(&spec.marker).is_some());
        }

        #[test]
        fn native_marker_tracks_title_restart_exclusion_and_window_lifetime() {
            use windows::core::w;
            use windows::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, SetWindowTextW, SW_HIDE, SW_SHOWNOACTIVATE,
                WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
            };
            struct TestWindow(HWND);
            impl Drop for TestWindow {
                fn drop(&mut self) {
                    unsafe {
                        let _ = DestroyWindow(self.0);
                    }
                }
            }
            let title = format!("shun-association-test-{}", std::process::id());
            let title_wide = wide(&title);
            let mut item = item();
            item.name = title.clone();
            let app = exe_stem(std::env::current_exe().unwrap().to_str().unwrap());
            let window = super::super::WindowMatch {
                app: Some(&app),
                title: Some(&title),
                title_exclude: Some("excluded"),
            };
            let vars = std::collections::HashMap::new();
            let spec = spec_for(&item, &app, &window);
            let create = || {
                TestWindow(unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        w!("STATIC"),
                        PCWSTR(title_wide.as_ptr()),
                        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                        0,
                        0,
                        100,
                        100,
                        None,
                        None,
                        None,
                        None,
                    )
                    .unwrap()
                })
            };
            let first = create();
            let raw = first.0 .0 as isize;
            assert_eq!(select(&enumerate(&spec).unwrap()), Selection::Bind(raw));
            let second = create();
            unsafe {
                let mut first_pid = 0;
                let mut second_pid = 0;
                GetWindowThreadProcessId(first.0, Some(&mut first_pid));
                GetWindowThreadProcessId(second.0, Some(&mut second_pid));
                assert_eq!(first_pid, second_pid);
            }
            let ambiguous = enumerate(&spec).unwrap();
            assert!(matches!(select(&ambiguous), Selection::First(_)));
            assert_eq!(discover(&ambiguous), None);
            unsafe {
                set_marker(first.0, &spec.marker).unwrap();
                SetWindowTextW(first.0, w!("another tab")).unwrap();
            }
            assert_eq!(select(&enumerate(&spec).unwrap()), Selection::Tracked(raw));
            // Rebuild lookup inputs: no in-memory HWND survives a simulated restart.
            let mut edited_vars = vars.clone();
            edited_vars.insert("unrelated".into(), "value".into());
            item.completion_list = vec!["changed completion".into()];
            let restarted = spec_for(&item, &app, &window);
            // Config reload with unrelated vars still finds the drifted window.
            super::super::activate_or_launch(&item, window, false, &edited_vars).unwrap();
            assert_eq!(
                select(&enumerate(&restarted).unwrap()),
                Selection::Tracked(raw)
            );
            unsafe {
                SetWindowTextW(first.0, w!("excluded tab")).unwrap();
            }
            assert_eq!(select(&enumerate(&restarted).unwrap()), Selection::Blocked);
            let mut launched = false;
            super::super::resolve_outcome(activate(&restarted, false), &item.name, || {
                launched = true;
                Ok(())
            })
            .unwrap();
            assert!(!launched);
            unsafe {
                SetWindowTextW(first.0, w!("another tab")).unwrap();
                let _ = ShowWindow(first.0, SW_HIDE);
            }
            assert_eq!(select(&enumerate(&restarted).unwrap()), Selection::Blocked);
            assert!(matches!(
                activate(&restarted, false).unwrap(),
                ActivateOutcome::ActionFailed(_)
            ));
            unsafe {
                let _ = ShowWindow(first.0, SW_SHOWNOACTIVATE);
            }
            drop(second);
            drop(first);
            let replacement = create();
            let replacement_raw = replacement.0 .0 as isize;
            assert!(!unsafe { has_marker(replacement.0, &restarted.marker) });
            assert_eq!(
                select(&enumerate(&restarted).unwrap()),
                Selection::Bind(replacement_raw)
            );
            unsafe {
                set_marker(replacement.0, &restarted.marker).unwrap();
                SetWindowTextW(replacement.0, w!("replacement tab")).unwrap();
            }
            assert_eq!(
                select(&enumerate(&restarted).unwrap()),
                Selection::Tracked(replacement_raw)
            );
        }
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
/// 巻き込むこともない。未許可の場合、`window_title` を使うエントリのホットキーが最初に
/// 押されたタイミングでシステム標準の許可ダイアログを一度だけ出す
/// （`AXIsProcessTrustedWithOptions` + `kAXTrustedCheckOptionPrompt`、以降はプロセスの
/// 寿命中再度は出さない）。許可されなかった場合はログを出してアプリ単位の動作に
/// フォールバックする（`window_title` を使わないエントリはこの API に一切触れない —
/// Accessibility 権限もダイアログも無くても今まで通り動く）。
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
        kAXErrorSuccess, kAXMainAttribute, kAXMinimizedAttribute, kAXRaiseAction,
        kAXTitleAttribute, kAXTrustedCheckOptionPrompt, kAXWindowsAttribute, AXError,
        AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue,
        AXUIElementCreateApplication, AXUIElementPerformAction, AXUIElementRef,
        AXUIElementSetAttributeValue,
    };
    use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
    use core_foundation_sys::base::{CFRelease, CFRetain, CFTypeRef};
    use core_foundation_sys::dictionary::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
    };
    use core_foundation_sys::number::{
        kCFBooleanFalse, kCFBooleanTrue, CFBooleanGetValue, CFBooleanRef,
    };
    use core_foundation_sys::string::CFStringRef;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
        NSRunningApplication, NSWorkspace,
    };
    use objc2_foundation::NSString;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

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
        item_name: &str,
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
        let mut attempt = 0;
        let result = loop {
            let mut result = None;
            DispatchQueue::main().exec_sync(|| {
                result = Some(activate_on_main_thread(
                    item_name,
                    app_name,
                    title.as_deref(),
                    title_exclude.as_deref(),
                    toggle,
                    false,
                ));
            });
            let result = result
                .expect("DispatchQueue::main().exec_sync always runs its closure before returning");
            // Retry here, on the background thread, never inside the main-thread closure
            // (sleeping there would freeze the app).
            if super::should_retry_enumeration(&result, attempt) {
                attempt += 1;
                std::thread::sleep(super::ENUMERATION_RETRY_DELAY);
                continue;
            }
            break result;
        };
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

    /// `activate_or_launch()` がウィンドウを見つけられず新規起動した直後に呼ばれる。
    ///
    /// バックグラウンドから spawn した GUI プロセスは、ウィンドウを作っても自動では
    /// フォアグラウンドにならないことを実機で確認した（`wezterm-gui start -- yazi` の
    /// 初回起動後、ウィンドウは背面に残ったまま — 次のホットキー押下で明示的に
    /// activate するまで前面化しなかった）。
    ///
    /// `item.name` をキーにした `apps::launched_pid` キャッシュ（`activate_on_main_thread`
    /// が最優先で参照する）のおかげで、単純な「同名の実行中インスタンスを何でもいいから
    /// 前面化する」誤り（`window_title` 指定時、たった今 spawn したはずのウィンドウでは
    /// なく、たまたま先に見つかった既存の別インスタンス — 例えば元々起動していたシェルの
    /// WezTerm — が前面化されてしまう、実機で確認済みの不具合）を踏まずに済む。新しい
    /// ウィンドウが実際に見えるようになるまでには一呼吸あるため、短い間隔で数回
    /// リトライする（起動直後の1回だけではまだ見つからないことがある）。`toggle=false`
    /// 固定（このタイミングで最小化してしまうことはない）。
    ///
    /// `post_launch=true` で呼ぶ: 起動直後のプロセスはまだウィンドウも AX も準備できて
    /// おらず `AXWindows` の列挙が失敗しがち。通常経路ではそれを「列挙失敗」としてアプリ
    /// 単位の前面化に倒すが、ここでそれをやると `open <bundle>` が元から動いていた別
    /// インスタンス（shell の WezTerm）を前面化し、それを「成功」と見なしてリトライを
    /// 打ち切ってしまう — 実機で確認した「Ctrl+F10 で shell 側がアクティブになる」の直接
    /// 原因。`post_launch` では列挙失敗を「まだ準備中」＝リトライ扱いにする。
    /// リトライを使い切っても見つからなければ、最後の手段として起動したプロセス自体
    /// （pid キャッシュ）を前面化する — 少なくとも正しいインスタンスが前に出る。
    pub(super) fn activate_after_launch(item: &super::LaunchItem, window: super::WindowMatch) {
        let Some(app_name) = super::resolve_window_app(window.app, &item.path) else {
            return;
        };
        let title = window.title.map(str::to_string);
        let title_exclude = window.title_exclude.map(str::to_string);
        for _ in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let mut result = None;
            DispatchQueue::main().exec_sync(|| {
                result = Some(activate_on_main_thread(
                    &item.name,
                    &app_name,
                    title.as_deref(),
                    title_exclude.as_deref(),
                    false,
                    true,
                ));
            });
            if matches!(result, Some(Ok(ActivateOutcome::Activated))) {
                return;
            }
        }
        let mut activated_launched = false;
        DispatchQueue::main().exec_sync(|| {
            if let Some(app) = crate::apps::launched_pid(&item.name).and_then(|pid| {
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid as libc::pid_t)
            }) {
                activate_specific_instance(&app, &app_name);
                activated_launched = true;
            }
        });
        log::warn!(
            "macos_impl: \"{app_name}\" window {:?}/exclude {:?} not found after launching it{}",
            title,
            title_exclude,
            if activated_launched {
                " — activated the launched process itself instead"
            } else {
                ""
            }
        );
    }

    fn activate_on_main_thread(
        item_name: &str,
        app_name: &str,
        title: Option<&str>,
        title_exclude: Option<&str>,
        toggle: bool,
        post_launch: bool,
    ) -> Result<ActivateOutcome, String> {
        let window_mode = title.is_some() || title_exclude.is_some();
        let ax_trusted = window_mode && unsafe { ax_is_trusted_prompting_once() };

        // shun 自身がこのエントリ向けに直近で spawn したプロセスがまだ生きていれば、
        // 名前/タイトルによる曖昧な検索を一切せず、そのインスタンスだけを対象にする —
        // 同名の複数インスタンスが実行中でも取り違えない（実機で確認済みの不具合の根本
        // 対応: window_title 検索でも、たった今 spawn したはずのウィンドウではなく、
        // たまたま先に見つかった既存の別インスタンスの方を掴んでしまうことがあった）。
        // 見つからなければ（未起動、または pid が既に終了 — 例えば wezterm-gui が
        // 既存の mux に合流した際の、リクエスト用の短命プロセスが該当）、通常の
        // 名前/タイトル検索に委ねる。
        let launched_pid = crate::apps::launched_pid(item_name);
        let cached_app = launched_pid.and_then(|pid| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid as libc::pid_t)
        });
        // 起動直後で、まだ起動したプロセスがアプリとして登録されていない: window_title が
        // 無い（名前だけで区別できない）場合、名前検索に回すと元から動いていた別インスタンス
        // を掴むので、まだ準備中としてリトライさせる。window_title がある場合はタイトル一致が
        // 取り違えを防ぐうえ、既存の mux に合流したケース（ウィンドウは元のプロセスに属する）
        // を拾うために名前検索が必要。
        if post_launch && !window_mode && launched_pid.is_some() && cached_app.is_none() {
            return Ok(ActivateOutcome::NotFound);
        }
        let (apps, strict_count): (Vec<Retained<NSRunningApplication>>, usize) = match cached_app {
            Some(app) => (vec![app], 1),
            None => {
                // 別プロセス登録（`wezterm-gui` 等）はウィンドウ検索でしか使えないので、
                // Accessibility 権限がある場合だけ候補に含める（無ければ従来の名前/bundle
                // 一致のみ）。
                find_running(app_name, window_mode && ax_trusted)
            }
        };
        let Some(app) = apps.first() else {
            return Ok(ActivateOutcome::NotFound);
        };

        if window_mode {
            if ax_trusted {
                return activate_window(
                    &apps,
                    strict_count,
                    app_name,
                    title,
                    title_exclude,
                    toggle,
                    post_launch,
                );
            }
            log::warn!(
                "macos_impl: \"{app_name}\" has window_title/window_title_exclude set but \
                 Accessibility permission was not granted when prompted (or the prompt was \
                 dismissed) — falling back to whole-app activate/toggle. Grant it under System \
                 Settings > Privacy & Security > Accessibility and press the hotkey again."
            );
        }

        activate_whole_app(app, app_name, toggle)
    }

    /// `AXIsProcessTrusted()` に加え、未許可ならシステム標準の許可ダイアログを一度だけ出す
    /// （`AXIsProcessTrustedWithOptions` + `kAXTrustedCheckOptionPrompt`）。ホットキーを
    /// 押すたびにダイアログが出ると鬱陶しいので、プロセスの寿命中は一度だけ試す
    /// （`PROMPTED`）。呼び出し元で `window_mode` のときだけ呼ぶ — `window_title` を
    /// 使わないエントリはこの関数自体に触れず、ダイアログも一切出ない。
    unsafe fn ax_is_trusted_prompting_once() -> bool {
        static PROMPTED: AtomicBool = AtomicBool::new(false);
        if AXIsProcessTrusted() {
            return true;
        }
        if PROMPTED.swap(true, Ordering::AcqRel) {
            return false;
        }
        let keys = [kAXTrustedCheckOptionPrompt as *const c_void];
        let values = [kCFBooleanTrue as *const c_void];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        let trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options as CFTypeRef);
        trusted
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
        // 同じバンドルの別インスタンスが動いていると `open` は別のインスタンスを前面化して
        // しまうため、インスタンス数に応じて `open` / pid 指定を使い分ける。
        activate_specific_instance(app, app_name);
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
        post_launch: bool,
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
                // Right after a launch an enumeration failure just means "not ready yet".
                // Whole-app activation would raise a different pre-existing instance and be
                // taken as success (see `activate_after_launch`), so report NotFound and let
                // the post-launch loop retry.
                if post_launch {
                    return Ok(ActivateOutcome::NotFound);
                }
                // The app is running but AXWindows could not be read; that is not "no matching
                // window". The two obvious fallbacks are both wrong:
                // - NotFound -> launch: duplicates a process we know is running.
                // - whole-app activate: raises whichever window the app had in front (e.g. the
                //   wrong window of a second instance) yet reports Activated, so the failure
                //   is invisible to the user.
                // So report EnumerationFailed: no launch, no activation, a warning in the log,
                // and the user presses again. `activate()` retries once after a short delay to
                // absorb transient AX failures (kAXErrorCannotComplete, AX not ready yet)
                // before giving up. Only a single short retry: unlike the post-launch wait,
                // there is no evidence of how long a running instance takes to recover, and
                // "press again" is cheap.
                log::warn!("macos_impl: AXWindows enumeration failed for \"{app_name}\"");
                return Ok(ActivateOutcome::EnumerationFailed);
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
        activate_specific_instance(app, app_name);
        Ok(ActivateOutcome::Activated)
    }

    /// インスタンスを指定した前面化（`activate_window` / `activate_whole_app` 共通）。
    ///
    /// バックグラウンドの shun から直接の前面化要求（`activateWithOptions()`、
    /// `AXFrontmostAttribute`、shun の子プロセスとして起動した System Events）は、
    /// 戻り値上は成功しても macOS 14+ の協調的アクティベーションで黙って無視される
    /// ことを実機で確認した（LaunchServices 起動の shun.app から 0/7、同じ呼び出しを
    /// ターミナル起動の CLI から行うと 7/7）。
    ///
    /// - 同じバンドルの実行中インスタンスが1つだけ → `open <bundle path>`（LaunchServices
    ///   が前面化を行うため確実、追加権限不要）。
    /// - 複数ある（`wezterm-gui` を bare サブプロセスとして起動した場合など）→ `open` は
    ///   どのインスタンスかを選べず、LaunchServices は元から動いていた方を前面化して
    ///   しまう。`activate_cooperatively`（shun 自身を一度アクティブにしてから対象に
    ///   yield）で pid 指定の前面化を行う。拒否された場合のみ System Events に委譲する。
    fn activate_specific_instance(app: &NSRunningApplication, app_name: &str) {
        let pid = app.processIdentifier();
        let bundle_path = app
            .bundleURL()
            .and_then(|u| u.path())
            .map(|p| p.to_string());
        let same_bundle_count = bundle_path.as_deref().map_or(1, |bp| {
            let running = NSWorkspace::sharedWorkspace().runningApplications();
            (0..running.count())
                .map(|i| running.objectAtIndex(i))
                .filter(|a| {
                    a.bundleURL()
                        .and_then(|u| u.path())
                        .is_some_and(|p| p.to_string() == bp)
                })
                .count()
        });
        if same_bundle_count <= 1 {
            open_bundle(app, app_name);
            return;
        }
        let allowed = activate_cooperatively(app);
        log::info!(
            "macos_impl: cooperative activation of \"{app_name}\" (pid {pid}) allowed={allowed}"
        );
        if !allowed {
            activate_pid_via_system_events(pid, app_name, bundle_path);
        }
    }

    /// macOS 14+ の協調的アクティベーション: 前面化は「現在アクティブなアプリが
    /// yield した相手」に対してしか保証されない（`NSApplication.activate` /
    /// `NSRunningApplication.activateFromApplication:options:` のドキュメント）。
    /// バックグラウンドの shun から他アプリを前面化しようとしても黙って無視されるのは
    /// このため（PR #262 の `activateWithOptions()`、`AXFrontmostAttribute`、shun の子
    /// プロセスとして起動した System Events も同様に exit 0 / true なのに効かないことを
    /// 実機で確認）。ユーザーが shun のホットキーを押した直後なので shun 自身は
    /// アクティブになれる（Ctrl+Space でランチャーを出すのと同じ）— 一度 shun を
    /// アクティブにし、対象に yield してから、shun を起点に対象をアクティブ化する。
    /// メインスレッド専用。
    fn activate_cooperatively(app: &NSRunningApplication) -> bool {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return false;
        };
        let ns_app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        ns_app.activateIgnoringOtherApps(true);
        ns_app.yieldActivationToApplication(app);
        app.activateFromApplication_options(
            &NSRunningApplication::currentApplication(),
            NSApplicationActivationOptions::empty(),
        )
    }

    /// `pid` のプロセスを System Events（`osascript`）経由でフロントにする。shun の
    /// プロセスからの直接の前面化 API は黙って無視されるため、pid 指定で前面化できる
    /// 唯一の信頼できる手段（`activate_specific_instance` のドキュメント参照）。
    /// 結果待ちはメインスレッドを塞がないよう別スレッドで行う（初回の Automation 許可
    /// ダイアログ待ちもあり得る）。失敗時、`fallback_bundle` があれば `open` する。
    fn activate_pid_via_system_events(
        pid: libc::pid_t,
        app_name: &str,
        fallback_bundle: Option<String>,
    ) {
        let script = format!(
            "tell application \"System Events\" to set frontmost of \
             (first process whose unix id is {pid}) to true"
        );
        let child = match std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                log::warn!("macos_impl: failed to spawn osascript: {e}");
                if let Some(path) = fallback_bundle {
                    let _ = std::process::Command::new("open").arg(path).spawn();
                }
                return;
            }
        };
        let app_name = app_name.to_string();
        std::thread::spawn(move || {
            let mut child = child;
            match child.wait() {
                Ok(status) if status.success() => {}
                other => {
                    log::warn!(
                        "macos_impl: System Events activation of \"{app_name}\" (pid {pid}) \
                         failed ({other:?})"
                    );
                    if let Some(path) = fallback_bundle {
                        let _ = std::process::Command::new("open").arg(path).spawn();
                    }
                }
            }
        });
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
                // bundle を持たない別プロセス登録は `open` できない。shun のプロセスからの
                // 直接の前面化 API（`activateWithOptions()` / `AXFrontmostAttribute`）は
                // 実機で黙って無視されることが分かっている（`activate_specific_instance`
                // のドキュメント参照）ので、pid を指定できる System Events に委譲する。
                activate_pid_via_system_events(app.processIdentifier(), app_name, None);
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

/// Linux:
///
/// - **niri**（`NIRI_SOCKET` あり）: `niri msg --json windows` で `app_id`（部分一致・大文字小文字
///   無視）と `window_title` / `window_title_exclude` で照合し、`focus-window --id` で前面化する。
///   `toggle` は対象が既にフォーカスされていれば `focus-window-previous` で直前のウィンドウへ
///   戻す（niri に最小化はない）。Wayland で動き、フォーカス状態も取れる。
/// - **X11**: `wmctrl` があれば `-x -a <app>` でアクティブ化する。`-x` により WM_CLASS
///   (`instance.class`) と部分一致・大文字小文字無視で照合する（タイトルではない）。
///   部分一致のため短い名前は別アプリに誤マッチし得る。`wmctrl` には「現在フォーカスされている
///   ウィンドウか」を安定して判定する手段がないため、`toggle` は `activate` と同じ動作
///   （フォーカスするだけで最小化はしない）にフォールバックする。
/// - それ以外（`wmctrl` なし・他の Wayland コンポジタ）: `Unsupported` を返し、呼び出し側が
///   起動にフォールバックする。
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::{ActivateOutcome, WindowMatch};
    use crate::niri;

    pub(super) fn activate(
        app_name: &str,
        window: WindowMatch,
        toggle: bool,
    ) -> Result<ActivateOutcome, String> {
        if niri::is_active() {
            return Ok(
                match niri::activate(app_name, window.title, window.title_exclude, toggle)? {
                    niri::Outcome::Activated => ActivateOutcome::Activated,
                    niri::Outcome::Hidden => ActivateOutcome::Minimized,
                    niri::Outcome::NotFound => ActivateOutcome::NotFound,
                    niri::Outcome::ActionFailed(e) => ActivateOutcome::ActionFailed(e),
                },
            );
        }
        let output = match crate::child_process::command("wmctrl")
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
    use super::{falls_back_to_launch, resolve_outcome, should_retry_enumeration, ActivateOutcome};

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

    #[test]
    fn only_not_found_and_unsupported_launch() {
        assert!(falls_back_to_launch(&ActivateOutcome::NotFound));
        assert!(falls_back_to_launch(&ActivateOutcome::Unsupported));
        assert!(!falls_back_to_launch(&ActivateOutcome::Activated));
        assert!(!falls_back_to_launch(&ActivateOutcome::Minimized));
        assert!(!falls_back_to_launch(&ActivateOutcome::EnumerationFailed));
    }

    #[test]
    fn enumeration_failure_never_launches() {
        let mut launched = false;
        let r = resolve_outcome(Ok(ActivateOutcome::EnumerationFailed), "app", || {
            launched = true;
            Ok(())
        });
        assert!(r.is_ok());
        assert!(!launched);
    }

    #[test]
    fn not_found_and_error_launch() {
        for result in [
            Ok(ActivateOutcome::NotFound),
            Ok(ActivateOutcome::Unsupported),
            Err("boom".to_string()),
        ] {
            let mut launched = false;
            let _ = resolve_outcome(result, "app", || {
                launched = true;
                Ok(())
            });
            assert!(launched);
        }
    }

    #[test]
    fn activated_does_not_launch() {
        let mut launched = false;
        let _ = resolve_outcome(Ok(ActivateOutcome::Minimized), "app", || {
            launched = true;
            Ok(())
        });
        assert!(!launched);
    }

    #[test]
    fn enumeration_retry_is_single() {
        let failed = Ok(ActivateOutcome::EnumerationFailed);
        assert!(should_retry_enumeration(&failed, 0));
        assert!(!should_retry_enumeration(&failed, 1));
        assert!(!should_retry_enumeration(&Ok(ActivateOutcome::NotFound), 0));
        assert!(!should_retry_enumeration(&Err("x".into()), 0));
    }
}
