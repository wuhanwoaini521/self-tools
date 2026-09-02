# HISTORY_QUERY_CONTRACT

第一版只读应用契约。所有结果来自 `data/normalized/history.duckdb`；字段缺失时返回 `null`、空数组或空字符串，不补造事实。

## PersonResult

`id`, `canonical_name_zh_cn`, `name_raw`, `traditional_name`, `birth_year`, `death_year`, `birth_precision`, `death_precision`, `gender`, `period_ids`, `regime_ids`, `dynasty_ids`, `intro_zh_cn`, `quality_status`, `created_from_source`, `search_name`, `search_aliases`, `search_text`, `pinyin`, `initials`, `aliases`, `source_mappings`。

`aliases` 是 `PersonAliasResult[]`；`source_mappings` 包含 `source_id`, `external_id`, `match_type`, `confidence` 以及关联 Source 的 dataset、dataset_version、license、original_url、raw_path、staging_path。

## PersonAliasResult

`person_id`, `alias`, `alias_zh_cn`, `alias_type`, `source`, `source_id`, `external_id`。`alias_type` 保持数据集原始值，不重分类。

## PersonRelationResult

`person_a_id`, `person_a_name`, `person_b_id`, `person_b_name`, `relation_type`, `start_year`, `end_year`, `description`, `source_ids`, `confidence`, `direction`, `source_dataset`。当前 `relation_type` 是 CBDB 原始数字代码，未创建未经来源支持的中文语义映射。

## PersonPlaceResult

`person_id`, `place_id`, `place_name`, `historical_name`, `modern_name`, `longitude`, `latitude`, `place_type`, `valid_from`, `valid_to`, `relation_type`, `start_year`, `end_year`, `source_id`, `external_id`, `quality_status`。`relation_type` 保持来源原始代码。

## WorkResult

`id`, `title`, `title_raw`, `title_zh_cn`, `author_ids`, `period`, `book_type`, `description`, `source_ids`, `source_id`, `quality_status`, `sources`。

## HistoricalTextResult

`id`, `title_zh_cn`, `book_id`, `work_title`, `chapter`, `section`, `original_text`, `original_simplified`, `translation_zh_cn`, `notes_zh_cn`, `intro_zh_cn`, `translation_type`, `translation_source`, `quality_status`, `source_id`, `alignment_quality`, `source`。

## SourceResult

`id`, `dataset`, `original_id`, `original_url`, `snapshot_version`, `snapshot_date`, `dataset_version`, `source_type`, `license`, `retrieved_at`, `raw_file`, `raw_path`, `staging_path`, `quality`, `quality_status`, `commercial_use`, `redistribution`, `attribution`, `notes`。

## StatsResult

`counts`, `historical_texts_complete`, `translation_equals_simplified`, `source_coverage`, `birth_after_death`, `broken_person_relations`, `broken_person_places`, `self_person_relations`, `review_queue_pending`。

## CLI

```text
python -m src.history_data_pipeline query person "曹操"
python -m src.history_data_pipeline query person "苏轼" --relations
python -m src.history_data_pipeline query person "曹操" --places
python -m src.history_data_pipeline query work "史记"
python -m src.history_data_pipeline query text --work "史记" --limit 10
python -m src.history_data_pipeline query source
python -m src.history_data_pipeline query stats
python -m src.history_data_pipeline query person "曹操" --json
```
