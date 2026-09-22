//! Windows 低レベルキーボードフック (`WH_KEYBOARD_LL`) によるホットキー。
//!
//! `RegisterHotKey`（tauri-plugin-global-shortcut が使う API）が拒否するキー、代表的には
//! OS がデバッガ用に予約している修飾なしの F12 を、AutoHotkey と同じ方式で捕まえる
//! フォールバック。一致したキー入力は握りつぶす（前面アプリには届かない）。
//!
//! フックは初回登録時に専用スレッドで1度だけ設置し、プロセス終了まで保持する。
//! config リロード時は `clear()` でバインディングだけ入れ替える。

use std::sync::{mpsc, Arc, LazyLock, Mutex};

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub type Callback = Arc<dyn Fn() + Send + Sync>;

struct Binding {
    vk: u16,
    mods: Modifiers,
    callback: Callback,
}

struct State {
    bindings: Vec<Binding>,
    /// keydown を握りつぶした VK。対応する keyup も握りつぶし、オートリピートでの
    /// 多重発火を防ぐ。
    held: Vec<u16>,
}

static STATE: Mutex<State> = Mutex::new(State {
    bindings: Vec::new(),
    held: Vec::new(),
});
/// フックスレッドの設置結果。初回参照時に1度だけ設置する。
static HOOK: LazyLock<Result<(), String>> = LazyLock::new(install_hook);

/// `shortcut` をフック経由で登録する。押下時に `callback` を別スレッドで呼ぶ。
pub fn register(shortcut: Shortcut, callback: Callback) -> Result<(), String> {
    let vk = code_to_vk(shortcut.key).ok_or_else(|| {
        format!(
            "key {:?} is not supported by the keyboard hook",
            shortcut.key
        )
    })?;
    ensure_hook()?;
    STATE.lock().unwrap().bindings.push(Binding {
        vk,
        mods: shortcut.mods,
        callback,
    });
    Ok(())
}

/// 全バインディングを解除する（フック自体は残す）。
pub fn clear() {
    STATE.lock().unwrap().bindings.clear();
}

fn ensure_hook() -> Result<(), String> {
    HOOK.clone()
}

fn install_hook() -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("shun-kbhook".into())
        .spawn(move || unsafe {
            let hmod = GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0));
            if let Err(e) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0) {
                let _ = tx.send(Err(format!("SetWindowsHookExW failed: {e}")));
                return;
            }
            let _ = tx.send(Ok(()));
            // LL フックは設置スレッドのメッセージループ上で呼ばれる
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let msg = wparam.0 as u32;
        let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if (down || up) && handle_key(kb.vkCode as u16, down) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// キーイベントを処理し、握りつぶすべきなら `true` を返す。
/// フックはタイムアウトがあるため、コールバックは別スレッドで実行する。
fn handle_key(vk: u16, down: bool) -> bool {
    let Ok(mut state) = STATE.lock() else {
        return false;
    };
    if let Some(i) = state.held.iter().position(|&v| v == vk) {
        if !down {
            state.held.swap_remove(i);
        }
        return true; // keyup もしくはオートリピート
    }
    if !down {
        return false;
    }
    let mods = current_mods();
    let Some(binding) = state.bindings.iter().find(|b| b.vk == vk && b.mods == mods) else {
        return false;
    };
    let callback = Arc::clone(&binding.callback);
    state.held.push(vk);
    drop(state);
    log::debug!("kbhook: vk {vk:#x} mods {mods:?} matched, dispatching");
    std::thread::spawn(move || callback());
    true
}

fn current_mods() -> Modifiers {
    let mut mods = Modifiers::empty();
    if pressed(VK_CONTROL) {
        mods |= Modifiers::CONTROL;
    }
    if pressed(VK_MENU) {
        mods |= Modifiers::ALT;
    }
    if pressed(VK_SHIFT) {
        mods |= Modifiers::SHIFT;
    }
    if pressed(VK_LWIN) || pressed(VK_RWIN) {
        mods |= Modifiers::SUPER;
    }
    mods
}

fn pressed(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 }
}

fn code_to_vk(code: Code) -> Option<u16> {
    use Code::*;
    let vk = match code {
        F1 => 0x70,
        F2 => 0x71,
        F3 => 0x72,
        F4 => 0x73,
        F5 => 0x74,
        F6 => 0x75,
        F7 => 0x76,
        F8 => 0x77,
        F9 => 0x78,
        F10 => 0x79,
        F11 => 0x7A,
        F12 => 0x7B,
        F13 => 0x7C,
        F14 => 0x7D,
        F15 => 0x7E,
        F16 => 0x7F,
        F17 => 0x80,
        F18 => 0x81,
        F19 => 0x82,
        F20 => 0x83,
        F21 => 0x84,
        F22 => 0x85,
        F23 => 0x86,
        F24 => 0x87,
        KeyA => 0x41,
        KeyB => 0x42,
        KeyC => 0x43,
        KeyD => 0x44,
        KeyE => 0x45,
        KeyF => 0x46,
        KeyG => 0x47,
        KeyH => 0x48,
        KeyI => 0x49,
        KeyJ => 0x4A,
        KeyK => 0x4B,
        KeyL => 0x4C,
        KeyM => 0x4D,
        KeyN => 0x4E,
        KeyO => 0x4F,
        KeyP => 0x50,
        KeyQ => 0x51,
        KeyR => 0x52,
        KeyS => 0x53,
        KeyT => 0x54,
        KeyU => 0x55,
        KeyV => 0x56,
        KeyW => 0x57,
        KeyX => 0x58,
        KeyY => 0x59,
        KeyZ => 0x5A,
        Digit0 => 0x30,
        Digit1 => 0x31,
        Digit2 => 0x32,
        Digit3 => 0x33,
        Digit4 => 0x34,
        Digit5 => 0x35,
        Digit6 => 0x36,
        Digit7 => 0x37,
        Digit8 => 0x38,
        Digit9 => 0x39,
        Space => 0x20,
        Enter => 0x0D,
        Escape => 0x1B,
        Tab => 0x09,
        Backspace => 0x08,
        Insert => 0x2D,
        Delete => 0x2E,
        Home => 0x24,
        End => 0x23,
        PageUp => 0x21,
        PageDown => 0x22,
        ArrowLeft => 0x25,
        ArrowUp => 0x26,
        ArrowRight => 0x27,
        ArrowDown => 0x28,
        Pause => 0x13,
        PrintScreen => 0x2C,
        ScrollLock => 0x91,
        _ => return None,
    };
    Some(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_keys_map_to_contiguous_vks() {
        assert_eq!(code_to_vk(Code::F1), Some(0x70));
        assert_eq!(code_to_vk(Code::F12), Some(0x7B));
        assert_eq!(code_to_vk(Code::F24), Some(0x87));
    }

    #[test]
    fn unmapped_key_is_rejected() {
        assert!(register("Ctrl+Numpad5".parse().unwrap(), Arc::new(|| {})).is_err());
    }
}
