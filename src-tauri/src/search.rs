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
    }
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
