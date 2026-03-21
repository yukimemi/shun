use rustmigemo::migemo::compact_dictionary::CompactDictionary;
use rustmigemo::migemo::query::query;
use rustmigemo::migemo::regex_generator::RegexOperator;
use std::sync::OnceLock;

// 辞書は public/ に配置し、Vite が dist/ にコピーする
// Rust 側は include_bytes! でバイナリに埋め込む（.bin 拡張子で Tauri asset serving が正常動作）
static DICT_BYTES: &[u8] = include_bytes!("../../public/migemo-compact-dict.bin");

static DICT: OnceLock<CompactDictionary> = OnceLock::new();

fn get_dict() -> &'static CompactDictionary {
    DICT.get_or_init(|| CompactDictionary::new(&DICT_BYTES.to_vec()))
}

/// ローマ字クエリを migemo regex 文字列に変換する
pub fn query_to_pattern(input: &str) -> String {
    query(input.to_string(), get_dict(), &RegexOperator::Default)
}

/// クエリから Regex をコンパイルして返す（失敗時は None）
/// search/complete の呼び出し元でアイテムループの外でコンパイルして使う
pub fn build_regex(query_str: &str) -> Option<regex::Regex> {
    if query_str.is_empty() {
        return None;
    }
    let pattern = query_to_pattern(query_str);
    regex::Regex::new(&format!("(?i){}", pattern)).ok()
}

/// migemo マッチ判定（complete.rs の単発マッチ用・アイテム数が少ない場合向け）
pub fn matches(query_str: &str, target: &str) -> bool {
    if query_str.is_empty() {
        return true;
    }
    build_regex(query_str)
        .map(|re| re.is_match(target))
        .unwrap_or_else(|| target.to_lowercase().contains(&query_str.to_lowercase()))
}
