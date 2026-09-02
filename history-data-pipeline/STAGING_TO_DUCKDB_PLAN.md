# STAGING_TO_DUCKDB_PLAN

## 当前输入

| staging | 实际记录数 | Schema/处理 |
|---|---:|---|
| `cbdb/people.jsonl` | 661,124 | `id`, 中英文名、出生/卒年、精度、性别、朝代；转换为稳定 `cbdb-person-<c_personid>` |
| `cbdb/person_aliases.jsonl` | 208,630 | 一行一个别名；保留 `source_id` 与 `external_id` |
| `cbdb/places.jsonl` | 30,100 | 名称、坐标、类型、有效年代；现代名/geometry 保留 NULL |
| `cbdb/dynasties.jsonl` | 85 | 直接进入 Dynasty；不强行转换为 Period/Regime |
| `cbdb/person_place.jsonl` | 460,772 | 由 `BIOG_ADDR_DATA` 产生，桥接目标校验后进入 `person_place` |
| `cbdb/person_relations.jsonl` | 561,461 | 由 `KIN_DATA` 产生；保留 CBDB 关系码，不猜中文关系 |
| `ctext/entities.jsonl` | 91,297 | `person/work/place` 进入 Canonical；`authority-cbdb` 作为直接匹配依据 |
| `classical-modern/20240421.jsonl` | 972,467 | 流式转换为 HistoricalText；OpenCC 派生简体，译文独立保存，`alignment_quality=heuristic_unverified` |

## 真实质量观察

- CBDB 人物、别名、地点、朝代字段适合第一版直接转换；简介、Period、Regime、Event 不在当前 staging 中。
- CBDB 生卒年存在缺失和精度差异，必须保留 `NULL` 与 `birth_precision/death_precision`。
- CBDB 地址与亲属关系可进入桥接表；关系码先原样保存，后续人工建立码表。
- CText 可可靠提供 source-native person/work/place 和外部 ID；CText↔CBDB 直接人物匹配共 38,236 条。
- NiuTrans 原文、译文和行号可直接使用；句对来自启发式分句/对齐，不能标记 verified。
- Wikipedia/Wikisource 只有 Raw，尚未进入 Canonical 结构化表；CHGIS 因许可缺失未进入。

## 构建顺序

```text
Raw metadata → sources
CBDB/CText/NiuTrans staging → transform → history.building.duckdb
SQL integrity/NULL/duplicate checks
quality exceptions recorded → atomic replace history.duckdb
reports + samples + Parquet exports
```

失败时保留原 `history.duckdb`；成功时旧库改名为 `history.previous.duckdb`。所有 Canonical ID 均由外部 ID 或确定性 hash 派生，可重复构建。
