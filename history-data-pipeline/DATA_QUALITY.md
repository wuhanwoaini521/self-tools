# DATA_QUALITY

## 状态

`verified`、`source_backed`、`reviewed`、`machine_generated`、`conflicting`、`incomplete`、`unverified`。

V1 不把“数据集存在”当作“人工验证”：NiuTrans 的句对默认 `alignment_quality=heuristic_unverified`；CHGIS 地理数据在获得许可并导入前不进入规范库；Wikimedia 介绍只能作为来源明确的候选字段。

## 自动规则

- 主实体 ID 唯一且不可为空；所有桥接表目标必须存在。
- 出生年不晚于卒年；违反者进入 `data_review`，不覆盖 Raw/Canonical，也不阻断只读查询。
- Story 事件序号唯一且有序；`usable=true` 需要至少 3 个事件、2 个人物、1 个地点、1 个来源。
- `original_text` 非空；`translation_zh_cn == original_simplified` 作为需分布分析的候选，不自动判错或删除。
- 任何合并必须有 `entity_source_mapping` 与置信度；姓名相同不自动合并。
- 冲突值进入 `fact_assertions`，不得简单覆盖。

运行：

```powershell
python -m src.history_data_pipeline build --sample
python -m src.history_data_pipeline validate
```
