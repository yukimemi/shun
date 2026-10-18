//! niri (Wayland) integration.
//!
//! Wayland gives clients no way to grab global keys, so the X11-based global shortcuts
//! never fire under niri. niri owns the keyboard, so shun hands it the shortcuts instead:
//!
//! 1. [`sync_binds`] renders every shun hotkey as a niri bind into `shun-binds.kdl` next
//!    to niri's `config.kdl` and makes sure `config.kdl` has a single `include` line for
//!    it. niri live-reloads, so editing shun's config is the only thing the user maintains.
//! 2. Each bind spawns shun with CLI arguments. The single-instance plugin forwards
//!    them to the resident instance, or a fresh instance handles them on startup.
//! 3. [`activate`] implements `hotkey_mode = "activate" | "toggle"` over niri's IPC
//!    (`niri msg`), which — unlike `wmctrl` — works on Wayland and knows which window is
//!    focused.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Generated file with shun's binds, kept next to niri's `config.kdl`.
pub const BINDS_FILE: &str = "shun-binds.kdl";
const INCLUDE_LINE: &str = "include \"shun-binds.kdl\"";
/// One-time copy of the user's `config.kdl` taken before shun first edits it.
const BACKUP_FILE: &str = "config.kdl.before-shun";

/// Whether the current session is niri (it exports `NIRI_SOCKET` to its children).
pub fn is_active() -> bool {
    std::env::var_os("NIRI_SOCKET").is_some_and(|v| !v.is_empty())
}

/// One niri bind that triggers a shun action.
pub struct Bind {
    pub shortcut: Shortcut,
    /// Shown in niri's hotkey overlay.
    pub title: String,
    /// shun CLI arguments (`toggle`, `hotkey F10`, ...).
    pub args: Vec<String>,
}

/// niri key name (an xkb keysym name) for a physical key.
fn key_name(code: Code) -> Option<&'static str> {
    use Code::*;
    Some(match code {
        KeyA => "A",
        KeyB => "B",
        KeyC => "C",
        KeyD => "D",
        KeyE => "E",
        KeyF => "F",
        KeyG => "G",
        KeyH => "H",
        KeyI => "I",
        KeyJ => "J",
        KeyK => "K",
        KeyL => "L",
        KeyM => "M",
        KeyN => "N",
        KeyO => "O",
        KeyP => "P",
        KeyQ => "Q",
        KeyR => "R",
        KeyS => "S",
        KeyT => "T",
        KeyU => "U",
        KeyV => "V",
        KeyW => "W",
        KeyX => "X",
        KeyY => "Y",
        KeyZ => "Z",
        Digit0 => "0",
        Digit1 => "1",
        Digit2 => "2",
        Digit3 => "3",
        Digit4 => "4",
        Digit5 => "5",
        Digit6 => "6",
        Digit7 => "7",
        Digit8 => "8",
        Digit9 => "9",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        F21 => "F21",
        F22 => "F22",
        F23 => "F23",
        F24 => "F24",
        Space => "Space",
        Enter => "Return",
        Tab => "Tab",
        Escape => "Escape",
        Backspace => "BackSpace",
        Delete => "Delete",
        Insert => "Insert",
        Home => "Home",
        End => "End",
        PageUp => "Page_Up",
        PageDown => "Page_Down",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        ArrowUp => "Up",
        ArrowDown => "Down",
        Minus => "Minus",
        Equal => "Equal",
        Comma => "Comma",
        Period => "Period",
        Slash => "Slash",
        Semicolon => "Semicolon",
        Quote => "Apostrophe",
        Backquote => "Grave",
        BracketLeft => "BracketLeft",
        BracketRight => "BracketRight",
        Backslash => "Backslash",
        PrintScreen => "Print",
        ScrollLock => "Scroll_Lock",
        Pause => "Pause",
        NumLock => "Num_Lock",
        CapsLock => "Caps_Lock",
        Numpad0 => "KP_0",
        Numpad1 => "KP_1",
        Numpad2 => "KP_2",
        Numpad3 => "KP_3",
        Numpad4 => "KP_4",
        Numpad5 => "KP_5",
        Numpad6 => "KP_6",
        Numpad7 => "KP_7",
        Numpad8 => "KP_8",
        Numpad9 => "KP_9",
        NumpadAdd => "KP_Add",
        NumpadDecimal => "KP_Decimal",
        NumpadDivide => "KP_Divide",
        NumpadEnter => "KP_Enter",
        NumpadEqual => "KP_Equal",
        NumpadMultiply => "KP_Multiply",
        NumpadSubtract => "KP_Subtract",
        _ => return None,
    })
}

/// niri key chord (`Ctrl+Alt+F10`) for `shortcut`, or `None` if its key has no niri name.
pub fn chord(shortcut: &Shortcut) -> Option<String> {
    let key = key_name(shortcut.key)?;
    let mut parts: Vec<&str> = Vec::new();
    if shortcut.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if shortcut.mods.contains(Modifiers::ALT) {
        parts.push("Alt");
    }
    if shortcut.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift");
    }
    if shortcut.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        parts.push("Super");
    }
    parts.push(key);
    Some(parts.join("+"))
}

/// Escapes `s` for use inside a KDL quoted string.
fn kdl_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Renders the contents of [`BINDS_FILE`]. Binds whose key has no niri name are skipped and
/// reported in the returned warnings.
pub fn render_binds(exe: &str, binds: &[Bind]) -> (String, Vec<String>) {
    let mut warnings = Vec::new();
    let mut out = String::from(
        "// Generated by shun from its config.toml (global hotkeys). Do not edit:\n\
         // it is rewritten on start and on /reload.\n\
         binds {\n",
    );
    for b in binds {
        let Some(chord) = chord(&b.shortcut) else {
            warnings.push(format!(
                "niri: key of \"{}\" has no niri key name, skipping",
                b.title
            ));
            continue;
        };
        let command = std::iter::once(exe)
            .chain(b.args.iter().map(String::as_str))
            .map(|arg| format!("\"{}\"", kdl_escape(arg)))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "    {chord} repeat=false hotkey-overlay-title=\"{title}\" {{ spawn {command}; }}\n",
            title = kdl_escape(&b.title),
        ));
    }
    out.push_str("}\n");
    (out, warnings)
}

/// niri's `config.kdl` (honours `NIRI_CONFIG`).
fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NIRI_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    Some(dirs_next::config_dir()?.join("niri").join("config.kdl"))
}

/// Writes `contents` to `path` atomically, only if it differs from what is there
/// (an identical rewrite would still make niri reload its config).
fn write_if_changed(path: &Path, contents: &str) -> Result<bool, String> {
    if std::fs::read_to_string(path).is_ok_and(|cur| cur == contents) {
        return Ok(false);
    }
    let tmp = path.with_extension("kdl.tmp");
    std::fs::write(&tmp, contents).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename to {}: {e}", path.display())
    })?;
    Ok(true)
}

/// `niri validate -c <config>`.
fn validate(config: &Path) -> Result<(), String> {
    let out = Command::new("niri")
        .arg("validate")
        .arg("-c")
        .arg(config)
        .output()
        .map_err(|e| format!("niri validate: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Whether a top-level, active KDL node includes the generated binds.
/// Scan tokens rather than matching lines: comments are whitespace, and semicolons
/// and newlines both terminate nodes. Strings must stay opaque to this scan.
fn has_include(mut src: &str) -> bool {
    let (mut depth, mut position, mut disabled) = (0usize, 0usize, false);
    let mut include = false;
    let mut skip_entry = false;
    while !src.is_empty() {
        if src.starts_with("//") {
            src = src.find('\n').map_or("", |end| &src[end..]);
            continue;
        }
        if let Some(rest) = src.strip_prefix("/*") {
            src = rest;
            let mut comments = 1;
            while !src.is_empty() && comments > 0 {
                if let Some(rest) = src.strip_prefix("/*") {
                    comments += 1;
                    src = rest;
                } else if let Some(rest) = src.strip_prefix("*/") {
                    comments -= 1;
                    src = rest;
                } else {
                    src = &src[src.chars().next().unwrap().len_utf8()..];
                }
            }
            continue;
        }
        let c = src.chars().next().unwrap();
        match c {
            '\n' | '\r' | ';' if depth == 0 => {
                position = 0;
                disabled = false;
                include = false;
            }
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    position = 0;
                    disabled = false;
                    include = false;
                }
            }
            c if c.is_whitespace() => {}
            _ => {
                if src.starts_with("/-") {
                    if position == 0 {
                        disabled = true;
                    } else {
                        skip_entry = true;
                    }
                    src = &src[2..];
                    continue;
                }
                // A KDL line continuation does not terminate the current node.
                if c == '\\' {
                    src = src[1..].trim_start_matches([' ', '\t']);
                    if src.starts_with("//") {
                        src = src.find('\n').map_or("", |end| &src[end..]);
                    }
                    src = src
                        .strip_prefix("\r\n")
                        .or_else(|| src.strip_prefix(['\n', '\r']))
                        .unwrap_or(src);
                    continue;
                }
                let (token, rest) = kdl_token(src);
                src = rest;
                if skip_entry {
                    skip_entry = false;
                    continue;
                }
                if depth == 0 && !disabled {
                    if position == 0 {
                        include = token == "include";
                    } else if position == 1 && include && token == BINDS_FILE {
                        return true;
                    }
                    position += 1;
                }
                continue;
            }
        }
        src = &src[c.len_utf8()..];
    }
    false
}

/// Consume a KDL v1 quoted/raw string or bare token. Keeping whole strings opaque
/// prevents apparent nodes or comment markers inside a string from being interpreted.
fn kdl_token(src: &str) -> (String, &str) {
    let hashes = src
        .strip_prefix('r')
        .map(|s| s.bytes().take_while(|b| *b == b'#').count());
    if let Some(hashes) = hashes {
        let start = 1 + hashes;
        if src[start..].starts_with('"') {
            let end_marker = format!("\"{}", "#".repeat(hashes));
            let value = &src[start + 1..];
            return match value.find(&end_marker) {
                Some(end) => (value[..end].into(), &value[end + end_marker.len()..]),
                None => (String::new(), ""),
            };
        }
    }
    if let Some(mut rest) = src.strip_prefix('"') {
        let mut value = String::new();
        while let Some(c) = rest.chars().next() {
            rest = &rest[c.len_utf8()..];
            match c {
                '"' => return (value, rest),
                '\\' => {
                    let Some(escaped) = rest.chars().next() else {
                        break;
                    };
                    rest = &rest[escaped.len_utf8()..];
                    if escaped == 'u' && rest.starts_with('{') {
                        if let Some(end) = rest.find('}') {
                            if let Some(c) = u32::from_str_radix(&rest[1..end], 16)
                                .ok()
                                .and_then(char::from_u32)
                            {
                                value.push(c);
                            }
                            rest = &rest[end + 1..];
                        }
                    } else {
                        value.push(match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            'b' => '\u{8}',
                            'f' => '\u{c}',
                            c => c,
                        });
                    }
                }
                c => value.push(c),
            }
        }
        return (String::new(), "");
    }
    let end = src
        .char_indices()
        .find_map(|(i, c)| {
            (c.is_whitespace() || matches!(c, ';' | '{' | '}' | '/' | '=')).then_some(i)
        })
        .unwrap_or(src.len())
        .max(src.chars().next().unwrap().len_utf8());
    (src[..end].into(), &src[end..])
}

/// Adds the `include` line to niri's `config.kdl` once. The file is appended to in place (a
/// symlinked/managed config keeps its link) and a one-time backup is kept.
/// Validate a temporary candidate before appending; never restore a user-owned file.
fn ensure_include(
    config: &Path,
    validate: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let original =
        std::fs::read_to_string(config).map_err(|e| format!("read {}: {e}", config.display()))?;
    if has_include(&original) {
        return validate(config);
    }
    let mut updated = original.clone();
    if !updated.ends_with('\n') && !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str("// shun global hotkeys (generated by shun, see shun-binds.kdl)\n");
    updated.push_str(INCLUDE_LINE);
    updated.push('\n');
    // Validate a sibling so relative includes resolve normally. Never roll back
    // config.kdl: an editor may save user changes while validation is running.
    let staged = tempfile::Builder::new()
        .prefix(".shun-validate-")
        .suffix(".kdl")
        .tempfile_in(config.parent().unwrap_or(Path::new(".")))
        .map_err(|e| format!("stage niri config: {e}"))?;
    std::fs::write(staged.path(), &updated).map_err(|e| format!("stage niri config: {e}"))?;
    validate(staged.path())?;
    if std::fs::read_to_string(config).map_err(|e| format!("read niri config: {e}"))? != original {
        return Err("niri config changed during validation; retry sync".into());
    }
    let backup = config.with_file_name(BACKUP_FILE);
    if !backup.exists() {
        std::fs::write(&backup, &original)
            .map_err(|e| format!("write backup {}: {e}", backup.display()))?;
    }
    // Append only our suffix; do not truncate an editor's concurrent save.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(config)
        .and_then(|mut file| file.write_all(&updated.as_bytes()[original.len()..]))
        .map_err(|e| format!("append {}: {e}", config.display()))?;
    Ok(())
}

/// Validate every sync and restore generated binds on failure.
/// Serialize background syncs so another update cannot race validation or rollback.
fn sync_at(
    config: &Path,
    contents: &str,
    enabled: bool,
    validate: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    static SYNC_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = SYNC_LOCK
        .lock()
        .map_err(|e| format!("niri sync lock: {e}"))?;
    let target = config.with_file_name(BINDS_FILE);
    let previous = match std::fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("read {}: {e}", target.display())),
    };
    // Opting out must not install any files or add an include.
    if !enabled && previous.is_none() {
        return Ok(());
    }
    let result = (|| {
        write_if_changed(&target, contents)?;
        if enabled {
            ensure_include(config, validate)
        } else {
            validate(config)
        }
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        let restored = match previous {
            Some(bytes) => std::fs::write(&target, bytes),
            None => match std::fs::remove_file(&target) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                result => result,
            },
        };
        if let Err(e) = restored {
            failures.push(format!("restore binds: {e}"));
        }
        return if failures.is_empty() {
            Err(format!(
                "niri sync rejected, generated binds restored: {error}"
            ))
        } else {
            Err(format!(
                "niri sync failed: {error}; {}",
                failures.join("; ")
            ))
        };
    }
    Ok(())
}

/// Brings niri's binds in line with `binds`. Returns warnings for binds that were skipped.
pub fn sync_binds(exe: &str, binds: &[Bind]) -> Result<Vec<String>, String> {
    let config = config_path().ok_or("cannot resolve niri config path")?;
    if !config.exists() {
        return Err(format!(
            "{} does not exist (niri is using its built-in config); create it to enable shun binds",
            config.display()
        ));
    }
    let (contents, warnings) = render_binds(exe, binds);
    sync_at(&config, &contents, true, validate)?;
    Ok(warnings)
}

/// Leaves the include valid but removes every generated shortcut on opt-out.
pub fn clear_binds() -> Result<(), String> {
    let config = config_path().ok_or("cannot resolve niri config path")?;
    let (contents, _) = render_binds("", &[]);
    sync_at(&config, &contents, false, validate)
}

/// A niri window as reported by `niri msg --json windows`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: u64,
    pub app_id: String,
    pub title: String,
    pub focused: bool,
    /// Monotonic last-focus time, `(secs, nanos)`; `None` if never focused.
    pub focus_time: Option<(u64, u64)>,
}

fn parse_windows(json: &str) -> Result<Vec<Window>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("niri windows json: {e}"))?;
    let arr = v.as_array().ok_or("niri windows json: not an array")?;
    Ok(arr
        .iter()
        .filter_map(|w| {
            Some(Window {
                id: w.get("id")?.as_u64()?,
                app_id: w
                    .get("app_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                title: w
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                focused: w
                    .get("is_focused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                focus_time: w
                    .get("focus_timestamp")
                    .and_then(|t| Some((t.get("secs")?.as_u64()?, t.get("nanos")?.as_u64()?))),
            })
        })
        .collect())
}

/// Whether `w` is a window for `app` (case-insensitive substring of the `app_id`, like the
/// `wmctrl -x` matching it replaces) that also satisfies the title conditions.
fn matches(w: &Window, app: &str, title: Option<&str>, title_exclude: Option<&str>) -> bool {
    if !w.app_id.to_lowercase().contains(&app.to_lowercase()) {
        return false;
    }
    let t = w.title.to_lowercase();
    title.is_none_or(|s| t.contains(&s.to_lowercase()))
        && title_exclude.is_none_or(|s| !t.contains(&s.to_lowercase()))
}

/// What [`activate`] did.
pub enum Outcome {
    Activated,
    /// The target was focused, so focus went back to the previous window (toggle only).
    Hidden,
    NotFound,
    /// The window was found but the niri action on it failed (e.g. it closed meanwhile).
    /// The app is running, so the caller must not launch another instance.
    ActionFailed(String),
}

fn niri_msg(args: &[&str]) -> Result<String, String> {
    let out = Command::new("niri")
        .arg("msg")
        .args(args)
        .output()
        .map_err(|e| format!("niri msg: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Picks the window to act on and what to do, as a pure function of niri's window list.
/// `Some((id, hide))`: `hide` means "the target is already focused and `toggle` is set".
fn choose(
    windows: &[Window],
    app: &str,
    title: Option<&str>,
    title_exclude: Option<&str>,
    toggle: bool,
) -> Option<(u64, bool)> {
    let mut found: Vec<&Window> = windows
        .iter()
        .filter(|w| matches(w, app, title, title_exclude))
        .collect();
    if found.is_empty() {
        return None;
    }
    if toggle {
        if let Some(w) = found.iter().find(|w| w.focused) {
            return Some((w.id, true));
        }
    }
    // Most recently focused first; never-focused windows last.
    found.sort_by_key(|w| std::cmp::Reverse(w.focus_time));
    Some((found[0].id, false))
}

/// Focuses the window of `app` (see [`matches`]); with `toggle`, a focused target is instead
/// hidden by returning focus to the previously focused window (niri has no minimize).
pub fn activate(
    app: &str,
    title: Option<&str>,
    title_exclude: Option<&str>,
    toggle: bool,
) -> Result<Outcome, String> {
    let windows = parse_windows(&niri_msg(&["--json", "windows"])?)?;
    match choose(&windows, app, title, title_exclude, toggle) {
        None => Ok(Outcome::NotFound),
        Some((_, true)) => Ok(match niri_msg(&["action", "focus-window-previous"]) {
            Ok(_) => Outcome::Hidden,
            Err(e) => Outcome::ActionFailed(e),
        }),
        Some((id, false)) => Ok(
            match niri_msg(&["action", "focus-window", "--id", &id.to_string()]) {
                Ok(_) => Outcome::Activated,
                Err(e) => Outcome::ActionFailed(e),
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(s: &str) -> Shortcut {
        s.parse().unwrap()
    }

    #[test]
    fn chord_maps_modifiers_and_keys() {
        assert_eq!(chord(&sc("F10")).as_deref(), Some("F10"));
        assert_eq!(chord(&sc("Ctrl+Space")).as_deref(), Some("Ctrl+Space"));
        assert_eq!(chord(&sc("Ctrl+Enter")).as_deref(), Some("Ctrl+Return"));
        assert_eq!(
            chord(&sc("Super+Shift+A")).as_deref(),
            Some("Shift+Super+A")
        );
        assert_eq!(chord(&sc("Alt+PageUp")).as_deref(), Some("Alt+Page_Up"));
        assert_eq!(
            chord(&sc("Ctrl+Alt+Shift+`")).as_deref(),
            Some("Ctrl+Alt+Shift+Grave")
        );
        assert_eq!(chord(&sc("Cmd+1")).as_deref(), Some("Super+1"));
    }

    #[test]
    fn render_escapes_for_kdl_and_skips_unmappable_keys() {
        let binds = vec![
            Bind {
                shortcut: sc("F9"),
                title: "shun: \"Edge\"".into(),
                args: vec!["hotkey".into(), "F9".into()],
            },
            Bind {
                shortcut: sc("Ctrl+CapsLock"),
                title: "ok".into(),
                args: vec!["toggle".into()],
            },
            Bind {
                shortcut: Shortcut::new(None, Code::Convert),
                title: "unmappable".into(),
                args: vec!["toggle".into()],
            },
        ];
        let (out, warnings) = render_binds("/x/shun", &binds);
        assert!(out.contains("F9 repeat=false hotkey-overlay-title=\"shun: \\\"Edge\\\"\""));
        assert!(out.contains("Ctrl+Caps_Lock"));
        assert!(!out.contains("unmappable"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unmappable"));
        assert!(out.trim_end().ends_with('}'));
    }

    #[test]
    fn has_include_ignores_comments() {
        assert!(has_include("a\ninclude \"shun-binds.kdl\"\n"));
        assert!(has_include("  include \"shun-binds.kdl\"  "));
        assert!(!has_include("// include \"shun-binds.kdl\"\n"));
        assert!(!has_include("include \"other.kdl\"\n"));
        assert!(!has_include("/*\ninclude \"shun-binds.kdl\"\n*/\n"));
        assert!(!has_include(
            "/* a /* b */\ninclude \"shun-binds.kdl\"\n*/\n"
        ));
        assert!(has_include("/* x */\ninclude \"shun-binds.kdl\"\n"));
    }

    #[test]
    fn include_detection_accepts_kdl_syntax_and_stays_idempotent() {
        for syntax in [
            "include \"shun-binds.kdl\";",
            "include \"shun-binds.kdl\" // hotkeys\n",
            "include \"shun-binds.kdl\"; // hotkeys\n",
            "include /* comment */ \"shun-binds.kdl\" /* trailing */;",
            "include\t\"shun-binds.kdl\";",
            "include /- \"unused.kdl\" \"shun-binds.kdl\";",
            "include \\\n \"shun-binds.kdl\";",
            "other; include \"shun-binds.kdl\"; other",
            "\"include\" r#\"shun-binds.kdl\"#;",
            "include \"shun\\u{2d}binds.kdl\";",
            "// \" /*\ninclude \"shun-binds.kdl\";",
        ] {
            assert!(has_include(syntax), "{syntax}");
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("config.kdl");
            std::fs::write(&config, syntax).unwrap();
            for _ in 0..2 {
                sync_at(&config, "binds {}", true, |_| Ok(())).unwrap();
                assert_eq!(std::fs::read_to_string(&config).unwrap(), syntax);
            }
            assert!(!dir.path().join(BACKUP_FILE).exists());
        }
        for syntax in [
            "/- include \"shun-binds.kdl\";",
            "other { include \"shun-binds.kdl\"; }",
            "value r#\"\ninclude \"shun-binds.kdl\"\n\"#;",
            "value \"\ninclude \\\"shun-binds.kdl\\\"\n\";",
        ] {
            assert!(!has_include(syntax), "{syntax}");
        }
    }

    #[test]
    fn sync_preserves_edits_during_validation() {
        for installed in [false, true] {
            for valid in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let config = dir.path().join("config.kdl");
                let target = dir.path().join(BINDS_FILE);
                let initial = if installed {
                    INCLUDE_LINE
                } else {
                    "// user config\n"
                };
                std::fs::write(&config, initial).unwrap();
                std::fs::write(&target, "old binds").unwrap();
                let edited = format!("{initial}\n// saved during validation\n");
                let result = sync_at(&config, "new binds", true, |_| {
                    std::fs::write(&config, &edited).unwrap();
                    if valid {
                        Ok(())
                    } else {
                        Err("invalid".into())
                    }
                });
                assert_eq!(std::fs::read_to_string(&config).unwrap(), edited);
                if installed && valid {
                    result.unwrap();
                } else {
                    assert!(result.is_err());
                    assert_eq!(std::fs::read_to_string(&target).unwrap(), "old binds");
                }
            }
        }
    }

    #[test]
    fn sync_preserves_config_replaced_by_editor_during_validation() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.kdl");
        std::fs::write(&config, "// original\n").unwrap();
        assert!(sync_at(&config, "bad binds", true, |_| {
            let saved = dir.path().join("saved.kdl");
            std::fs::write(&saved, "// editor replacement\n").unwrap();
            std::fs::rename(saved, &config).unwrap();
            Err("invalid".into())
        })
        .is_err());
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "// editor replacement\n"
        );
        assert!(!dir.path().join(BINDS_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn first_install_preserves_config_symlink_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let managed = dir.path().join("managed.kdl");
        let config = dir.path().join("config.kdl");
        let original = "// managed config without trailing newline";
        std::fs::write(&managed, original).unwrap();
        std::os::unix::fs::symlink(&managed, &config).unwrap();
        sync_at(&config, "binds {}", true, |_| Ok(())).unwrap();
        sync_at(&config, "binds {}", true, |_| Ok(())).unwrap();
        assert!(config.is_symlink());
        let installed = std::fs::read_to_string(managed).unwrap();
        assert!(installed.starts_with(original));
        assert_eq!(installed.matches(INCLUDE_LINE).count(), 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(BACKUP_FILE)).unwrap(),
            original
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 4);
    }

    #[test]
    fn render_spawns_executable_and_arguments_without_shell() {
        let (out, _) = render_binds(
            "/opt/my apps/it's \"shun\"",
            &[Bind {
                shortcut: sc("F9"),
                title: "test".into(),
                args: vec!["hotkey".into(), "Ctrl+F9".into()],
            }],
        );
        assert!(out.contains(r#"spawn "/opt/my apps/it's \"shun\"" "hotkey" "Ctrl+F9";"#));
    }

    #[test]
    fn sync_validates_existing_include_and_restores_rejected_binds() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.kdl");
        let target = dir.path().join(BINDS_FILE);
        std::fs::write(&config, INCLUDE_LINE).unwrap();
        std::fs::write(&target, "binds {}\n").unwrap();
        let result = sync_at(&config, "conflicting binds", true, |path| {
            assert_eq!(path, config);
            assert_eq!(
                std::fs::read_to_string(&target).unwrap(),
                "conflicting binds"
            );
            Err("duplicate bind".into())
        });
        assert!(result.unwrap_err().contains("generated binds restored"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "binds {}\n");
        assert_eq!(std::fs::read_to_string(&config).unwrap(), INCLUDE_LINE);
        sync_at(&config, "new binds", true, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new binds");
    }

    #[test]
    fn failed_first_install_leaves_config_untouched_and_restores_target() {
        for existing in [None, Some("previous binds")] {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("config.kdl");
            let target = dir.path().join(BINDS_FILE);
            std::fs::write(&config, "// user config\n").unwrap();
            if let Some(contents) = existing {
                std::fs::write(&target, contents).unwrap();
            }
            assert!(sync_at(&config, "bad binds", true, |path| {
                assert!(has_include(&std::fs::read_to_string(path).unwrap()));
                assert_eq!(
                    std::fs::read_to_string(&config).unwrap(),
                    "// user config\n"
                );
                Err("invalid".into())
            })
            .is_err());
            assert_eq!(
                std::fs::read_to_string(&config).unwrap(),
                "// user config\n"
            );
            assert_eq!(std::fs::read_to_string(&target).ok().as_deref(), existing);
        }
    }

    #[test]
    fn opt_out_clears_binds_and_can_be_reenabled() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.kdl");
        let target = dir.path().join(BINDS_FILE);
        std::fs::write(&config, "// user config\n").unwrap();
        sync_at(&config, "enabled binds", true, |_| Ok(())).unwrap();
        let installed = std::fs::read_to_string(&config).unwrap();
        let (empty, _) = render_binds("", &[]);
        sync_at(&config, &empty, false, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), empty);
        assert!(!empty.contains("spawn"));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), installed);
        sync_at(&config, "enabled again", true, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "enabled again");
    }

    #[test]
    fn opt_out_without_prior_install_does_not_create_files() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.kdl");
        sync_at(&config, "binds {}", false, |_| {
            panic!("nothing to validate")
        })
        .unwrap();
        assert!(!config.exists());
        assert!(!dir.path().join(BINDS_FILE).exists());
    }

    #[test]
    #[ignore = "requires the niri executable"]
    fn sync_with_real_niri_validator() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.kdl");
        let target = dir.path().join(BINDS_FILE);
        let user_config = "binds { F9 { spawn \"user-app\"; }; }\n";
        std::fs::write(&config, user_config).unwrap();
        let bind = Bind {
            shortcut: sc("F10"),
            title: "test".into(),
            args: vec!["toggle".into()],
        };
        let (valid, _) = render_binds("/opt/my apps/it's \"shun\"", &[bind]);
        sync_at(&config, &valid, true, validate).unwrap();
        let duplicate = || Bind {
            shortcut: sc("F9"),
            title: "conflict".into(),
            args: vec!["toggle".into()],
        };
        let (invalid, _) = render_binds("/opt/shun", &[duplicate(), duplicate()]);
        assert!(sync_at(&config, &invalid, true, validate).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), valid);
        validate(&config).unwrap();
        let (empty, _) = render_binds("", &[]);
        sync_at(&config, &empty, false, validate).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), empty);
        validate(&config).unwrap();
    }

    #[test]
    #[ignore = "requires the niri executable"]
    fn existing_include_syntax_with_real_niri_validator() {
        for include in [
            "include \"shun-binds.kdl\";",
            "include \"shun-binds.kdl\" // hotkeys\n",
            "include /* hotkeys */ \"shun-binds.kdl\"; // trailing\n",
            "include r#\"shun-binds.kdl\"#;",
            "include /- \"unused.kdl\" \"shun-binds.kdl\";",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("config.kdl");
            std::fs::write(&config, include).unwrap();
            let (binds, _) = render_binds(
                "shun",
                &[Bind {
                    shortcut: sc("F10"),
                    title: "test".into(),
                    args: vec!["toggle".into()],
                }],
            );
            for _ in 0..2 {
                sync_at(&config, &binds, true, validate).unwrap();
                assert_eq!(std::fs::read_to_string(&config).unwrap(), include);
            }
        }
    }

    fn win(id: u64, app_id: &str, title: &str, focused: bool, t: Option<(u64, u64)>) -> Window {
        Window {
            id,
            app_id: app_id.into(),
            title: title.into(),
            focused,
            focus_time: t,
        }
    }

    #[test]
    fn parse_windows_reads_niri_json() {
        let json = r#"[
          {"id":5,"title":"a","app_id":"brave-browser","is_focused":false,
           "focus_timestamp":{"secs":10,"nanos":5}},
          {"id":2,"title":null,"app_id":null,"is_focused":true,"focus_timestamp":null},
          {"title":"no id"}
        ]"#;
        let w = parse_windows(json).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], win(5, "brave-browser", "a", false, Some((10, 5))));
        assert_eq!(w[1], win(2, "", "", true, None));
        assert!(parse_windows("{}").is_err());
    }

    #[test]
    fn matches_uses_substring_app_id_and_title_filters() {
        let w = win(1, "brave-browser", "Docs - Brave", false, None);
        assert!(matches(&w, "brave", None, None));
        assert!(matches(&w, "BRAVE", Some("docs"), None));
        assert!(!matches(&w, "brave", Some("mail"), None));
        assert!(!matches(&w, "brave", None, Some("docs")));
        assert!(!matches(&w, "firefox", None, None));
    }

    #[test]
    fn choose_activates_most_recent_and_toggles_when_focused() {
        let ws = vec![
            win(1, "neovide", "a", false, Some((5, 0))),
            win(2, "neovide", "b", false, Some((9, 0))),
            win(3, "neovide", "c", false, None),
            win(4, "other", "d", true, Some((99, 0))),
        ];
        assert_eq!(choose(&ws, "neovide", None, None, false), Some((2, false)));
        assert_eq!(choose(&ws, "neovide", None, None, true), Some((2, false)));
        assert_eq!(choose(&ws, "missing", None, None, true), None);

        let focused = vec![
            win(1, "neovide", "a", false, Some((5, 0))),
            win(2, "neovide", "b", true, Some((9, 0))),
        ];
        assert_eq!(
            choose(&focused, "neovide", None, None, true),
            Some((2, true))
        );
        // Plain activate never hides.
        assert_eq!(
            choose(&focused, "neovide", None, None, false),
            Some((2, false))
        );
    }
}
