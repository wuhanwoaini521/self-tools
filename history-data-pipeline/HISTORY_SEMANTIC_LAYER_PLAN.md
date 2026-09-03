# History Semantic Layer V1 实施计划与实际 Schema

## 1. Schema 检查结论

检查对象：`data/normalized/history.duckdb`（真实本地正式库，构建版本 20260829）。

| 表 | 当前行数 | V1 处理 |
|---|---:|---|
| `people` | 676,426 | 只读复用 canonical ID；不创建第二套人物 |
| `person_aliases` | 208,624 | 保持不变 |
| `person_relations` | 561,461 | 保持源关系码；新增关系字典，不覆盖原码 |
| `person_place` | 460,402 | 保持不变 |
| `places` | 31,978 | 只读复用；不可靠地点使用 `needs_linking` |
| `works` | 27,394 | 只读复用 |
| `historical_texts` | 972,467 | 只读复用三层文本和 `source_id` |
| `sources` | 5 | 新增 `source-curated-semantic-v1`，明确为人工整理来源 |
| `entity_source_mapping` | 773,937 | 不改既有 mapping |
| `data_review` | 5 | 保留生卒异常，新增 57 条自关系 Review |
| `periods` / `regimes` | 0 / 0 | 建立 V1 curated reference |
| `events` / `stories` | 0 / 0 | 建立 V1 三条 Story 与事件链 |

原库已提前存在空的 `events`、`stories`、`story_events`、`event_relations`、`event_person`、`event_place` 表，但列不足以表达来源和未链接地点。迁移只扩展语义列；`event_place` / `story_place` 使用可空 `place_id` 的 V1 桥接结构并保留旧列语义。

## 2. 复用与新增边界

复用：人物、人物别名、人物关系原始码、地点、作品、HistoricalText、Source、既有 Review。

新增/扩展：

- `relation_type_dictionary`：从 CBDB SQLite 官方 `KINSHIP_CODES` 读取 488 个码，保留原始码、官方原名、官方中文名、配对关系和源文件定位。
- `periods`：学习浏览时期，与具体政权分离；采用 BCE 负整数和 `date_precision`。
- `regimes`：具体政权；包含 `period_id`、父政权、年代精度和质量状态。
- `events`：结构化历史节点，使用 `period_id` / `regime_id` 与 `source_ids`。
- `event_person`、`event_place`、`event_text`：事件到人物、地点、HistoricalText 的桥接。
- `event_relations`：只在 curated 文件显式声明先后/因果/发展关系，不由时间自动推导因果。
- `stories`、`story_events`、`story_person`、`story_place`：故事聚合和阅读顺序。

不改：Raw、Staging、History UI、既有人物/地点/文本事实与原始 `relation_type`。

## 3. Curated 数据

人工整理文件位于 `data/curated/`：

- `periods.yml`
- `regimes.yml`
- `stories.yml`

每个语义实体写入 `source_type=curated_reference` 和 `source_ids=["source-curated-semantic-v1"]`；该来源不会冒充 CBDB、CText 或 Classical-Modern。

## 4. 质量规则

- 生年晚于卒年的既有 5 条记录继续保持 pending，不改 Raw。
- 57 条 self relation 全部写入 `data_review` 和 `reports/self_relation_review.csv`，分类为 `unknown`，因为仅凭 Canonical 结果不能证明是源数据事实、ETL 映射错误、canonical merge 或语义特例。
- Story 只有在事件数不少于 5、Canonical 人物不少于 3、至少一个 Place 或 `needs_linking`、至少一个 HistoricalText、来源覆盖 100% 时才标记 `usable`。
- `precedes/follows` 表示顺序；`causes/leads_to` 只有 curated 关系文件显式提供时才写入。

## 5. 执行方式

真实库执行：

```powershell
$py = 'history-data-pipeline\.venv\Scripts\python.exe'
& $py -c "import sys; sys.path.insert(0, 'history-data-pipeline/src'); from pathlib import Path; from history_data_pipeline.semantic_layer import build_semantic_layer, write_story_samples, write_semantic_report; root=Path('history-data-pipeline'); db=root/'data/normalized/history.duckdb'; result=build_semantic_layer(db, root); write_story_samples(db, root); write_semantic_report(db, root, result)"
```

正式构建流程 `history-data build --from-staging` 会在新正式库生成后自动执行同一语义层构建。该过程幂等，可重复运行。
