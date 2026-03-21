use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher,
};

use crate::apps::LaunchItem;
use crate::config::SearchMode;

pub fn filter(items: &[LaunchItem], query: &str, mode: &SearchMode) -> Vec<LaunchItem> {
    match mode {
        SearchMode::Exact => exact_filter(items, query),
        SearchMode::Fuzzy => fuzzy_filter(items, query),
        SearchMode::Migemo => migemo_filter(items, query),
    }
}

fn migemo_filter(items: &[LaunchItem], query: &str) -> Vec<LaunchItem> {
    if query.is_empty() {
        return items.to_vec();
    }
    let Some(re) = crate::migemo::build_regex(query) else {
        let q = query.to_lowercase();
        return items
            .iter()
            .filter(|item| item.name.to_lowercase().contains(&q))
            .cloned()
            .collect();
    };
    items
        .iter()
        .filter(|item| re.is_match(&item.name))
        .cloned()
        .collect()
}

fn exact_filter(items: &[LaunchItem], query: &str) -> Vec<LaunchItem> {
    let q = query.to_lowercase();
    items
        .iter()
        .filter(|item| item.name.to_lowercase().contains(&q))
        .cloned()
        .collect()
}

fn fuzzy_filter(items: &[LaunchItem], query: &str) -> Vec<LaunchItem> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut scored: Vec<(u32, &LaunchItem)> = items
        .iter()
        .filter_map(|item| {
            let score = pattern.score(
                nucleo_matcher::Utf32Str::new(&item.name, &mut vec![]),
                &mut matcher,
            )?;
            Some((score, item))
        })
        .collect();

    // スコア降順（高いほど良いマッチ）
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, item)| item.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::{ItemSource, LaunchItem};
    use crate::config::CompletionType;

    fn item(name: &str) -> LaunchItem {
        LaunchItem {
            name: name.to_string(),
            path: name.to_string(),
            args: vec![],
            workdir: None,
            source: ItemSource::Config,
            completion: CompletionType::None,
            completion_list: vec![],
            completion_command: None,
            completion_search_mode: None,
            history_key: None,
        }
    }

    // --- exact_filter ---

    #[test]
    fn exact_substring_match() {
        let items = vec![item("Firefox"), item("Notepad"), item("fire_starter")];
        let results = exact_filter(&items, "fire");
        let names: Vec<&str> = results.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Firefox"));
        assert!(names.contains(&"fire_starter"));
        assert!(!names.contains(&"Notepad"));
    }

    #[test]
    fn exact_case_insensitive() {
        let items = vec![item("Firefox")];
        assert_eq!(exact_filter(&items, "FIREFOX").len(), 1);
    }

    #[test]
    fn exact_empty_query_returns_all() {
        let items = vec![item("a"), item("b")];
        assert_eq!(exact_filter(&items, "").len(), 2);
    }

    #[test]
    fn exact_no_match_returns_empty() {
        let items = vec![item("Notepad")];
        assert!(exact_filter(&items, "xyz").is_empty());
    }

    // --- fuzzy_filter ---

    #[test]
    fn fuzzy_matches_subsequence() {
        let items = vec![item("Visual Studio Code"), item("Notepad")];
        let results = fuzzy_filter(&items, "vsc");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Visual Studio Code");
    }

    #[test]
    fn fuzzy_no_match_returns_empty() {
        let items = vec![item("Notepad")];
        assert!(fuzzy_filter(&items, "zzzzzz").is_empty());
    }

    #[test]
    fn fuzzy_empty_query_does_not_panic() {
        let items = vec![item("a"), item("b")];
        let _ = fuzzy_filter(&items, "");
    }

    #[test]
    fn fuzzy_returns_all_matching_items() {
        let items = vec![item("firefox"), item("file manager"), item("notepad")];
        let results = fuzzy_filter(&items, "fi");
        let names: Vec<&str> = results.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"firefox"));
        assert!(names.contains(&"file manager"));
        assert!(!names.contains(&"notepad"));
    }

    // --- filter dispatch ---

    #[test]
    fn filter_dispatches_exact() {
        let items = vec![item("foo"), item("bar")];
        let r = filter(&items, "foo", &SearchMode::Exact);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "foo");
    }

    #[test]
    fn filter_dispatches_fuzzy() {
        let items = vec![item("foo"), item("bar")];
        let r = filter(&items, "fo", &SearchMode::Fuzzy);
        assert!(!r.is_empty());
        assert_eq!(r[0].name, "foo");
    }

    // --- migemo_filter ---

    #[test]
    fn migemo_empty_query_returns_all() {
        let items = vec![item("Firefox"), item("Notepad")];
        assert_eq!(migemo_filter(&items, "").len(), 2);
    }

    #[test]
    fn migemo_ascii_substring_match() {
        let items = vec![item("Firefox"), item("Notepad"), item("fire_starter")];
        let results = migemo_filter(&items, "fire");
        let names: Vec<&str> = results.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Firefox"));
        assert!(names.contains(&"fire_starter"));
        assert!(!names.contains(&"Notepad"));
    }

    #[test]
    fn migemo_romaji_matches_japanese() {
        // "hajime" should match items containing hiragana/kanji read as "hajime"
        let items = vec![item("初めてのRust"), item("Notepad"), item("はじめに")];
        let results = migemo_filter(&items, "hajime");
        let names: Vec<&str> = results.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"初めてのRust"), "should match kanji 初め");
        assert!(names.contains(&"はじめに"), "should match hiragana はじめ");
        assert!(!names.contains(&"Notepad"));
    }

    #[test]
    fn migemo_no_match_returns_empty() {
        let items = vec![item("Firefox")];
        assert!(migemo_filter(&items, "zzzzzzzzz").is_empty());
    }

    #[test]
    fn filter_dispatches_migemo() {
        let items = vec![item("はじめに"), item("Notepad")];
        let r = filter(&items, "hajime", &SearchMode::Migemo);
        assert!(!r.is_empty());
        assert_eq!(r[0].name, "はじめに");
    }
}
