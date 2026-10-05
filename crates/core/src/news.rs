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
    /// **源健康度**（2026-10-05 起）。
    ///
    /// 为什么要单独一维：抓取「成功」不等于源还活着。实测发现人民网
    /// `people.com.cn/rss/politics.xml` 一直返回 200，但内容停在 2025-06 ——
    /// 这种「假活」源会静默贡献过期内容，用户只会觉得「新闻怎么不更新」。
    /// `Stale` 就是给这种源用的：**不报错，但明确标记为已停更**。
    pub health: SourceHealth,
    /// 该源最新一篇文章的发布时间（用于判断停更与展示）。
    pub latest_article_at: Option<i64>,
    /// 被系统停用的原因（`None` = 未停用）。
    ///
    /// 例如「人民网 politics RSS 仍返回 200，但最新条目停在 2025-06（已停更）」。
    /// 必须把原因告诉用户：源凭空消失会让人以为数据丢了。
    pub disabled_reason: Option<String>,
}

/// 新闻源健康度。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceHealth {
    /// 正常：抓取成功且内容新鲜。
    #[default]
    Ok,
    /// 抓取失败（404 / 401 / 返回 HTML / 网络错误）——`last_error` 有原因。
    Failing,
    /// 抓取成功但内容停更（最新文章超过阈值）——**最容易被忽略的一种坏**。
    Stale,
    /// 已被系统停用（例如上一版目录里的源已下线）。
    Disabled,
}

impl SourceHealth {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failing => "failing",
            Self::Stale => "stale",
            Self::Disabled => "disabled",
        }
    }

    #[must_use]
    pub fn from_id(raw: &str) -> Option<Self> {
        match raw {
            "ok" => Some(Self::Ok),
            "failing" => Some(Self::Failing),
            "stale" => Some(Self::Stale),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    /// 是否需要在 UI 上给出**明确的坏消息**（而不是假装正常）。
    #[must_use]
    pub const fn needs_attention(self) -> bool {
        matches!(self, Self::Failing | Self::Stale | Self::Disabled)
    }
}

/// 停更判定阈值（天）。
///
/// 10 天：日更源（中新网 / BBC / HN / 东财 / IT之家）远低于此；
/// 周更源（阮一峰）也在此之内。超过 10 天基本等于「源死了但还在返回 200」。
pub const SOURCE_STALE_AFTER_DAYS: i64 = 10;

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
    /// **人工实测日期**（`YYYY-MM-DD`）：这个 URL 在那天确实能拉到 feed。
    /// 没有它，目录就只是一张写着「看起来不错」的地址表。
    pub verified_on: String,
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
        .map(
            |&(name, url, site, category, note, verified_on)| RecommendedSource {
                name: (*name).to_string(),
                url: (*url).to_string(),
                site_url: site.map(str::to_string),
                category,
                note: (*note).to_string(),
                verified_on: (*verified_on).to_string(),
            },
        )
        .collect()
}

/// 目录表：(name, feed_url, site_url, category, note, verified_on)。
///
/// ## 为什么要带「实测日期」
///
/// 上一版目录里的 11 个源，到 2026-10-05 已有 **6 个死了**（404 / 401 / 返回 HTML），
/// 还有 1 个（人民网）**不报错但内容停在 2025-06** —— 这种「假活」最坏：
/// 用户只会看到「今日新闻里怎么都是三个月前的旧闻」。
///
/// 所以每个源都记录**人工实测日期**：目录不是写完就算数，是要定期复核的东西。
/// 复核脚本：`scripts/check_news_sources.sh`（逐个拉取、验证是否 feed、报告最新条目时间）。
///
/// ## 已下线的源（不要再加回来）
///
/// | 源 | 2026-10-05 实测 | 处置 |
/// |---|---|---|
/// | 新华网 `news.cn/rss/politics.xml` | 404 | 删除，政要用「中国新闻网·中国」 |
/// | 财联社 `cls.cn/nodeapi/telegraphList` | 404（非 RSS 接口） | 删除，财经用东方财富 |
/// | 第一财经 `yicai.com/rss/news.xml` | 404 | 删除 |
/// | 路透中文 `cn.reuters.com/tools/rss` | 301 → reuters.com，401 | 删除（中文站已下线） |
/// | 36 氪 `36kr.com/feed` | 200 但返回 HTML（JS 壳） | 删除，科技用 IT之家 / 极客公园 |
/// | 澎湃新闻 `thepaper.cn/rss.xml` | 302 → 首页 | 删除 |
/// | 人民网 `people.com.cn/rss/politics.xml` | 200 但最新条目 2025-06-05 | 停止 seed（内容停更），旧库由迁移自动停用 |
/// 目录表的一行：`(name, feed_url, site_url, category, note, verified_on)`。
type CatalogRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    NewsCategory,
    &'static str,
    &'static str,
);

const RECOMMENDED_SOURCES: &[CatalogRow] = &[
    (
        "中国新闻网",
        "https://www.chinanews.com.cn/rss/scroll-news.xml",
        Some("https://www.chinanews.com.cn/"),
        NewsCategory::General,
        "中新社滚动新闻，社会要闻（实测更新最勤的中文源）",
        "2026-10-05",
    ),
    (
        "中国新闻网·中国",
        "https://www.chinanews.com.cn/rss/china.xml",
        Some("https://www.chinanews.com.cn/"),
        NewsCategory::China,
        "中新社国内要闻；分频道 RSS 比综合版更聚焦",
        "2026-10-05",
    ),
    (
        "中国新闻网·国际",
        "https://www.chinanews.com.cn/rss/world.xml",
        Some("https://www.chinanews.com.cn/"),
        NewsCategory::World,
        "中新社国际新闻",
        "2026-10-05",
    ),
    (
        "中国新闻网·财经",
        "https://www.chinanews.com.cn/rss/finance.xml",
        Some("https://www.chinanews.com.cn/"),
        NewsCategory::Finance,
        "中新社财经要闻",
        "2026-10-05",
    ),
    (
        "东方财富",
        "http://rss.eastmoney.com/rss_partener.xml",
        Some("https://www.eastmoney.com/"),
        NewsCategory::Finance,
        "财经资讯流，条目量大、更新快（替代已下线的财联社 / 第一财经）",
        "2026-10-05",
    ),
    (
        "BBC 中文",
        "https://feeds.bbci.co.uk/zhongwen/simp/rss.xml",
        Some("https://www.bbc.com/zhongwen/simp"),
        NewsCategory::World,
        "国际新闻与深度分析",
        "2026-10-05",
    ),
    (
        "联合国新闻（中文）",
        "https://news.un.org/feed/subscribe/zh/news/all/rss.xml",
        Some("https://news.un.org/zh/"),
        NewsCategory::World,
        "联合国官方中文稿源（替代已下线的路透中文）",
        "2026-10-05",
    ),
    (
        "IT之家",
        "https://www.ithome.com/rss/",
        Some("https://www.ithome.com/"),
        NewsCategory::Tech,
        "科技资讯与产品动态，条目多、更新快（替代已下线的 36 氪）",
        "2026-10-05",
    ),
    (
        "极客公园",
        "http://www.geekpark.net/rss",
        Some("https://www.geekpark.net/"),
        NewsCategory::Tech,
        "科技产品与创投观察",
        "2026-10-05",
    ),
    (
        "少数派",
        "https://sspai.com/feed",
        Some("https://sspai.com/"),
        NewsCategory::Tech,
        "效率工具与应用推荐",
        "2026-10-05",
    ),
    (
        "阮一峰的网络日志",
        "https://www.ruanyifeng.com/blog/atom.xml",
        Some("https://www.ruanyifeng.com/blog/"),
        NewsCategory::Tech,
        "科技周刊，长期高质量（周更）",
        "2026-10-05",
    ),
    (
        "Hacker News Front Page",
        "https://hnrss.org/frontpage",
        Some("https://news.ycombinator.com/"),
        NewsCategory::Tech,
        "技术与创业社区每日头条（英文）",
        "2026-10-05",
    ),
];
