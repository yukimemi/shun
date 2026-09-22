use std::path::{Path, PathBuf};

/// チルダと環境変数を展開する
/// 対応形式: ~ / %VAR% (Windows) / $VAR / ${VAR} (Unix)
pub fn expand_path(path: &str) -> String {
    let s = expand_tilde(path);
    expand_env_vars(&s)
}

/// 実行中バイナリのパス (`.../<name>.app/Contents/MacOS/<binary>`) から
/// `.app` バンドルのルート (`.../<name>.app`) を求める。
/// 期待する 3 階層構造 (親が `MacOS`、その親が `Contents`、その親の拡張子が `app`)
/// に合致しない場合は `None` を返す。ファイルシステムへのアクセスは行わない。
/// 呼び出し元 (`lib.rs`) は macOS 専用のため、他 OS では未使用 (テストからのみ参照) になる。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn macos_app_bundle_root(exe: &Path) -> Option<PathBuf> {
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let app_dir = contents_dir.parent()?;
    if app_dir.extension()?.to_str()? != "app" {
        return None;
    }
    Some(app_dir.to_path_buf())
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
            let ch = s[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
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

    #[test]
    fn expand_multibyte_unchanged() {
        let s = "/path/to/日本語.md";
        assert_eq!(expand_path(s), s);
    }

    #[test]
    fn expand_multibyte_with_var() {
        std::env::set_var("SHUN_TEST_MB", "/memo");
        let result = expand_path("$SHUN_TEST_MB/日本語.md");
        std::env::remove_var("SHUN_TEST_MB");
        assert_eq!(result, "/memo/日本語.md");
    }

    #[test]
    fn bundle_root_valid_structure() {
        let exe = PathBuf::from("/Applications/shun.app/Contents/MacOS/shun");
        assert_eq!(
            macos_app_bundle_root(&exe),
            Some(PathBuf::from("/Applications/shun.app"))
        );
    }

    #[test]
    fn bundle_root_rejects_wrong_macos_dir_name() {
        let exe = PathBuf::from("/Applications/shun.app/Contents/Resources/shun");
        assert_eq!(macos_app_bundle_root(&exe), None);
    }

    #[test]
    fn bundle_root_rejects_wrong_contents_dir_name() {
        let exe = PathBuf::from("/Applications/shun.app/Resources/MacOS/shun");
        assert_eq!(macos_app_bundle_root(&exe), None);
    }

    #[test]
    fn bundle_root_rejects_non_app_extension() {
        let exe = PathBuf::from("/Applications/shun.bundle/Contents/MacOS/shun");
        assert_eq!(macos_app_bundle_root(&exe), None);
    }

    #[test]
    fn bundle_root_rejects_insufficient_depth() {
        let exe = PathBuf::from("/shun");
        assert_eq!(macos_app_bundle_root(&exe), None);
    }

    #[test]
    fn bundle_root_rejects_bare_binary() {
        let exe = PathBuf::from("MacOS/shun");
        assert_eq!(macos_app_bundle_root(&exe), None);
    }
}

#[cfg(test)]
mod tera_date_test {
    #[test]
    fn tera_now_date_format() {
        let result = tera::Tera::one_off(
            r#"{{ now() | date(format="%Y%m%d") }}"#,
            &tera::Context::new(),
            false,
        );
        assert!(result.is_ok(), "tera now()|date failed: {:?}", result);
        let s = result.unwrap();
        assert_eq!(s.len(), 8, "expected YYYYMMDD, got: {}", s);
        println!("date = {}", s);
    }
}
