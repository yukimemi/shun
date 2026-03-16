use std::path::Path;

use crate::config::CompletionType;

/// 補完タイプに応じて候補を返す
/// 戻り値: (prefix, completions)
///   prefix      = 入力のうちパス以外の部分 (例: "--flag ")
///   completions = 補完候補リスト
pub fn complete(
    input: &str,
    completion_type: &CompletionType,
    completion_list: &[String],
    completion_command: &Option<String>,
    workdir: &Option<String>,
) -> (String, Vec<String>) {
    match completion_type {
        CompletionType::None => (String::new(), vec![]),
        CompletionType::Path => complete_path(input),
        CompletionType::List => complete_list(input, completion_list),
        CompletionType::Command => complete_command(input, completion_command, workdir),
    }
}

// --- path 補完 ---

fn complete_path(input: &str) -> (String, Vec<String>) {
    let (prefix, partial) = split_last_token(input);
    if partial.is_empty() {
        return (prefix, vec![]);
    }

    let expanded = crate::utils::expand_path(partial);
    let expanded_path = Path::new(&expanded);

    let (dir, stem) = if expanded.ends_with('/') || expanded.ends_with('\\') {
        (expanded.as_str().to_string(), String::new())
    } else {
        let parent = expanded_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let file = expanded_path
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        (parent, file)
    };

    let dir_path = Path::new(&dir);
    if !dir_path.exists() {
        return (prefix, vec![]);
    }

    let entries = match std::fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(_) => return (prefix, vec![]),
    };

    let mut completions: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.to_lowercase().starts_with(&stem) {
                return None;
            }
            let full = entry.path();
            let mut s = full.to_string_lossy().to_string();
            s = s.replace('\\', "/");
            if entry.path().is_dir() {
                s.push('/');
            }
            if let Some(home) = dirs_next::home_dir() {
                let home_str = home.to_string_lossy().replace('\\', "/");
                if s.starts_with(&*home_str) {
                    s = format!("~{}", &s[home_str.len()..]);
                }
            }
            Some(s)
        })
        .collect();

    sort_completions(&mut completions);
    (prefix, completions)
}

// --- list 補完 ---

fn complete_list(input: &str, list: &[String]) -> (String, Vec<String>) {
    let (prefix, partial) = split_last_token(input);
    let partial_lower = partial.to_lowercase();
    let mut completions: Vec<String> = list
        .iter()
        .filter(|s| s.to_lowercase().starts_with(&partial_lower))
        .cloned()
        .collect();
    completions.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    (prefix, completions)
}

// --- command 補完 ---

fn complete_command(
    input: &str,
    command: &Option<String>,
    workdir: &Option<String>,
) -> (String, Vec<String>) {
    let Some(cmd_str) = command else {
        return (String::new(), vec![]);
    };

    let (prefix, partial) = split_last_token(input);
    let partial_lower = partial.to_lowercase();

    // シェル経由で実行
    let output = {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("cmd");
            cmd.args(["/c", cmd_str]);
            if let Some(dir) = workdir {
                cmd.current_dir(crate::utils::expand_path(dir));
            }
            cmd.output()
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", cmd_str]);
            if let Some(dir) = workdir {
                cmd.current_dir(crate::utils::expand_path(dir));
            }
            cmd.output()
        }
    };

    let output = match output {
        Ok(o) => o,
        Err(_) => return (prefix, vec![]),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut completions: Vec<String> = stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && l.to_lowercase().starts_with(&partial_lower))
        .collect();
    completions.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    (prefix, completions)
}

// --- ユーティリティ ---

fn sort_completions(completions: &mut Vec<String>) {
    completions.sort_by(|a, b| {
        let a_name = a.trim_end_matches('/').split('/').last().unwrap_or("");
        let b_name = b.trim_end_matches('/').split('/').last().unwrap_or("");
        let a_special = a_name.starts_with('.') || a_name.starts_with('$');
        let b_special = b_name.starts_with('.') || b_name.starts_with('$');
        let a_dir = a.ends_with('/');
        let b_dir = b.ends_with('/');
        match (a_special, b_special) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => match (a_dir, b_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.to_lowercase().cmp(&b.to_lowercase()),
            },
        }
    });
}

fn split_last_token(input: &str) -> (String, &str) {
    if let Some(pos) = input.rfind(' ') {
        let prefix = &input[..=pos];
        let last = &input[pos + 1..];
        (prefix.to_string(), last)
    } else {
        (String::new(), input)
    }
}

