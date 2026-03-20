use std::path::Path;

use crate::config::CompletionType;

/// 補完タイプに応じて候補を返す
/// 戻り値: (prefix, completions)
///   prefix      = 入力のうちパス以外の部分 (例: "--flag ")
///   completions = 補完候補リスト（base_path がある場合は strip 済み）
pub fn complete(
    input: &str,
    completion_type: &CompletionType,
    completion_list: &[String],
    completion_command: &Option<String>,
    workdir: &Option<String>,
    base_path: Option<&str>,
) -> (String, Vec<String>) {
    match completion_type {
        CompletionType::None => (String::new(), vec![]),
        CompletionType::Path => complete_path(input, base_path),
        CompletionType::List => complete_list(input, completion_list),
        CompletionType::Command => complete_command(input, completion_command, workdir),
    }
}

// --- path 補完 ---

fn complete_path(input: &str, base_path: Option<&str>) -> (String, Vec<String>) {
    // base_path がある場合は base + input を実際のパスとして補完し、結果から base を strip する
    let effective_input = match base_path {
        Some(base) => {
            let base_expanded = crate::utils::expand_path(base).replace('\\', "/");
            format!("{}{}", base_expanded, input)
        }
        None => input.to_string(),
    };

    let (prefix, partial) = split_last_token(&effective_input);
    if partial.is_empty() && base_path.is_none() {
        return (prefix, vec![]);
    }

    let expanded = crate::utils::expand_path(if partial.is_empty() { &effective_input } else { partial });
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

    // base_path の展開済み文字列（strip 用）
    let base_expanded = base_path.map(|b| crate::utils::expand_path(b).replace('\\', "/"));

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
            // base_path がある場合は strip して相対パスで返す
            if let Some(ref base) = base_expanded {
                let base = base.trim_end_matches('/');
                if s.starts_with(base) {
                    return Some(s[base.len()..].trim_start_matches('/').to_string());
                }
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
    // base_path がある場合は prefix も strip（ユーザー入力の空白区切り部分のみ残す）
    let final_prefix = if base_path.is_some() {
        let base_len = base_expanded.as_deref().map(|b| b.trim_end_matches('/').len() + 1).unwrap_or(0);
        if prefix.len() > base_len {
            prefix[base_len..].to_string()
        } else {
            String::new()
        }
    } else {
        prefix
    };
    (final_prefix, completions)
}

// --- list 補完 ---

fn complete_list(input: &str, list: &[String]) -> (String, Vec<String>) {
    // list 補完はサブコマンド（最初のワード）向けなので split しない。
    // 入力がすでに完全な list アイテム + スペース で始まっていたら補完しない
    // (例: "search " や "search hoge" は補完不要)
    let input_lower = input.to_lowercase();
    if list
        .iter()
        .any(|item| input_lower.starts_with(&format!("{} ", item.to_lowercase())))
    {
        return (String::new(), vec![]);
    }
    let mut completions: Vec<String> = list
        .iter()
        .filter(|s| s.to_lowercase().starts_with(&input_lower))
        .cloned()
        .collect();
    completions.sort_by_key(|a| a.to_lowercase());
    (String::new(), completions)
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
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let mut cmd = std::process::Command::new("powershell");
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", cmd_str]);
            cmd.creation_flags(CREATE_NO_WINDOW);
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
    completions.sort_by_key(|a| a.to_lowercase());
    (prefix, completions)
}

// --- ユーティリティ ---

fn sort_completions(completions: &mut [String]) {
    completions.sort_by(|a, b| {
        let a_name = a.trim_end_matches('/').split('/').next_back().unwrap_or("");
        let b_name = b.trim_end_matches('/').split('/').next_back().unwrap_or("");
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- split_last_token ---

    #[test]
    fn split_no_space() {
        assert_eq!(split_last_token("foo"), (String::new(), "foo"));
    }

    #[test]
    fn split_with_space() {
        assert_eq!(
            split_last_token("--flag foo"),
            ("--flag ".to_string(), "foo")
        );
    }

    #[test]
    fn split_trailing_space() {
        let (prefix, last) = split_last_token("foo ");
        assert_eq!(prefix, "foo ");
        assert_eq!(last, "");
    }

    #[test]
    fn split_empty() {
        assert_eq!(split_last_token(""), (String::new(), ""));
    }

    #[test]
    fn split_multiple_spaces_uses_last() {
        let (prefix, last) = split_last_token("a b c");
        assert_eq!(prefix, "a b ");
        assert_eq!(last, "c");
    }

    // --- sort_completions ---

    #[test]
    fn sort_dirs_before_files() {
        let mut v = vec!["z_file".to_string(), "a_dir/".to_string()];
        sort_completions(&mut v);
        assert_eq!(v[0], "a_dir/");
    }

    #[test]
    fn sort_dotfiles_after_normal() {
        let mut v = vec![".hidden".to_string(), "normal".to_string()];
        sort_completions(&mut v);
        assert_eq!(v[0], "normal");
        assert_eq!(v[1], ".hidden");
    }

    #[test]
    fn sort_dollar_after_normal() {
        let mut v = vec!["$ENV".to_string(), "abc".to_string()];
        sort_completions(&mut v);
        assert_eq!(v[0], "abc");
        assert_eq!(v[1], "$ENV");
    }

    #[test]
    fn sort_dirs_before_dotdirs() {
        let mut v = vec![
            ".git/".to_string(),
            "src/".to_string(),
            "README.md".to_string(),
        ];
        sort_completions(&mut v);
        // src/ (normal dir) before README.md (file) before .git/ (dot dir)
        let src_pos = v.iter().position(|s| s == "src/").unwrap();
        let readme_pos = v.iter().position(|s| s == "README.md").unwrap();
        let git_pos = v.iter().position(|s| s == ".git/").unwrap();
        assert!(src_pos < readme_pos);
        assert!(readme_pos < git_pos);
    }

    // --- complete_list ---

    #[test]
    fn list_prefix_match() {
        let list = vec![
            "start".to_string(),
            "stop".to_string(),
            "status".to_string(),
        ];
        let (prefix, completions) = complete_list("st", &list);
        assert_eq!(prefix, "");
        assert!(completions.contains(&"start".to_string()));
        assert!(completions.contains(&"stop".to_string()));
        assert!(completions.contains(&"status".to_string()));
    }

    #[test]
    fn list_case_insensitive() {
        let list = vec!["Start".to_string()];
        let (_, completions) = complete_list("sta", &list);
        assert_eq!(completions, vec!["Start"]);
    }

    #[test]
    fn list_no_match() {
        let list = vec!["start".to_string()];
        let (_, completions) = complete_list("xyz", &list);
        assert!(completions.is_empty());
    }

    #[test]
    fn list_suppressed_after_full_match_plus_space() {
        let list = vec!["search".to_string(), "settings".to_string()];
        let (_, completions) = complete_list("search ", &list);
        assert!(completions.is_empty());
    }

    #[test]
    fn list_empty_input_returns_all() {
        let list = vec!["a".to_string(), "b".to_string()];
        let (_, completions) = complete_list("", &list);
        assert_eq!(completions.len(), 2);
    }

    #[test]
    fn list_sorted_alphabetically() {
        let list = vec!["stop".to_string(), "install".to_string(), "add".to_string()];
        let (_, completions) = complete_list("", &list);
        assert_eq!(completions, vec!["add", "install", "stop"]);
    }

    // --- complete_path (tempdir) ---

    #[test]
    fn path_lists_matching_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.txt"), "").unwrap();
        std::fs::write(dir.path().join("bar.txt"), "").unwrap();

        let input = format!("{}/fo", dir.path().to_string_lossy().replace('\\', "/"));
        let (prefix, completions) = complete_path(&input, None);
        assert_eq!(prefix, "");
        assert_eq!(completions.len(), 1);
        assert!(completions[0].contains("foo.txt"));
    }

    #[test]
    fn path_nonexistent_dir_returns_empty() {
        let (_, completions) = complete_path("/nonexistent_abc_xyz_shun_test/foo", None);
        assert!(completions.is_empty());
    }

    #[test]
    fn path_appends_slash_for_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("mydir")).unwrap();
        let input = format!("{}/my", dir.path().to_string_lossy().replace('\\', "/"));
        let (_, completions) = complete_path(&input, None);
        assert!(completions.iter().any(|c| c.ends_with("mydir/")));
    }

    #[test]
    fn path_splits_prefix_from_input() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("run.sh"), "").unwrap();
        let dir_str = dir.path().to_string_lossy().replace('\\', "/");
        let input = format!("--flag {}/ru", dir_str);
        let (prefix, completions) = complete_path(&input, None);
        assert_eq!(prefix, "--flag ");
        assert!(!completions.is_empty());
    }
}
