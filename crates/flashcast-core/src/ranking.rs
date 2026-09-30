//! 稳定排序。对应 ADR §4 的排序键：
//! （匹配层级降序，贡献者内分数降序，来源优先级，来源内原始顺序，id）。

use std::collections::HashMap;

use crate::model::{MatchTier, Score, SearchItem, SourceId};

/// 计算一次匹配的层级与相对分数。
///
/// `query` 必须已做小写化与首尾去空白处理。返回 `None` 表示不匹配、不应出现
/// 在结果中。
pub fn score_match(query: &str, title: &str, metadata: &[&str]) -> Option<Score> {
    if query.is_empty() {
        return Some(Score::unordered());
    }
    let title_lower = title.to_lowercase();
    if title_lower == query {
        // 标题完全相等属于前缀层级，但相关度最高。
        return Some(Score::new(MatchTier::TitlePrefix, 100));
    }
    if title_lower.starts_with(query) {
        // 越接近完全匹配，相关度越高。
        let extra = title_lower.chars().count().saturating_sub(query.chars().count());
        let relevance = 80u8.saturating_sub((extra.min(30) as u8).saturating_mul(2));
        return Some(Score::new(MatchTier::TitlePrefix, relevance.max(40)));
    }
    if title_lower.contains(query) {
        let extra = title_lower.chars().count().saturating_sub(query.chars().count());
        let relevance = 55u8.saturating_sub((extra.min(20) as u8).saturating_mul(2));
        return Some(Score::new(MatchTier::TitleSubstring, relevance.max(20)));
    }
    for (index, value) in metadata.iter().enumerate() {
        let value_lower = value.to_lowercase();
        if value_lower.contains(query) {
            // 越靠前的元数据字段（说明、关键词、路径）相关度略高。
            let relevance = 35u8.saturating_sub((index.min(10) as u8).saturating_mul(3));
            return Some(Score::new(MatchTier::MetadataSubstring, relevance.max(5)));
        }
    }
    None
}

/// 待排序的条目：附带来源内原始顺序。
pub struct RankedItem {
    pub item: SearchItem,
    /// 该条目在来源结果中的原始位置。
    pub source_order: usize,
    /// 来源优先级，越小越靠前。
    pub source_priority: u32,
}

/// 按 ADR §4 的排序键排序：先层级，再相关度，再来源优先级、来源内顺序、id。
pub fn sort_ranked(items: &mut [RankedItem]) {
    items.sort_by(|a, b| {
        a.item
            .score
            .tier
            .cmp(&b.item.score.tier)
            .then_with(|| b.item.score.relevance.cmp(&a.item.score.relevance))
            .then_with(|| a.source_priority.cmp(&b.source_priority))
            .then_with(|| a.source_order.cmp(&b.source_order))
            .then_with(|| a.item.id.cmp(&b.item.id))
    });
}

/// 为每个来源分配优先级。宿主自身优先，其后按插件注册顺序。
pub fn source_priorities(sources: &[SourceId]) -> HashMap<SourceId, u32> {
    let mut map = HashMap::new();
    for (index, source) in sources.iter().enumerate() {
        map.entry(source.clone()).or_insert(index as u32);
    }
    map
}
