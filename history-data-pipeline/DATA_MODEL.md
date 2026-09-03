# DATA_MODEL

数据库文件为 `data/normalized/history.duckdb`。所有列表字段在 DuckDB 中以 JSON 字符串保存，避免丢失源 ID；后续可按访问模式拆成桥接表。

## 实体

| 表 | 核心字段 |
|---|---|
| `periods` | `id`, `name_zh_cn`, 年代、`date_precision`, `quality_status`, `source_type`, `source_reference`, `source_ids` |
| `dynasties` | `id`, `name_zh_cn`, `name_raw`, 年代、父级、`date_precision`, `confidence` |
| `regimes` | `id`, `name_zh_cn`, 年代、`date_precision`, `period_id`, `capital_place_id`, `quality_status`, `source_reference` |
| `stories` | `id`, `title_zh_cn`, 年代、摘要、`story_type`, `usable`, `quality_status`, `source_reference` |
| `events` | `id`, `name_zh_cn`, 起止年月日、`date_precision`、摘要/背景/结果、`quality_status`, `source_reference` |
| `people` | `id`, 标准名、原名、字号、出生/卒年及精度、时期/政权/朝代、简介、搜索字段 |
| `person_aliases` | `person_id`, `alias`, `alias_type`, `source` |
| `places` | `id`, 历史名、现代名、坐标/geometry、类型、有效年代、父地点 |
| `historical_texts` | `original_text`, `original_simplified`, `translation_zh_cn`, 书卷章、译文类型、质量与对齐质量 |
| `event_text` | `event_id`, `historical_text_id`, `role`, `source_id`, `source_quality_status`, `link_quality_status`, `link_confidence`, `link_reason`, 七项评分、`quality_status` |
| `event_text_candidates` | EventText 候选及 temporal/person/place/keyword/work/context/chapter 分数；保留 `rejected_temporal_conflict` 审计记录 |
| `relation_type_dictionary` | CBDB `source_relation_code`、官方中英文语义、配对关系、`source_reference` |
| `works` | 书名、作者、时期、类型、来源 |
| `sources` | 数据集、原始 ID/URL、快照版本/日期、License、Raw 文件、质量能力 |
| `fact_assertions` | 主体、谓词、值、来源、置信度、偏好值；用于保存冲突而不覆盖事实 |
| `data_review` | 数据异常登记、当前值/源值、复核状态、复核备注；不覆盖 Raw 或 Canonical 事实 |

## 关系

- `story_events(story_id, event_id, sequence, role, importance)`：故事事件顺序。
- `story_person` / `story_place`：故事关键人物和地点；地点可为 `NULL` 并标记 `needs_linking`。
- `event_relations(source_event_id, target_event_id, relation_type, confidence, source_ids)`：`causes`, `caused_by`, `leads_to`, `preceded_by`, `followed_by`, `part_of`, `related_to`, `accelerates`, `weakens`, `strengthens`。
- `person_relations(person_a_id, person_b_id, relation_type, start_year, end_year, source_ids)`：亲属、师生、君臣、盟友、敌对等。
- `event_person(event_id, person_id, role, side, importance)`。
- `event_place(event_id, place_id, place_name_raw, role, sequence, link_status, link_quality_status, link_confidence, link_reason)`；无法可靠匹配时 `place_id=NULL`、`link_status=needs_linking`。
- `event_person(event_id, person_id, role, side, importance, link_quality_status, link_confidence, link_reason)`；生卒年与事件不重叠的错误人物链接拒绝写入为有效链接。
- `entity_source_mapping(entity_type, entity_id, source_id, external_id, match_type, confidence)`：canonical entity 到原始实体的可追溯映射。
- `text_alignments(text_id, line_number, original_text, translation, alignment_quality)`：句级古文/译文对齐。

## 时间和文本不变量

- BCE 核心值使用负整数，例如前 221 年为 `-221`；不使用“前221”作为排序字段。
- `date_precision` 至少区分 `exact`, `year`, `range`, `approximate`, `before`, `after`, `unknown`。
- `original_text` 永远保留 Raw 原文；OpenCC 只写 `original_simplified`。
- `translation_zh_cn` 独立保存，`translation_type` 区分 `human`, `published`, `dataset`, `ai`, `ai_reviewed`, `unknown`。
