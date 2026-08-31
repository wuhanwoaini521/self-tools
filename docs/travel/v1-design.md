# Travel 模块设计档案（V1）

> 状态：已实现（2026-08）。本文档为设计决策记录，供后续扩展参考，不一定与代码逐行同步；以代码为准。

## 核心原则

```
AI 负责整理信息，而不是凭空提供信息。
```

- 所有重要事实尽可能带来源（Sources 可查看、可打开）；
- 缺失字段显示「暂无可靠数据」，LLM 无事实输入时禁止编造；
- China First：搜索 / 数据源优先国内可访问；海外服务仅作后续 optional fallback；
- 不做反爬：不实现验证码破解 / WAF 绕过 / 指纹伪装 / 账号登录；抓不到全文就保留搜索摘要（低可信）。

## 分层与模块（按现有 crates 三层层级）

```
React (ui/src/features/travel/{TravelPage,TravelProgress,TravelGuide}.tsx)
   ↓ invoke
Tauri 命令 (apps/desktop/src/lib.rs — travel_research_start/progress/recent_guides/load_guide)
   ↓
Application (crates/application/src/travel/service.rs — TravelResearchService)
   ↓
Domain 纯规则 (crates/core/src/travel/)
   ├── model.rs          SearchResult / TravelDocument / TravelFact / SourceLevel / 进度事件
   ├── guide.rs          CityGuide 结构化模型 + VerifiedValue(多源验证值) + VerifiedFact
   ├── query_planner.rs  TravelQueryPlanner（确定性规则模板 + 偏好/月份扩展，独立模块）
   ├── ranking.rs        SourceAuthority(规则分类 S/A/B/C) + freshness/relevance/final 评分
   ├── dedup.rs          搜索结果 URL 归一化去重 / 事实去重 / verify_facts 冲突检测（官方优先）
   ├── llm_parse.rs      LLM JSON 容错解析（剥围栏 / 缺字段默认 / 非法 JSON → Err）
   └── cache.rs          TTL 常量：Guide 24h / Search 24h / Document 7d
   ↓
Infrastructure (crates/infrastructure/src/travel/)
   ├── search.rs         trait SearchProvider + Bing中国(主) / 百度(备) / SearXNG(自托管)
   ├── fetcher.rs        trait WebFetcher + HttpWebFetcher（reqwest + GBK 解码 + 去噪正文提取）
   ├── llm.rs            trait LlmProvider + OpenAI-Compatible（DeepSeek/Qwen/Ollama /v1）
   ├── data_provider.rs  trait TravelDataProvider + 高德 POI（地点搜索）+ 和风天气（3天预报）；
   │                     Key 可选：未配置 → 该阶段跳过，核心流程照常（增强路径）
   └── store.rs          TravelStore（SQLite：travel_guides + 搜索/文档缓存）
```

## 主流程（TravelResearchService::research_city）

1. 识别城市（空名 → `EmptyCity`）
2. 攻略缓存命中（24h，`force` 跳过）→ 直接返回
3. `TravelQueryPlanner::plan` 规划搜索任务；LLM 可选追加主题
4. 逐任务搜索：24h 缓存 → Provider 链 fallback（单 Provider 失败不阻塞）
5. 结果按 URL 去重 → `rate_source` 综合评分排序 → 取前 20
6. 并发抓取（分组限流 ≤6）；失败保留 snippet（SnippetOnly）→ Partial Success
7. LLM 逐文档提取 `TravelFact`（JSON 解析失败跳过该文档）
8. 结构化数据 Provider 合并（V1 通常为空）
9. `verify_facts`：按 (category, subject) 分组，官方/权威优先 + 多数一致 → VerifiedFact
10. LLM 生成结构化 CityGuide JSON → 解析 → **程序层合并已验证事实**（防 LLM 遗漏）
11. 降级路径：LLM 未配置 / 失败 → 「来源 + 标题级条目」模式，meta.notes 说明原因
12. `upsert_guide` 落库（SQLite）

## 错误语义

- 空城市 → `travel_empty_city`
- 所有搜索 Provider 全挂 → `travel_failed`（前端显示失败）
- 其余一律 Partial Success：单个搜索 / 抓取 / LLM / 数据源失败只记入 `meta.notes`
- 缓存 JSON 损坏 → 视为 miss，重新研究，不崩溃

## Progress（需求 #十六）

- `TravelResearchEvent { phase, status, message, seq }` 为纯数据（未来可直接转 Tauri Event）
- V1 采用「会话 + 短轮询」：`travel_research_start` 返回 session_id（后台任务），
  前端 600ms 轮询 `travel_research_progress`；复用现有“无 Tauri emit”的机制，未引入新事件通道

## 配置（settings.json，全部 Optional）

| 字段 | 说明 |
|---|---|
| `travel.search_backend` | auto / searxng / baidu / bing（SearXNG 不可用自动回退 Bing） |
| `travel.searxng_url` | 如 `http://localhost:8080` |
| `travel.llm_base_url / llm_model / llm_api_key` | OpenAI-Compatible；留空 → 降级模式 |
| `travel.amap_api_key` | **已接入**：高德 POI（地点搜索，景点/美食/住宿）→ 补充 Attraction/Food/Accommodation 事实并进入 Sources |
| `travel.qweather_api_host / qweather_api_key` | **已接入**：和风天气专属 API Host + Key，3 天预报 → 逐日天气卡片；仅展示 API 实际预报窗口 |
| `travel.baidu_map_api_key` | 预留（V2 地点检索） |

Key 只保存在本机 settings.json（个人桌面工具），不硬编码、不提交。
未配置任何 Key 时「结构化数据源」阶段显示 Skipped，并提示可增强路径。

## 日期范围与天气展示

- Travel 输入区支持一个起止日期范围控件；范围为 1～7 天时自动同步行程天数。
- 该范围会进入查询规划、LLM 补充查询和攻略生成提示，用于检索对应日期的活动、天气和客流信息。
- 日期范围攻略不复用“城市 + 天数”的普通 24 小时缓存，避免不同出行日期得到错误推荐。
- 和风天气当前接入的是近期 3 天预报；若用户所选日期不在预报窗口，界面明确提示，并仍按所选范围生成非实时的季节、活动和客流建议。

## 测试矩阵（现状：108 项全绿）

- core 42 项（Travel 35）：query planner、authority/freshness/relevance、去重、冲突检测、LLM JSON 解析、TTL
- infrastructure 46 项（新增 20）：Bing/百度/SearXNG 解析、正文提取、GBK 解码、chat 响应解析、
  TravelStore CRUD + TTL + 损坏容错、**高德 POI 解析（含错误状态）、和风定位/预报解析、providers_for 装配**
- application 20 项（新增 16，全部 Mock）：正常全流程 / 搜索全挂 / Provider fallback / 部分网页失败
  保留 snippet / LLM 失败降级 / 非法 JSON 容忍 / 冲突官方优先 / 无 LLM / 缓存命中 / 空城市 / 无结果 /
  **数据源事实补进攻略与 Sources、数据源失败 Partial Success、无 Key 阶段 Skipped**

## V1 明确不做（后续阶段）

复杂 GIS / 导航 / 真实路线规划 / 订票 / 复杂地图 / 多 Agent 并发框架 / 向量数据库 / RAG /
百度地图地点检索（Key 已预留输入框）；`BrowserFetcher` 仅作 HTTP 失败时的后续 fallback。
