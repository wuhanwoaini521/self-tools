# HISTORY Semantic Query Contract V2

查询层仍是 `HistoryQueryService`，不是第二套 Graph/Query 系统。所有返回值均来自 DuckDB SELECT；Rust `HistoryDuckDbRepository` 提供同名只读入口。

## 基础结果

### PeriodResult

`id`, `name_zh_cn`, `name_raw`, `start_year`, `end_year`, `date_precision`, `description_zh_cn`, `quality_status`, `source_type`, `source_reference`, `source_ids`

### RegimeResult

`id`, `name_zh_cn`, `name_raw`, `start_year`, `end_year`, `date_precision`, `period_id`, `parent_regime_id`, `capital_place_id`, `parent_dynasty_id`, `description_zh_cn`, `quality_status`, `source_type`, `source_reference`, `source_ids`

### StoryResult

`id`, `title_zh_cn`, `title_raw`, `start_year`, `end_year`, `summary_zh_cn`, `background_zh_cn`, `result_zh_cn`, `story_type`, `importance`, `period_id`, `period_ids`, `quality_status`, `source_type`, `source_ids`, `usable`

### StoryEventResult

`story_id`, `event_id`, `sequence`, `role`, `importance`, `transition_text_zh_cn`, `quality_status`，以及事件的 `name_zh_cn`, `event_type`, `start_year`, `end_year`, `date_precision`, `summary_zh_cn`, `result_zh_cn`, `event_quality_status`, `source_type`, `source_ids`。

### EventResult

`id`, `name_zh_cn`, `name_raw`, `event_type`, 起止年月日、`date_precision`, `period_id`, `period_ids`, `dynasty_ids`, `regime_id`, `regime_ids`, `summary_zh_cn`, `background_zh_cn`, `result_zh_cn`, `importance`, `quality_status`, `source_type`, `source_reference`, `source_ids`。

### EventRelationResult

`source_event_id`, `target_event_id`, `relation_type`, `confidence`, `description_zh_cn`, `source_type`, `source_id`, `source_ids`, `quality_status`, `source_event_name`, `target_event_name`。

### EventPersonResult

桥接字段 `event_id`, `person_id`, `role`, `role_zh_cn`, `side`, `importance`, `description`, `source_type`, `source_id`, `quality_status`, `link_quality_status`, `link_confidence`, `link_reason`，加 canonical person 的 `canonical_name_zh_cn`, `name_raw`, 生卒年和 `person_quality_status`。

### EventPlaceResult

桥接字段 `event_id`, `place_id`, `place_name_raw`, `role`, `sequence`, `description_zh_cn`, `source_type`, `source_id`, `quality_status`, `link_status`, `link_quality_status`, `link_confidence`, `link_reason`，加 Place 的名称、历史名、现代名和坐标。`place_id=null` 且 `link_status=needs_linking` 是合法状态。

### EventHistoricalTextResult

`event_id`, `historical_text_id`, `role`, `sequence`, `description_zh_cn`, `source_type`, `source_id`, `quality_status`, `source_quality_status`, `link_quality_status`, `link_confidence`, `link_reason`, `temporal_score`, `person_score`, `place_score`, `keyword_score`, `work_score`, `context_score`, `chapter_score`，以及 `title_zh_cn`, `book_id`, `work_title`, `chapter`, `original_text`, `original_simplified`, `translation_zh_cn`, `translation_source`, `alignment_quality`。

`event_text_candidates` 用于 QA 审计被选中、待复核和拒绝的文本候选；候选被拒绝不等于其来源不存在，`source_quality_status` 与 `link_quality_status` 必须分开解释。

## 聚合结果

### StoryDetailResult

```json
{
  "story": "StoryResult",
  "events": ["StoryEventResult，按 sequence 升序"],
  "key_people": ["EventPersonResult 的去重人物聚合"],
  "key_places": ["EventPlaceResult 的去重地点聚合"],
  "historical_texts": ["EventHistoricalTextResult 的去重文本聚合"],
  "sources": ["SourceResult 的去重来源聚合"]
}
```

不在 Story 结果中复制整个人物、地点或文本实体；通过 `person_id`、`place_id`、`historical_text_id` 和 `event_id` 保持可追溯性。

## Python 查询入口

```text
list_periods()
get_period(query)
get_regime(query)
list_regimes_by_period(period)
list_stories(period=None)
get_story(query)
get_story_events(story)
get_story_people(story)
get_story_places(story)
get_event(query)
get_event_people(event_id)
get_event_places(event_id)
get_event_relations(event_id)
get_event_texts(event_id)
get_relation_type_dictionary()
```

## CLI

```bash
history-data query periods --json
history-data query stories --json
history-data query story "楚汉争霸" --json
history-data query story "安史之乱" --events --json
history-data query event "赤壁之战" --json
history-data query event "赤壁之战" --people --json
history-data query event "赤壁之战" --texts --json
```

CLI 的 `--json` 结果是上述结构的真实 DuckDB 查询结果；`--events/--people/--texts` 为前端调用保留的表达性参数，详细接口按结果对象中的对应字段返回。
