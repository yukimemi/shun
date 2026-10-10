//! Diagnostics only: startup/spawn environment records and log pruning.
//!
//! Nothing here modifies the process environment or the environment passed to
//! launched apps; it only reads a fixed allowlist of variables for logging.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The only environment variables ever written to the log (the full
/// environment can hold secrets).
pub const ENV_ALLOWLIST: &[&str] = &[
    "NO_COLOR",
    "TERM",
    "COLORTERM",
    "XPC_SERVICE_NAME",
    "SHELL",
    "LANG",
];

const MAX_CMDLINE_LEN: usize = 512;

/// `NAME="value"` pairs (or `NAME=<unset>`) for the allowlist, using `get` as the lookup.
pub fn format_env_allowlist(get: impl Fn(&str) -> Option<String>) -> String {
    ENV_ALLOWLIST
        .iter()
        .map(|k| match get(k) {
            Some(v) => format!("{k}={v:?}"),
            None => format!("{k}=<unset>"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn env_allowlist_now() -> String {
    format_env_allowlist(|k| std::env::var_os(k).map(|v| v.to_string_lossy().into_owned()))
}

fn truncate_cmdline(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= MAX_CMDLINE_LEN {
        return s.to_string();
    }
    let cut: String = s.chars().take(MAX_CMDLINE_LEN).collect();
    format!("{cut}…")
}

#[cfg(unix)]
fn ps_field(field: &str, pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Best effort: `(ppid, parent command line)`. Never fails.
fn parent_info() -> (Option<u32>, Option<String>) {
    #[cfg(unix)]
    {
        let ppid = ps_field("ppid", std::process::id()).and_then(|s| s.parse::<u32>().ok());
        let cmd = ppid
            .and_then(|p| ps_field("command", p))
            .map(|s| truncate_cmdline(&s));
        (ppid, cmd)
    }
    #[cfg(not(unix))]
    {
        (None, None)
    }
}

/// One info line describing how this instance was started.
pub fn log_startup_environment() {
    use std::io::IsTerminal;
    let (ppid, parent_cmd) = parent_info();
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unknown: {e}>"));
    log::info!(
        "startup: pid={} ppid={} parent_cmd={} exe={} stdin_tty={} stdout_tty={} env: {}",
        std::process::id(),
        ppid.map_or("<unknown>".to_string(), |p| p.to_string()),
        parent_cmd.map_or("<unknown>".to_string(), |c| format!("{c:?}")),
        exe,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        env_allowlist_now(),
    );
}

/// Log what the update path is about to spawn; the child inherits the current environment.
pub fn log_update_spawn(label: &str, exe: &str, args: &[String]) {
    log::info!(
        "update: about to spawn ({label}) by pid={}: exe={exe:?} args={args:?}; child inherits env: {}",
        std::process::id(),
        env_allowlist_now(),
    );
}

/// Log that this instance is exiting as part of an update.
pub fn log_update_exit(label: &str) {
    log::info!(
        "update: old instance pid={} exiting ({label})",
        std::process::id()
    );
}

/// Pure: which `(path, mtime)` entries are older than `max_age` at `now`.
pub fn select_stale(
    entries: &[(PathBuf, SystemTime)],
    now: SystemTime,
    max_age: Duration,
) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|(_, m)| now.duration_since(*m).map(|d| d > max_age).unwrap_or(false))
        .map(|(p, _)| p.clone())
        .collect()
}

/// Is `name` a rotated log of `stem` (`<stem>_<date>.log`, optionally `.bak`)?
/// The active `<stem>.log` is never a match.
pub fn is_rotated_log_name(stem: &str, name: &str) -> bool {
    let Some(rest) = name.strip_prefix(stem).and_then(|r| r.strip_prefix('_')) else {
        return false;
    };
    rest.ends_with(".log") || rest.ends_with(".log.bak")
}

/// Delete rotated logs in `dir` older than `max_age`. Best effort; returns the number removed.
pub fn prune_old_logs(dir: &Path, stem: &str, max_age: Duration) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let entries: Vec<(PathBuf, SystemTime)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| is_rotated_log_name(stem, &e.file_name().to_string_lossy()))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    let mut removed = 0;
    for p in select_stale(&entries, SystemTime::now(), max_age) {
        if std::fs::remove_file(&p).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_allowlist_formats_set_and_unset() {
        let s = format_env_allowlist(|k| match k {
            "NO_COLOR" => Some("1".into()),
            "TERM" => Some("dumb".into()),
            _ => None,
        });
        assert!(s.contains("NO_COLOR=\"1\""));
        assert!(s.contains("TERM=\"dumb\""));
        assert!(s.contains("COLORTERM=<unset>"));
        assert!(s.contains("XPC_SERVICE_NAME=<unset>"));
    }

    #[test]
    fn env_allowlist_ignores_other_vars() {
        let s = format_env_allowlist(|k| Some(format!("v-{k}")));
        assert_eq!(s.matches('=').count(), ENV_ALLOWLIST.len());
        assert!(!s.contains("SECRET"));
    }

    #[test]
    fn truncate_cmdline_limits_length() {
        let long = "a".repeat(2000);
        let t = truncate_cmdline(&long);
        assert_eq!(t.chars().count(), MAX_CMDLINE_LEN + 1);
        assert_eq!(truncate_cmdline("  short  "), "short");
    }

    #[test]
    fn rotated_name_matching() {
        assert!(is_rotated_log_name("shun", "shun_2026-10-04_12-00-00.log"));
        assert!(is_rotated_log_name(
            "shun",
            "shun_2026-10-04_12-00-00.log.bak"
        ));
        assert!(!is_rotated_log_name("shun", "shun.log"));
        assert!(!is_rotated_log_name("shun", "other_2026.log"));
        assert!(!is_rotated_log_name("shun", "shun_notes.txt"));
    }

    #[test]
    fn select_stale_by_age() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 86400);
        let day = Duration::from_secs(86400);
        let entries = vec![
            (PathBuf::from("old"), now - day * 31),
            (PathBuf::from("edge"), now - day * 30),
            (PathBuf::from("new"), now - day * 2),
            (PathBuf::from("future"), now + day),
        ];
        assert_eq!(
            select_stale(&entries, now, day * 30),
            vec![PathBuf::from("old")]
        );
    }

    #[test]
    fn prune_removes_only_old_rotated_logs() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("shun_2026-01-01_00-00-00.log");
        let fresh = dir.path().join("shun_2026-10-01_00-00-00.log");
        let active = dir.path().join("shun.log");
        let other = dir.path().join("notes.txt");
        for p in [&old, &fresh, &active, &other] {
            std::fs::write(p, "x").unwrap();
        }
        let f = std::fs::File::options().write(true).open(&old).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(40 * 86400))
            .unwrap();
        drop(f);
        // active file is also old, but must be kept
        let f = std::fs::File::options().write(true).open(&active).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(40 * 86400))
            .unwrap();
        drop(f);
        let n = prune_old_logs(dir.path(), "shun", Duration::from_secs(30 * 86400));
        assert_eq!(n, 1);
        assert!(!old.exists());
        assert!(fresh.exists() && active.exists() && other.exists());
    }
}
