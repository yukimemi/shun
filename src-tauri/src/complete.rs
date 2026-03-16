use std::path::{Path, PathBuf};

/// 入力の末尾にあるパスっぽいトークンを補完候補と合わせて返す
/// 戻り値: (prefix, completions)
///   prefix    = 入力のうちパス以外の部分 (例: "--flag ")
///   completions = パス補完候補リスト
pub fn complete(input: &str) -> (String, Vec<String>) {
    // 入力の最後のトークンをパスとして扱う
    let (prefix, partial) = split_last_token(input);
    if partial.is_empty() {
        return (prefix, vec![]);
    }

    let expanded = expand_tilde(partial);
    let expanded_path = Path::new(&expanded);

    // dir と stem を分ける
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
            // パスを組み立て（チルダを復元）
            let full = entry.path();
            let mut s = full.to_string_lossy().to_string();
            // Windows のバックスラッシュをスラッシュに
            s = s.replace('\\', "/");
            // ディレクトリには末尾スラッシュ
            if entry.path().is_dir() {
                s.push('/');
            }
            // チルダに戻す
            if let Some(home) = dirs_next::home_dir() {
                let home_str = home.to_string_lossy().replace('\\', "/");
                if s.starts_with(&*home_str) {
                    s = format!("~{}", &s[home_str.len()..]);
                }
            }
            Some(s)
        })
        .collect();

    // ディレクトリ優先、次に通常ファイル。ドット/ドル始まりは後ろへ
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
    (prefix, completions)
}

fn split_last_token(input: &str) -> (String, &str) {
    // スペース区切りで最後のトークンを取り出す
    if let Some(pos) = input.rfind(' ') {
        let prefix = &input[..=pos];
        let last = &input[pos + 1..];
        (prefix.to_string(), last)
    } else {
        (String::new(), input)
    }
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.replacen("~", &home.to_string_lossy().replace('\\', "/"), 1)
    } else {
        path.to_string()
    }
}
