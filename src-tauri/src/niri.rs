//! Explicit app-window activation via niri IPC for `hotkey_mode = "activate" | "toggle"`.
//! Optional included-file global-key synchronization is handled separately by `niri_binds`.

use serde_json::Value;
use std::process::Command;

/// Whether the current session is niri (it exports `NIRI_SOCKET` to its children).
pub fn is_active() -> bool {
    std::env::var_os("NIRI_SOCKET").is_some_and(|v| !v.is_empty())
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
