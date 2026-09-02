# DATA_SOURCES

本表区分“官方入口已核验”和“本 checkout 已实际下载”。具体版本 URL 不写入代码：CBDB、CText、Wikimedia 和 NiuTrans 均在运行时动态解析。每个成功快照的最终 URL、大小、SHA-256 和检索时间以 `data/raw/<dataset>/<version>/metadata.json` 为准。

| 名称 | 用途 | 官方主页/入口 | 当前已核验信息（2026-09-02） | License | 当前状态 | Parser/Adapter |
|---|---|---|---|---|---|---|
| CBDB | 人物、籍贯、官职、关系 | [cbdb_sqlite](https://github.com/cbdb-project/cbdb_sqlite) / [latest.json](https://github.com/cbdb-project/cbdb_sqlite/raw/refs/heads/master/latest.json) | `cbdb_20260829.sqlite3`；官方 SQLite SHA-256 `f620ca1a4c794411b81d5039adf5756df129fb9aa0f4509b4c843bb66e0caa2a`；本地 SQLite 585,605,120 bytes，ZIP 138,994,057 bytes | 以项目/快照随附说明为准 | 已下载并校验；staging：661,124 人、208,630 别名、30,100 地点、85 朝代 | `CBDBDownloader` |
| CText Data Wiki | 实体、古籍、地点、关系 | [Linked Open Data](https://ctext.org/tools/linked-open-data) | `ctext_datawiki-2026-01-22.ttl.zip`；本地 7,111,898 bytes；SHA-256 `a3b4c3a50ba024ad2349142b7f746a3c6e0fd1f209432ae75fba6d722fafa8d5` | CC BY-NC-SA 3.0 | 已下载并校验；staging：91,297 个实体 | `CTextDownloader` |
| NiuTrans Classical-Modern | 古文/现代文平行语料 | [Classical-Modern](https://github.com/NiuTrans/Classical-Modern) | 官方仓库 README：v2.0（2023-03）；本地 ZIP 302,269,130 bytes；SHA-256 `a09e9f735a814bc521faadb05a6b91deab0be56f3e8cdff9d04aa0579bd29b49`；流式解析 972,467 句对 | 仓库 MIT；数据文件按 `数据来源.txt` 逐项核验 | 已下载并解析；`alignment_quality=heuristic_unverified` | `ClassicalModernDownloader` |
| CHGIS V4 | 历史行政区与地点 | [数据下载](https://yugong.fudan.edu.cn/CHGIS/sjxz.htm) / [版权声明](https://yugong.fudan.edu.cn/CHGIS/bqsm.htm) | 官方页列出 TS、1820、1911、DEM；中国大陆使用需复旦许可 | 非商业学术研究；商业需另行协议；禁止未经书面同意电子再发布 | 只允许手动导入 | `CHGISManualImporter` |
| 中文 Wikipedia Dump | 现代介绍候选 | [zhwiki latest](https://dumps.wikimedia.org/zhwiki/latest/) | `zhwiki-latest-pages-articles-multistream.xml.bz2`；本地 1,093,582,476 bytes；SHA-256 `b89f5827b322f93e337ed50cf6b0817f1df794daf816ed9f7ae3299ead4721ef` | CC BY-SA 4.0 + GFDL 1.3 | 已下载并校验；尚未解析 | `WikipediaDumpDownloader` |
| 中文 Wikisource Dump | 原始史料候选 | [zhwikisource latest](https://dumps.wikimedia.org/zhwikisource/latest/) | `zhwikisource-latest-pages-articles-multistream.xml.bz2`；本地 617,840,267 bytes；SHA-256 `c42f52fe0e87fb52b1ba1b4d5342a68d8fd09e5bb9319afdcd92bfafa49a7233` | Wikimedia 文本许可，发布前按页面/贡献条款复核 | 已下载并校验；尚未解析 | `WikisourceDumpDownloader` |

## 下载协议

1. 运行时先验证官方 URL，再解析最新版本与文件名。
2. 下载到 `data/raw/<dataset>/<version>/`，存在同版本快照时拒绝覆盖。
3. 下载完成后执行 SHA-256；官方提供 SHA-256/MD5 时额外比对。
4. 记录文件大小、URL、版本、许可、检索时间和 Raw 路径。
5. Wikimedia 仅选择 `pages-articles-multistream.xml.bz2`，找不到时才回退到 `pages-articles.xml.bz2`；不取 images/history/logging。

上述文件均已下载并保存为 Raw Snapshot；当前 `history.duckdb` 已由真实 CBDB/CText/Classical-Modern staging 构建。`build --sample` 仍只生成内部验证数据，不能覆盖或替代正式库。
