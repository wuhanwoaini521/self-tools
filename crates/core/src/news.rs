//! News 领域契约（ADR-010：**独立 bounded context** —— 新闻发现 / 聚合 / 阅读 / AI 理解）。
//!
//! NewsSource 可以通过 RSS / Atom 取新闻，但 **RSS 只是 `NewsSourceType`
//! 的一种摄取方式，不是 News 的领域模型**。对比错误建模
//! `Feed { kind: Rss | News }` —— News 不是 RSS 的子类。
//!
//! 核心对象（ADR-010）：
//!
//! - [`NewsSource`] —— 新闻源（系统维护 / 推荐，不是用户 RSS 订阅的分类）
//! - [`NewsArticle`] —— 新闻文章
//! - [`NewsCategory`] —— 新闻分类（产品语义）
//! - `NewsTopic` —— 事件聚合 / Timeline / 多来源对照（**后续能力，暂不落契约**，
//!   等真实用例出现再加，避免死类型）
//!
//! 纯数据模型，无任何 I/O。**不含 `core::rss` 类型引用**（两个 context 只在
//! 共享 `core::feed` 摄取契约处相遇）。

use serde::{Deserialize, Serialize};

/// 新闻源的摄取方式（**技术属性**，不是产品分类）。
///
/// 前三者复用共享 `feed_fetcher`（`FeedFetcherPort`）；`Api` 为将来接入
/// 结构化新闻 API 预留。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NewsSourceType {
    #[default]
    Rss,
    Atom,
    JsonFeed,
    Api,
}

impl NewsSourceType {
    /// 稳定 id（持久化 / 前端分支用）。
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Rss => "rss",
            Self::Atom => "atom",
            Self::JsonFeed => "json_feed",
            Self::Api => "api",
        }
    }

    /// 从稳定 id 解析（未知 → `None`，不做模糊匹配）。
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "rss" => Some(Self::Rss),
            "atom" => Some(Self::Atom),
            "json_feed" => Some(Self::JsonFeed),
            "api" => Some(Self::Api),
            _ => None,
        }
    }
}

/// 新闻分类（**产品语义**：News 页的栏目）。
///
/// 不依赖任何源 —— 分类属于 News，源属于 NewsSource，两者是多对一。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NewsCategory {
    /// 综合要闻（通讯社 / 门户头条）。
    #[default]
    General,
    /// 科技 / AI。
    Tech,
    /// 财经 / 市场。
    Finance,
    /// 国际 / 外交。
    World,
    /// 中国 / 国内时政。
    China,
}

impl NewsCategory {
    /// 全部分类（顺序即展示顺序）。
    pub const ALL: [NewsCategory; 5] = [
        NewsCategory::General,
        NewsCategory::Tech,
        NewsCategory::Finance,
        NewsCategory::World,
        NewsCategory::China,
    ];

    /// 稳定 id（持久化 / 前端 i18n 键）。
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Tech => "tech",
            Self::Finance => "finance",
            Self::World => "world",
            Self::China => "china",
        }
    }

    /// 中文标签。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "综合",
            Self::Tech => "科技",
            Self::Finance => "财经",
            Self::World => "国际",
            Self::China => "中国",
        }
    }

    /// 从稳定 id 解析（未知 → `None`，不做模糊匹配）。
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.id() == id)
    }
}

/// 新闻源行（`news_sources` 表）。
///
/// 由**系统 seed / 推荐目录**建立，用户可删可改分类 —— 但它不是
/// `RssSubscription`：两边独立，同一 URL 允许同时存在。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewsSource {
    pub id: i64,
    pub name: String,
    /// 摄取地址（RSS / Atom / JSON Feed URL，或 API endpoint）。
    pub url: String,
    pub source_type: NewsSourceType,
    pub category: NewsCategory,
    pub site_url: Option<String>,
    pub last_updated: Option<i64>,
    /// 最近一次抓取失败原因（`None` = 正常）。
    pub last_error: Option<String>,
    /// 未读条数（查询时由 articles 实时聚合）。
    pub unread_count: i64,
}

/// 新闻文章行（`news_articles` 表；`source_name` 冗余便于列表直接展示）。
///
/// 与 `core::rss::ArticleRow`（`RssEntry`）是**平行类型**，不是同一类型的别名 ——
/// 两者未来会长出不同字段（News 会有 Topic / 多来源对照 / 事件聚合）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewsArticle {
    pub id: i64,
    pub source_id: i64,
    pub source_name: String,
    /// 去重主键（摄取契约 `FetchedEntry::guid`）。
    pub guid: String,
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub published_at: Option<i64>,
    pub summary: Option<String>,
    pub is_read: bool,
    pub starred: bool,
}

/// 一个推荐新闻源（**系统 seed / 候选**，不是用户 RSS 订阅）。
///
/// 由 `news_store` 在首次打开 `news.db` 时灌进 `news_sources`
/// （「News 应建立独立的 NewsSource seed 数据」）。用户可删；
/// 删掉的会重新出现在 UI 的推荐区。
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecommendedSource {
    /// 展示名（站点名）。
    pub name: String,
    /// Feed 地址（http/https）。
    pub url: String,
    /// 站点首页（可选）。
    pub site_url: Option<String>,
    /// 分类。
    pub category: NewsCategory,
    /// 一句话说明（为什么推荐）。
    pub note: String,
}

/// 内置推荐源目录（News 模块 onboarding 与 seed 的唯一来源）。
///
/// 这**不是订阅数据**：它描述「系统维护哪些新闻源」。用户随时可删源，
/// 本表只用于 seed 与推荐区展示。
///
/// 选取原则：① 官方/通讯社；② 格式稳定（RSS 2.0 / Atom / JSON Feed）；
/// ③ 覆盖五个分类。
pub fn recommended_sources() -> Vec<RecommendedSource> {
    RECOMMENDED_SOURCES
        .iter()
        .map(|&(name, url, site, category, note)| RecommendedSource {
            name: (*name).to_string(),
            url: (*url).to_string(),
            site_url: site.map(str::to_string),
            category,
            note: (*note).to_string(),
        })
        .collect()
}

/// 目录表：(name, feed_url, site_url, category, note)。
const RECOMMENDED_SOURCES: &[(&str, &str, Option<&str>, NewsCategory, &str)] = &[
    (
        "新华网",
        "http://www.news.cn/rss/politics.xml",
        Some("https://www.news.cn/"),
        NewsCategory::China,
        "国家通讯社，时政要闻第一手",
    ),
    (
        "人民网",
        "http://www.people.com.cn/rss/politics.xml",
        Some("http://www.people.com.cn/"),
        NewsCategory::China,
        "人民日报社主办，党政要闻与评论",
    ),
    (
        "财联社",
        "https://www.cls.cn/nodeapi/telegraphList",
        Some("https://www.cls.cn/"),
        NewsCategory::Finance,
        "24 小时电报流，市场速览",
    ),
    (
        "第一财经",
        "https://www.yicai.com/rss/news.xml",
        Some("https://www.yicai.com/"),
        NewsCategory::Finance,
        "宏观、公司与市场深度报道",
    ),
    (
        "路透中文",
        "https://cn.reuters.com/tools/rss",
        Some("https://cn.reuters.com/"),
        NewsCategory::World,
        "国际通讯社中文版，环球要闻",
    ),
    (
        "BBC 中文",
        "https://feeds.bbci.co.uk/zhongwen/simp/rss.xml",
        Some("https://www.bbc.com/zhongwen/simp"),
        NewsCategory::World,
        "国际新闻与深度分析",
    ),
    (
        "Hacker News Front Page",
        "https://hnrss.org/frontpage",
        Some("https://news.ycombinator.com/"),
        NewsCategory::Tech,
        "技术与创业社区每日头条",
    ),
    (
        "少数派",
        "https://sspai.com/feed",
        Some("https://sspai.com/"),
        NewsCategory::Tech,
        "效率工具与应用推荐",
    ),
    (
        "36 氪",
        "https://36kr.com/feed",
        Some("https://36kr.com/"),
        NewsCategory::Tech,
        "科技创投与商业趋势",
    ),
    (
        "澎湃新闻",
        "https://www.thepaper.cn/rss.xml",
        Some("https://www.thepaper.cn/"),
        NewsCategory::General,
        "综合时政与社会新闻",
    ),
    (
        "中国新闻网",
        "https://www.chinanews.com.cn/rss/scroll-news.xml",
        Some("https://www.chinanews.com.cn/"),
        NewsCategory::General,
        "中新社滚动新闻，社会要闻",
    ),
];
