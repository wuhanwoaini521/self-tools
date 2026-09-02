# IMPLEMENTATION_SUMMARY

## 已完成

- 新增独立 `history-data-pipeline/`，与现有 Rust/Tauri 和 History UI 解耦。
- 实现六类源的官方入口配置与独立 Downloader；CBDB 从 `latest.json` 取当前文件名、SHA-256、生成日期和 Hugging Face 地址；CText/Wikimedia/NiuTrans 运行时发现版本，避免写死具体版本 URL。
- 实现不可覆盖的 Raw Snapshot、`metadata.json`、`checksum.sha256`、下载大小和 UTC 检索时间记录。
- CHGIS 实现 `CHGISManualImporter`：只检测用户给出的目录，要求显式许可文本，复制后逐文件校验；不自动下载、不绕过许可。
- 实现 DuckDB Canonical Schema、文本繁简派生、句对 `alignment_quality`、Source Mapping、Fact Assertion、校验、统计报告和 Parquet 导出。
- 提供楚汉/三国/唐最小验证样本，证明 `曹操 → 官渡之战 → 赤壁之战 → 三国格局形成` 的事件链及人物、地点、史料、来源可查询。
- 没有修改现有 History 页面，也没有创建 UI 或批量生成历史内容。

## 本次实际数据

本 checkout 已下载五个官方 Raw Snapshot：CBDB SQLite 585,605,120 bytes、CText ZIP 7,111,898 bytes、NiuTrans ZIP 302,269,130 bytes、中文 Wikipedia Dump 1,093,582,476 bytes、中文 Wikisource Dump 617,840,267 bytes。CBDB、NiuTrans、CText 已解析到 staging，并已由真实 staging 构建正式 `data/normalized/history.duckdb`：676,426 人、31,978 地点、27,394 Work、972,467 HistoricalText、561,461 人物关系、460,402 人物—地点关系。查询验证、Review、JSON samples 和 Raw inventory 见 `data/reports/` 与 `HISTORY_QUERY_CONTRACT.md`。

## 当前质量与风险

- Event/Story/Period/Regime 当前保持真实为空；不要将现有 UI 种子或验证样本当作正式 DuckDB 事实。
- CBDB License 需要随具体快照/项目说明继续核验，默认 `unknown`。
- CText 是 CC BY-NC-SA 3.0，不进入商业安全包。
- CHGIS 是非商业学术用途并有地区许可要求，禁止默认公开再分发。
- NiuTrans 的句对由启发式分句/对齐产生，默认 `heuristic_unverified`，不能宣称全部人工验证。
- Wikipedia/Wikisource 的 `latest/` 入口和文件大小会变化，每次构建都应重新解析并记录。

## 下一阶段仍需做的事

- 逐条审核 `data_review` 中的 5 条 birth/death 异常，并决定是否记录为来源错误、规范化错误或保留接受；Raw 永远不变。
- 为 CBDB 关系类型代码建立有来源的字典后，再考虑应用展示语义；继续保持重名人物/地点不自动合并。
- 在核心 Query Layer 稳定后，再评估 MediaWiki XML、CHGIS、Event/Story 的后续工作；本阶段不启动这些任务。
- 做基于姓名、别名、年代、籍贯、官职、关系和外部 ID 的 Entity Resolution；`>0.95` 自动合并、`0.75–0.95` 人工复核、低于 `0.75` 分开保留。
- 由人工审核少量 Story Candidate，再扩充事件关系和史料关联；不批量让 LLM 凭记忆生成事实。

## History UI 应使用的真实字段

下一阶段 UI 应从 `periods/dynasties/regimes/stories/events/people/places/historical_texts` 及桥接表读取：标准名与别名、年代精度、摘要/背景/结果、参与角色、事件顺序、地点历史名与现代名、原文/简体/独立译文、`quality_status`、`source_ids` 和可回溯的 `entity_source_mapping`。UI 不应把 `original_simplified` 当翻译，也不应隐藏冲突事实和许可状态。
