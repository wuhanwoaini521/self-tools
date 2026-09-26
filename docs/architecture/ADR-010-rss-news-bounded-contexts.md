# ADR-010 · RSS 与 News 的 bounded context 边界

- 状态：**Accepted**（2026-09-24，V12 重构 PASS）
- 领域：`crates/{core,application,infrastructure}` + `apps/{desktop,mcp}` + `ui`
- 关联：[ADR-004-personal-ai-module-expansion.md](ADR-004-personal-ai-module-expansion.md)
  （模块接入模式）、[ADR-007-mcp-as-adapter.md](ADR-007-mcp-as-adapter.md)
  （MCP 暴露表 = 白名单）

## 背景

第一版实现把 News 建模成 **RSS 订阅的一个分类**：

```rust
Feed { kind: Rss | News }          // feeds.kind = 'news' | 'rss'
```

后果是两个产品概念在数据、端口、命令、UI、AI 工具五层互相污染：

- `RssRepositoryPort` 长出 `list_feeds_by_kind` / `set_feed_kind` /
  `latest_articles_by_kind` —— News 的查询面塞进 RSS 端口；
- News 页面提供「改为订阅」、RSS 页面提供「改为新闻」—— 两个 bounded context
  互相转换；
- `NewsService` 直接持有 `Arc<dyn RssRepositoryPort>` —— **News 依赖 RSS 的
  repository**，News 没有自己的存储；
- 用户没订阅任何 news 源时，News 页和 RSS 页显示同一批数据，产品语义为零。

而产品定义是：

| | RSS | News |
| --- | --- | --- |
| 本质 | **个人订阅阅读器** | **新闻发现 / 聚合 / 阅读 / AI 理解** |
| 源 | 用户主动决定订什么 | 系统维护 / 推荐 |
| 能力 | 添加/删除订阅、条目阅读、已读、收藏、Folder/Tag、刷新、未来 OPML | Latest、Category、Region、Source、Search、推荐源、详情、多源聚合、未来 AI 摘要 / 事件聚合 / Timeline |
| 领域对象 | `RssSubscription` / `RssEntry` / `RssFolder` | `NewsSource` / `NewsArticle` / `NewsCategory` / `NewsTopic` |

**RSS 可以是 NewsSource 的摄取方式，但 NewsSource 不是 RSS 订阅的子类。**

## 决策

### 1. 共享基础设施，不共享领域语义

```text
   RSS Domain ──┐
                ├── Shared Feed Infrastructure
   News Domain ──┘
```

共享（`crates/core/src/feed.rs` + `crates/application/src/feed/` +
`crates/infrastructure/src/feed_fetcher.rs`）：

- `FeedFetcherPort`（HTTP 抓取端口，`+ Send` RPITIT）；
- `FetchedFeed` / `FetchedEntry` 归一化契约（三种 Feed 格式的解析结果）；
- `fetch_many()` 并发抓取骨架（**保序、不含落库**，两个域各自 commit）；
- `feed-rs` 解析、`feed_client()` 连接池 / UA / 超时、URL 归一化、
  HTML 嗅探、guid 三级回退、author / 缩略图提取。

不共享：repository 端口、用例编排、DTO、存储、AI 工具命名空间。

### 2. 两个独立的领域契约

```rust
// crates/core/src/rss.rs —— 个人订阅阅读器
FeedRow   // = RssSubscription 的载体（命名保留，避免 100+ 引用的纯 churn）
ArticleRow // = RssEntry 的载体

// crates/core/src/news.rs —— 新闻发现与阅读
NewsSource     { id, name, url, source_type, category, site_url, last_updated, last_error, unread_count }
NewsArticle    { id, source_id, source_name, guid, url, title, author, image_url, published_at, summary, is_read, starred }
NewsCategory   // general / tech / finance / world / china（产品语义）
NewsSourceType // rss / atom / json_feed / api（**摄取技术属性**，不是产品分类）
RecommendedSource + recommended_sources()  // 系统 seed 目录
```

`core::news` **零 `core::rss` 类型引用**；两者只在 `core::feed` 的共享
摄取契约处相遇。同一个 URL 同时存在于 `feeds` 与 `news_sources` 是**允许**
的（两个 context，只是底层 URL 相同）。

### 3. 两个独立的 application 用例层

```text
rss::ports::RssRepositoryPort   → feeds / articles（config/dashboard.db）
news::ports::NewsRepositoryPort → news_sources / news_articles（config/news.db）

rss::service::{RssService, RssIngestService<F>}
news::service::{NewsService, NewsIngestService<F>}
```

两边同构（同步读写 + async 摄取），但**不互相引用**：`application/src/news/`
grep `crate::rss` = 0 命中。

### 4. Personal AI 分别注册两套能力

```text
rss.list_subscriptions / rss.list_entries / rss.search / rss.get_entry
rss.mark_read(SafeWrite) / rss.refresh(SafeWrite)

news.latest / news.search / news.by_category / news.by_source / news.get_article
news.refresh(SafeWrite)
```

场景区分：「看看我关注的博客今天有什么更新」→ `rss.*`；
「今天 AI 有什么重要新闻」→ `news.*`。

风险分级由语义决定，不为 registry 放行而伪装：本地库读 = `Read`；
写本地状态或联网落库 = `SafeWrite`（`registry::allowed_risk` 只放行这两档）。

### 5. 两个独立的存储 + 一次性迁移

| 库 | 表 | 用途 |
| --- | --- | --- |
| `config/dashboard.db` | `feeds` / `articles` | RSS 订阅与条目（**现有用户数据原地不动**） |
| `config/news.db`（新） | `news_sources` / `news_articles` / `schema_meta` | 新闻源与新闻（首次打开 seed 推荐目录） |

迁移（`infrastructure/news_migration.rs`，desktop setup 与 MCP compose 都调用，
**先于** 打开两侧库）：

1. 老库 `feeds` 若存在 `kind` 列 → 把 `kind='news'` 的行**明确标记**搬进
   `news.db`（**不是 URL 猜测**）；
2. `kind='rss'` / 默认行原地不动，继续是 RSS 订阅；
3. 搬运成功后 `ALTER TABLE feeds DROP COLUMN kind`，删除错误字段；
4. `schema_meta.rss_news_migrated` 标记幂等；搬运失败**绝不 DROP**。

### 6. MCP 只读暴露 9 项

`default_exposure`（白名单，未列出 = 不暴露）：

- News 5 读：`news.latest` / `news.search` / `news.by_category` /
  `news.by_source` / `news.get_article` → `ModuleRead` + `selftools.read`；
- RSS 4 读：`rss.list_subscriptions` / `rss.list_entries` / `rss.search` /
  `rss.get_entry` → 同上；
- **不暴露**：`news.refresh` / `rss.refresh`（联网）与 `rss.mark_read`
  （本地状态写）—— MCP 入口不装配 ingest，工具本身也会如实降级。

## 后果

**正面**：

- 两个产品各有一套自洽的领域对象、端口、存储、工具命名空间；
- `application/src/news` 对 `crate::rss` 零引用（可 grep 验证）；
- 两个 `ToolRegistry` 命名空间 `news.*` / `rss.*` 互不交叉（装配测试断言）；
- 存储隔离有端到端冒烟证明：写 `news.db` 不动 `dashboard.db`，反之亦然；
- 用户 RSS 数据与行为逐字保留（迁移只搬被明确标记的行）。

**代价/约束**：

- 两个 store 各一套 CRUD 适配器（desktop / MCP 各一份）—— 这是**有意的
  重复**：共享 repository 就等于共享语义；
- 推荐源目录是 `core::news` 里的常量表（seed 唯一来源）；换目录需改代码，
  不是配置文件（P0 不做配置化，等出现用户自定义需求）；
- `FeedFetcherPort` 收紧为显式 `+ Send`（RPITIT）—— 既有实现
  （`FeedFetcherAdapter` 持 `reqwest::Client`）本就 Send，把隐含能力写成
  契约，零行为变化；
- 抓全文（`fetch_article_url`）作为**共享基础设施命令**被两个页面共用，
  不归属任一领域的用例。

## 替代方案（已否决）

| 方案 | 否决理由 |
| --- | --- |
| `Feed { kind: Rss \| News }` + 按 kind 分流 | 两个 bounded context 被压成一张表；端口、DTO、命令、UI 全部互相知道对方的语义（本次重构前的实际状态） |
| 迁移时按 URL 匹配推荐目录自动标 news | URL 猜测会误伤（用户订的新闻源未必在目录里）；违反「现有 RSS 数据保持 RSS 语义」 |
| News 继续读 `dashboard.db`（不建 news.db） | News 永远依赖 RSS repository，「独立模块」只是命名；News 页刷新会碰 feeds 表 |
| 为了去重把 `fetch_many` 也各写一份 | 抓取并发骨架是基础设施，复制它没有语义收益（语义在各自的 commit 里） |
| 给 MCP 暴露 news/rss 的写工具 | 外部 agent 不该代按本地状态、不该触发用户流量；fail-closed 是 V8 既定策略 |
