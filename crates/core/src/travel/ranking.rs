//! 来源可信度与评分（需求 #七 / #八）。
//!
//! V1 使用规则系统：按域名特征分类 `SourceLevel`，再按
//! `final_score = authority × freshness × relevance` 综合排序。
//! 规则简单但可单测、可解释；后续可替换为更细的规则或模型打分。

use crate::travel::model::{SearchResult, SourceLevel, TravelSource};
use crate::travel::query_planner::QueryCategory;

const REL_KEYWORDS: &[&str] = &[
    "推荐",
    "攻略",
    "门票",
    "开放时间",
    "美食",
    "景点",
    "交通",
    "住宿",
    "天气",
    "活动",
    "避坑",
    "预约",
    "地铁",
    "机场",
    "车站",
    "路线",
    "博物馆",
    "价格",
    "时间",
    "地址",
];

/// 官方 / 权威域名关键字（host 小写后包含匹配）。
const OFFICIAL_KEYWORDS: &[&str] = &[
    "gov.cn", "12301", "12306", "caac", "cma.gov", "airport", "metro", "subway", "museum", "bwg",
    "bowuguan", "tourism", "wenlv", "culture",
];

/// 大型稳定信息源。
const AUTHORITATIVE_KEYWORDS: &[&str] = &["baike", "amap", "gaode", "map.baidu"];

/// 旅游内容平台。
const PLATFORM_KEYWORDS: &[&str] = &[
    "ctrip",
    "qunar",
    "mafengwo",
    "tongcheng",
    "fliggy",
    "alitrip",
    "tuniu",
    "elong",
    "ly.com",
    "meituan",
    "dianping",
];

/// UGC / 社区 / 个人站点。
const UGC_KEYWORDS: &[&str] = &[
    "zhihu",
    "xiaohongshu",
    "xhs",
    "weibo",
    "bilibili",
    "douban",
    "jianshu",
    "csdn",
    "tieba",
    "blog",
    "wordpress",
    "tumblr",
    "lofter",
];

/// 按域名与 URL 特征分类来源等级。未知来源一律按 C（低可信）处理。
#[must_use]
pub fn classify_source(host: &str, _url: &str) -> SourceLevel {
    let lowered = host.trim().to_ascii_lowercase();
    if OFFICIAL_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
    {
        return SourceLevel::S;
    }
    if AUTHORITATIVE_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
    {
        return SourceLevel::A;
    }
    if PLATFORM_KEYWORDS
        .iter()
        .any(|keyword| lowered.contains(keyword))
    {
        return SourceLevel::B;
    }
    if UGC_KEYWORDS.iter().any(|keyword| lowered.contains(keyword)) {
        return SourceLevel::C;
    }
    SourceLevel::C
}

/// 新鲜度评分：发布时间越近越高；无发布时间按中等偏上处理（需求 #八）。
#[must_use]
pub fn freshness_score(published_at: Option<i64>, now: i64) -> f32 {
    let Some(published) = published_at.filter(|p| *p > 0) else {
        return 0.78;
    };
    let age_days = ((now - published).max(0) as f64 / 86_400.0) as u64;
    match age_days {
        0..=30 => 1.0,
        31..=90 => 0.92,
        91..=180 => 0.82,
        181..=365 => 0.68,
        _ => 0.5,
    }
}

/// 相关度评分：统计查询中的关键旅行词在标题/摘要里的出现次数（需求 #八 relevance）。
/// 中文无法按词切分，使用特征词命中近似；无命中给保底 0.45，避免把优质内容一票否决。
#[must_use]
pub fn relevance_score(doc_text: &str, _query: &str) -> f32 {
    let text = doc_text.to_lowercase();
    let matched = REL_KEYWORDS
        .iter()
        .filter(|keyword| text.contains(**keyword))
        .count();
    (0.45 + 0.12 * matched as f32).min(1.0)
}

/// 与用户请求绑定的来源评分上下文。搜索词、意图、日期和偏好共同决定相关性。
#[derive(Clone, Debug)]
pub struct SourceRankingContext<'a> {
    pub city: &'a str,
    pub category: QueryCategory,
    pub preferences: &'a [String],
    pub date_range: Option<(&'a str, &'a str)>,
}

fn context_relevance(context: &SourceRankingContext<'_>, query: &str, text: &str) -> f32 {
    let lowered = text.to_lowercase();
    let terms = query
        .split_whitespace()
        .filter(|term| term.len() > 1 && *term != context.city)
        .collect::<Vec<_>>();
    let query_hits = terms.iter().filter(|term| lowered.contains(**term)).count();
    let preference_hits = context
        .preferences
        .iter()
        .filter(|preference| lowered.contains(&preference.to_lowercase()))
        .count();
    let category_terms = match context.category {
        QueryCategory::Food => ["美食", "小吃", "餐厅", "吃"].as_slice(),
        QueryCategory::Transport => ["交通", "公交", "车站", "打车"].as_slice(),
        QueryCategory::Accommodation => ["住宿", "酒店", "区域", "住"].as_slice(),
        QueryCategory::Warnings => ["开放", "预约", "门票", "注意"].as_slice(),
        QueryCategory::Activities => ["活动", "天气", "演出", "摄影"].as_slice(),
        QueryCategory::Itinerary => ["路线", "行程", "一日游", "两日游", "三日游"].as_slice(),
        QueryCategory::Attractions => ["景点", "风景", "博物馆", "古迹"].as_slice(),
        QueryCategory::CityIntro => ["城市", "历史", "文化", "旅行"].as_slice(),
    };
    let category_hits = category_terms
        .iter()
        .filter(|term| lowered.contains(**term))
        .count();
    let date_hits = context.date_range.is_some()
        && (lowered.contains("2026") || lowered.contains("日期") || lowered.contains("活动"));
    (0.35
        + 0.1 * query_hits as f32
        + 0.06 * category_hits as f32
        + 0.08 * preference_hits as f32
        + if date_hits { 0.08 } else { 0.0 })
    .min(1.0)
}

fn category_authority(level: SourceLevel, category: QueryCategory, host: &str) -> f32 {
    let base = level.weight();
    let host = host.to_ascii_lowercase();
    match category {
        QueryCategory::Food
            if host.contains("dianping")
                || host.contains("meituan")
                || host.contains("xiaohongshu") =>
        {
            (base + 0.08).min(1.0)
        }
        QueryCategory::Warnings if level == SourceLevel::S => 1.0,
        QueryCategory::Transport
            if level == SourceLevel::S || host.contains("amap") || host.contains("12306") =>
        {
            (base + 0.05).min(1.0)
        }
        _ => base,
    }
}

/// 按用户搜索意图评分；同一来源在“开放状态”和“本地美食”场景可有不同权重。
#[must_use]
pub fn rate_source_for(
    context: &SourceRankingContext<'_>,
    query: &str,
    result: &SearchResult,
) -> TravelSource {
    let level = classify_source(&host_of(&result.url), &result.url);
    let authority = category_authority(level, context.category, &host_of(&result.url));
    let freshness = freshness_score(result.published_at, result.fetched_at);
    let relevance = context_relevance(
        context,
        query,
        &format!("{} {}", result.title, result.snippet),
    );
    let score = authority * freshness * relevance;
    TravelSource {
        url: result.url.clone(),
        title: result.title.clone(),
        host: host_of(&result.url),
        level,
        state: crate::travel::model::ContentState::SnippetOnly,
        published_at: result.published_at,
        fetched_at: result.fetched_at,
        score: (score * 1000.0).round() / 1000.0,
    }
}

/// 综合评分一个搜索结果，产出带评级的 `TravelSource`。
#[must_use]
pub fn rate_source(query: &str, result: &SearchResult) -> TravelSource {
    let level = classify_source(&host_of(&result.url), &result.url);
    let score = level.weight()
        * freshness_score(result.published_at, result.fetched_at)
        * relevance_score(&format!("{} {}", result.title, result.snippet), query);
    TravelSource {
        url: result.url.clone(),
        title: result.title.clone(),
        host: host_of(&result.url),
        level,
        state: crate::travel::model::ContentState::SnippetOnly,
        published_at: result.published_at,
        fetched_at: result.fetched_at,
        score: (score * 1000.0).round() / 1000.0,
    }
}

/// 提取 host（不含端口 / 用户信息）。极简字符串实现，避免 core 引入 URL 解析依赖；
/// 解析失败返回空串，评分按 C 处理。
#[must_use]
pub fn host_of(url: &str) -> String {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let without_userinfo = authority.rsplit('@').next().unwrap_or(authority);
    without_userinfo
        .split(':')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use crate::travel::model::{SearchResult, SourceLevel};

    use super::{classify_source, freshness_score, rate_source, relevance_score};

    #[test]
    fn classifies_official_sources_as_s() {
        assert_eq!(classify_source("hz.gov.cn", ""), SourceLevel::S);
        assert_eq!(classify_source("wlj.zj.gov.cn", ""), SourceLevel::S);
        assert_eq!(classify_source("hzmetro.com", ""), SourceLevel::S);
        assert_eq!(classify_source("xiamenairport.com.cn", ""), SourceLevel::S);
        assert_eq!(classify_source("www.12306.cn", ""), SourceLevel::S);
        assert_eq!(classify_source("zjbowuguan.cn", ""), SourceLevel::S);
    }

    #[test]
    fn classifies_authoritative_and_platform_and_ugc() {
        assert_eq!(classify_source("baike.baidu.com", ""), SourceLevel::A);
        assert_eq!(classify_source("restapi.amap.com", ""), SourceLevel::A);
        assert_eq!(classify_source("you.ctrip.com", ""), SourceLevel::B);
        assert_eq!(classify_source("www.mafengwo.cn", ""), SourceLevel::B);
        assert_eq!(classify_source("www.zhihu.com", ""), SourceLevel::C);
        assert_eq!(classify_source("www.xiaohongshu.com", ""), SourceLevel::C);
    }

    #[test]
    fn unknown_hosts_default_to_low_trust() {
        assert_eq!(
            classify_source("some-personal-blog.example", ""),
            SourceLevel::C
        );
        assert_eq!(classify_source("", ""), SourceLevel::C);
    }

    #[test]
    fn freshness_decays_with_age_and_missing_date_is_mid() {
        let now = 1_800_000_000;
        assert_eq!(freshness_score(Some(now - 10 * 86_400), now), 1.0);
        assert_eq!(freshness_score(Some(now - 60 * 86_400), now), 0.92);
        assert_eq!(freshness_score(Some(now - 100 * 86_400), now), 0.82);
        assert_eq!(freshness_score(Some(now - 300 * 86_400), now), 0.68);
        assert_eq!(freshness_score(Some(now - 800 * 86_400), now), 0.5);
        assert_eq!(freshness_score(None, now), 0.78);
    }

    #[test]
    fn relevance_ignores_unknown_queries_but_keeps_floor() {
        assert!(relevance_score("杭州 美食 攻略", "杭州") >= 0.45 + 0.12 * 2.0 - f32::EPSILON);
        assert_eq!(relevance_score("随便一段文字", "随便"), 0.45);
    }

    #[test]
    fn rate_source_combines_three_scores() {
        let result = SearchResult {
            title: "杭州博物馆开放时间".to_string(),
            url: "https://hz.gov.cn/museum".to_string(),
            snippet: "开放时间 门票".to_string(),
            provider: "bing".to_string(),
            published_at: Some(1_700_000_000),
            fetched_at: 1_700_000_000,
        };
        let rated = rate_source("杭州 博物馆", &result);
        assert_eq!(rated.level, SourceLevel::S);
        assert_eq!(rated.host, "hz.gov.cn");
        assert!(rated.score > 0.8 - f32::EPSILON, "score: {}", rated.score);
        assert_eq!(rated.state, crate::travel::model::ContentState::SnippetOnly);
    }
}
