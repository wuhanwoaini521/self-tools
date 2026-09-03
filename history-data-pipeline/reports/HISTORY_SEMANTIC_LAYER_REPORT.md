# HISTORY Semantic Layer V1 Report

构建时间：2026-09-03T01:27:08.292741Z。本报告针对真实 DuckDB `D:\code\self-github\self-tools\history-data-pipeline\data\normalized\history.duckdb` 生成。

## 数据范围与原则

本阶段只新增/更新三个 Story 的 curated semantic 数据和正式库语义表；Raw/Staging 未修改。人工语义事实标记为 `source_type=curated_reference`，HistoricalText 链接回溯到现有文本与 Source。

## Relation Dictionary

- CBDB 官方 `KINSHIP_CODES` 关系码：488（当前 person_relations 实际使用 438 个）
- 已解析为可读中文：488
- 未解析中文字段：0
- 当前实际使用但未匹配：0
- Codebook 缺失：0
- 读取依据：CBDB SQLite 的 `KINSHIP_CODES`，未按数字代码猜义。

## Self Relation Review

- 自关系总数：57
- reviewed：0
- pending：57
- 57 条均未凭结构自动判定，等待对照 KIN_DATA/source 复核。

## Semantic Counts

- relation_type_dictionary: 488
- periods: 27
- regimes: 29
- events: 26
- stories: 3
- story_events: 26
- event_person: 58
- event_place: 26
- event_relations: 23
- event_text: 26
- event_text_candidates: 29
- story_person: 18
- story_place: 23

## Story Coverage

| Story | Events | People | Valid People | Places | Linked Places | HistoricalTexts | Reviewed/Verified Texts | Rejected Candidates | Sources | usable |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 楚汉争霸 | 9 | 5 | 5 | 9 | 1 | 9 | 9 | 1 | 100% | usable |
| 三国格局形成 | 8 | 6 | 6 | 8 | 0 | 8 | 8 | 0 | 100% | usable |
| 安史之乱 | 9 | 7 | 7 | 6 | 2 | 9 | 9 | 2 | 100% | usable |

## Source Coverage

| Entity / relation | With source | Total | Coverage |
|---|---:|---:|---:|
| Event | 26 | 26 | 100.00% |
| Story | 3 | 3 | 100.00% |
| EventPerson | 58 | 58 | 100.00% |
| EventPlace | 26 | 26 | 100.00% |
| EventRelation | 23 | 23 | 100.00% |
| EventText | 26 | 26 | 100.00% |

## Q&A

- Q1：三个 Story 是否 usable？是，三个均为 usable。
- Q2：修复 Person 错链 1 条；正式 Person rejected=0，needs_review=0。
- Q3：修复 Place 错链 5 条；可靠 linked=3，needs_linking=18，rejected 原始候选=5。
- Q4：修复/拒绝已知 HistoricalText 错链 3 条；候选 rejected=3。
- Q5：HistoricalText candidate 等待 Review 0 条；正式 event_text 中 candidate=0。
- Q6：正式 EventText 跨时代冲突 0 条；不存在明显跨时代正式链接。
- Q7：三条 Story 涉及 canonical_name_zh_cn 未统一简体数为 0；name_raw 未覆盖。
- Q8：当前 Semantic Layer 可以安全作为 History UI V2 的只读数据源。

READY_FOR_HISTORY_UI_V2 = true

## 复核与缺口

- `reports/self_relation_review.csv` 保留 57 条自关系逐条待复核。
- EventText 候选评分和拒绝记录保留在 `event_text_candidates`；正式 `event_text` 只包含 reviewed/verified 链接。
- 事件叙述不是 LLM 生成文章；页面应使用 StoryEvent 顺序、EventRelation 和桥接表重建历史链。
- 未可靠匹配的地点以 `place_id=NULL`、`link_status=needs_linking` 保留，不强行绑定错误 Place。
