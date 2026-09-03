# 中国历史离线数据仓库 V1

这是与 Tauri/Rust 应用解耦的 Python 数据管线。它遵循：

```text
Source → Raw Snapshot → Staging → Normalize → Link → Validate → Export
```

Raw 快照只追加、不覆盖；所有规范化文本、实体和关系都可从 Raw 重新生成。V1 默认构建少量 `sample/` 验证数据，不自动编造全量 Story，也不修改现有 History UI。

## 环境

```powershell
python -m venv .venv
.\.venv\Scripts\python -m pip install -r requirements.txt
```

需要 Python 3.11+、DuckDB、PyArrow 和 pytest。OpenCC 是可选依赖；未安装时会保留原文并使用安全的最小回退映射，生产构建建议安装 `opencc-python-reimplemented`。

如需直接使用 `history-data` 命令，可在虚拟环境中执行 `python -m pip install -e .`；不安装时也可使用下方的 `python -m src.history_data_pipeline` 写法。

## CLI

在本目录执行：

```powershell
python -m src.history_data_pipeline download cbdb
python -m src.history_data_pipeline download ctext
python -m src.history_data_pipeline download niutrans
python -m src.history_data_pipeline download wikipedia
python -m src.history_data_pipeline download wikisource
python -m src.history_data_pipeline import chgis --input D:\path\to\licensed\CHGIS --version v4
python -m src.history_data_pipeline build --sample
python -m src.history_data_pipeline validate
python -m src.history_data_pipeline stats
python -m src.history_data_pipeline export

# 只读查询正式 DuckDB
python -m src.history_data_pipeline query person "曹操" --relations --places
python -m src.history_data_pipeline query work "史记"
python -m src.history_data_pipeline query text --work "史记" --limit 10 --json
python -m src.history_data_pipeline query source
python -m src.history_data_pipeline query stats
python -m src.history_data_pipeline query periods --json
python -m src.history_data_pipeline query stories --json
python -m src.history_data_pipeline query story "楚汉争霸" --json
python -m src.history_data_pipeline query story "安史之乱" --events --json
python -m src.history_data_pipeline query event "赤壁之战" --people --json
python -m src.history_data_pipeline query event "赤壁之战" --texts --json
```

## 本地构建（数据不入 Git）

原始下载、staging、DuckDB、Parquet 和报告均只保存在本机，并由 `.gitignore` 排除。拉取代码后，可使用脚本下载数据、解析清洗并构建本地数据库：

```powershell
.\scripts\build-local.ps1 -InstallDependencies
```

脚本默认下载并构建正式库所需的 CBDB、CText 和 Classical-Modern。依赖已经安装时可省略 `-InstallDependencies`；也可以通过 `-Datasets` 指定数据集。Wikipedia/Wikisource 当前支持下载快照，但尚未接入正式解析构建。

脚本执行顺序为 `download → parse → build --from-staging → validate → stats → export`。Raw 快照不会覆盖已有同版本目录，所有中间结果都可以从 Raw 重新生成。

大型 Dump 下载前请确认磁盘空间；默认仅选文章 XML，不下载 images、history 或日志。所有实际使用的 URL、版本、大小、校验和与许可写入 `DATA_SOURCES.md` 对应的快照 `metadata.json`。

## 目录

```text
config/                 数据源和规范化配置
data/raw/<dataset>/<v>/ 原始不可变快照
data/staging/           解析中间结果
data/normalized/        history.duckdb
data/exports/           Parquet
data/reports/           统计和质量报告
data/curated/            人工整理且带 source_type 的 Semantic Reference
sample/                 最小可验证数据
src/history_data_pipeline/ 独立 Adapter、Schema、校验和导出
tests/                  pytest 数据契约测试
```

## 已知边界

- CBDB 按官方 SQLite 快照导入；姓名/关系的实体解析仍需要后续规则与人工复核。
- CText Bulk RDF 只动态获取 Data Wiki 快照，V1 不把网页全文当作史料。
- NiuTrans 仓库自身说明句对由编辑距离和长度比启发式对齐生成，因此统一写入 `alignment_quality=heuristic_unverified`，不能视为全部人工验证。
- Wikipedia 用于现代介绍候选，Wikisource 用于原始史料候选；两者都不会直接覆盖 `original_text`。
- CHGIS 不自动下载、不自动公开再分发；无许可的输入会拒绝导入。

详细模型见 `DATA_MODEL.md`，质量规则见 `DATA_QUALITY.md`，许可风险见 `LICENSE_NOTES.md`；基础查询字段契约见 `HISTORY_QUERY_CONTRACT.md`，Semantic Layer V2 契约见 `HISTORY_SEMANTIC_QUERY_CONTRACT.md`，实施计划见 `HISTORY_SEMANTIC_LAYER_PLAN.md`，链接 QA 见 `reports/SEMANTIC_LINK_QA.md`，阶段报告见 `reports/HISTORY_SEMANTIC_LAYER_REPORT.md`。
