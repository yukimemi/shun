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

/// 文字列に Tera テンプレート構文（値展開 `{{ }}` または制御構文 `{% %}`）が
/// 含まれるかを判定する。両フォームとも一律で「テンプレートとして展開すべき
/// 文字列」の判定に使う（`os` 変数だけを使う `{% if os == "windows" %}...{% endif %}`
/// のような制御構文のみの文字列も対象に含めるため、`{{` の有無だけでは判定しない）。
pub fn has_template_syntax(s: &str) -> bool {
    s.contains("{{") || s.contains("{%")
}

/// launch 時にしか値が決まらない変数（`apps::build_template_context` だけが
/// 差し込むもの）。config ロード時のコンテキストには存在しない。
const LAUNCH_TIME_VARS: &[&str] = &[
    "args",
    "args_list",
    "file_path",
    "file_name",
    "file_stem",
    "file_ext",
    "file_dir",
];

/// テンプレート文字列が launch 時専用変数を参照しているかを判定する。
///
/// config ロード時の展開は `args` / `file_*` を知らないため、`{{ args }}` は
/// Tera がエラーにしてくれる（＝文字列が保持され launch 時に解決される）が、
/// `{% if args %}` のような制御構文は未定義 ident が黙って false 扱いになり、
/// ブロックごと消えてしまう。そうなる前にロード時展開自体を見送るための判定。
///
/// 識別子境界を見るので `{{ vars.args }}` や `{{ env.file_path }}` のような
/// ドット付きメンバー参照には反応しない。
pub fn references_launch_time_var(s: &str) -> bool {
    LAUNCH_TIME_VARS
        .iter()
        .any(|name| contains_bare_ident(s, name))
}

/// `s` に `ident` が「単体の識別子として」出現するか。直前が識別子文字か `.`
/// （メンバーアクセス）の場合、直後が識別子文字の場合はヒットとみなさない。
fn contains_bare_ident(s: &str, ident: &str) -> bool {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';
    let bytes = s.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = s[from..].find(ident) {
        let start = from + rel;
        let end = start + ident.len();
        let before_ok = s[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident_char(c) && c != '.');
        let after_ok = s[end..].chars().next().is_none_or(|c| !is_ident_char(c));
        if before_ok && after_ok {
            return true;
        }
        // 次の候補へ。ident が非空なので必ず前進する。
        from = start + 1;
        while from < bytes.len() && !s.is_char_boundary(from) {
            from += 1;
        }
    }
    false
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

    #[test]
    fn has_template_syntax_detects_value_placeholder() {
        assert!(has_template_syntax("{{ vars.foo }}"));
    }

    #[test]
    fn has_template_syntax_detects_control_only_block() {
        assert!(has_template_syntax(
            r#"{% if os == "windows" %}wt{% endif %}"#
        ));
    }

    #[test]
    fn has_template_syntax_false_for_plain_string() {
        assert!(!has_template_syntax("neovide"));
    }

    #[test]
    fn references_launch_time_var_detects_value_and_control_forms() {
        assert!(references_launch_time_var("{{ args }}"));
        assert!(references_launch_time_var("{% if args %}--file{% endif %}"));
        assert!(references_launch_time_var(
            "{% for a in args_list %}{{ a }}{% endfor %}"
        ));
        assert!(references_launch_time_var("{{ file_stem }}.log"));
        assert!(references_launch_time_var("{% if file_ext %}x{% endif %}"));
    }

    #[test]
    fn references_launch_time_var_ignores_member_access_and_substrings() {
        // vars/env のメンバーはロード時に解決できるので巻き込まない
        assert!(!references_launch_time_var("{{ vars.args }}"));
        assert!(!references_launch_time_var("{{ env.file_path }}"));
        // 別名の一部として現れるだけのケース
        assert!(!references_launch_time_var("{{ vars.my_args_dir }}"));
        assert!(!references_launch_time_var("{{ argsx }}"));
        assert!(!references_launch_time_var(
            "{% if os == \"windows\" %}wt{% endif %}"
        ));
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
