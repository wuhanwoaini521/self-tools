# SEMANTIC Link QA V1

数据库：`D:\code\self-github\self-tools\history-data-pipeline\data\normalized\history.duckdb`。本报告由 DuckDB 重新计算生成。

## QA 规则

- EventText 候选同时计算 temporal/person/place/keyword/work/context/chapter 七项分数；作品与章节先于关键词命中参与筛选。
- 章节可明确指向晋、隋、后梁等时代且与 Event 年代无交集时，直接标记 `rejected_temporal_conflict`，不写入正式 event_text。
- Person 需要 canonical ID；已有生卒年时必须与 Event 时间重叠。缺少生卒年只能为 `reviewed`，不能伪装成已验证年代。
- Place 需要名称匹配和有效年代重叠；无法可靠匹配时统一为 `place_id=NULL`、`link_status=needs_linking`。
- `source_quality_status` 只说明文本是否有真实 Source；`link_quality_status` 单独说明文本是否与 Event 相关。

## Person Linking

| 状态 | 数量 |
|---|---:|
| linked（verified/reviewed） | 58 |
| rejected | 0 |
| needs_review | 0 |

## Place Linking

| 状态 | 数量 |
|---|---:|
| linked | 3 |
| needs_linking | 18 |
| rejected | 5 |

## HistoricalText Linking

| 状态 | Candidate 表 | 正式 event_text |
|---|---:|---:|
| verified | 10 | 10 |
| reviewed | 16 | 16 |
| candidate | 0 | 0 |
| rejected | 3 | 0 |

## Temporal Conflict

- 正式 EventText 中的跨时代链接：**0**。
- 候选集中被时间规则拒绝的候选：3。
- 这些候选不会进入正式 Story HistoricalTexts；候选记录保留在 `event_text_candidates` 供审计。

## 本轮已修正的已知错误

- Person 错链：1 条（范增不再指向范增肱，新增/使用独立 curated canonical identity）。
- Place 错链：5 条（包括范阳→邺郡、东汉洛阳、三国荆州及超出有效年代的邺郡）。
- HistoricalText 错链：3 条（入蜀/长安失守/荥阳对峙的后梁、晋、隋首条关键词命中）。

## 典型拒绝样例

| Event | Text | Work | Chapter | Confidence | Reason |
|---|---|---|---|---:|---|
| 长安失守 | text-niutrans-001916ce996444297435 | 资治通鉴 | 晋纪九 | 0.425 | rejected_temporal_conflict: chapter=晋纪九 context_range=(265, 420) event_range=756..756 |
| 唐玄宗入蜀 | text-niutrans-0012aca6d5dcc3407238 | 资治通鉴 | 后梁纪三 | 0.275 | rejected_temporal_conflict: chapter=后梁纪三 context_range=(907, 923) event_range=756..756 |
| 荥阳对峙 | text-niutrans-0220c15301a44aa90542 | 资治通鉴 | 隋纪六 | 0.425 | rejected_temporal_conflict: chapter=隋纪六 context_range=(581, 618) event_range=-205..-203 |

## 简体字段

- 三条 Story 涉及的 canonical person 名称不符合预期简体覆盖数：0。
- Raw/外部名称仍保留在 `name_raw`，本轮只更新语义层需要展示的 canonical 简体字段。

## Story 判定

| Story | Events | People | Valid People | Places | Linked Places | Reviewed/Verified Texts | Temporal Conflicts | Source Coverage | usable |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 楚汉争霸 | 9 | 5 | 5 | 9 | 1 | 9 | 0 | 100% | true |
| 三国格局形成 | 8 | 6 | 6 | 8 | 0 | 8 | 0 | 100% | true |
| 安史之乱 | 9 | 7 | 7 | 6 | 2 | 9 | 0 | 100% | true |

## 结论

- 三个 Story 全部 usable：是。
- 是否仍有明显跨时代正式 EventText：否。
- Candidate 等待 Review：0 条（正式 Story 不使用这些候选）。
- 仍需后续人工处理的主要缺口：自关系 57 条 pending，以及未经可靠映射的地点；这些不会被误展示为已链接实体。
