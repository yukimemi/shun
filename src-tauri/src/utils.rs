use std::path::PathBuf;

/// チルダと環境変数を展開する
/// 対応形式: ~ / %VAR% (Windows) / $VAR / ${VAR} (Unix)
pub fn expand_path(path: &str) -> String {
    let s = expand_tilde(path);
    expand_env_vars(&s)
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path.starts_with("~\\") || path == "~" {
        let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.replacen("~", &home.to_string_lossy(), 1)
    } else {
        path.to_string()
    }
}

fn expand_env_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            // %VAR% スタイル (Windows)
            if let Some(end) = s[i + 1..].find('%') {
                let var_name = &s[i + 1..i + 1 + end];
                if !var_name.is_empty() {
                    if let Ok(val) = std::env::var(var_name) {
                        result.push_str(&val);
                        i = i + 1 + end + 1;
                        continue;
                    }
                }
            }
            result.push('%');
            i += 1;
        } else if bytes[i] == b'$' {
            let start = i + 1;
            if start < bytes.len() && bytes[start] == b'{' {
                // ${VAR} スタイル
                if let Some(end) = s[start + 1..].find('}') {
                    let var_name = &s[start + 1..start + 1 + end];
                    if let Ok(val) = std::env::var(var_name) {
                        result.push_str(&val);
                        i = start + 1 + end + 1;
                        continue;
                    }
                }
            } else {
                // $VAR スタイル
                let end = s[start..]
                    .chars()
                    .position(|c| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(s.len() - start);
                let var_name = &s[start..start + end];
                if !var_name.is_empty() {
                    if let Ok(val) = std::env::var(var_name) {
                        result.push_str(&val);
                        i = start + end;
                        continue;
                    }
                }
            }
            result.push('$');
            i += 1;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_plain_string_unchanged() {
        assert_eq!(expand_path("notepad"), "notepad");
    }

    #[test]
    fn expand_absolute_path_unchanged() {
        assert_eq!(expand_path("/usr/bin/bash"), "/usr/bin/bash");
    }

    #[test]
    fn expand_percent_style() {
        std::env::set_var("SHUN_TEST_PCT", "hello");
        let result = expand_path("%SHUN_TEST_PCT%/world");
        std::env::remove_var("SHUN_TEST_PCT");
        assert_eq!(result, "hello/world");
    }

    #[test]
    fn expand_dollar_style() {
        std::env::set_var("SHUN_TEST_DOLLAR", "testval");
        let result = expand_path("$SHUN_TEST_DOLLAR/path");
        std::env::remove_var("SHUN_TEST_DOLLAR");
        assert_eq!(result, "testval/path");
    }

    #[test]
    fn expand_dollar_brace_style() {
        std::env::set_var("SHUN_TEST_BRACE", "braced");
        let result = expand_path("${SHUN_TEST_BRACE}/end");
        std::env::remove_var("SHUN_TEST_BRACE");
        assert_eq!(result, "braced/end");
    }

    #[test]
    fn expand_missing_var_keeps_sigil() {
        let result = expand_path("$SHUN_NONEXISTENT_XYZ_VAR/path");
        assert!(result.starts_with('$'));
    }
}
