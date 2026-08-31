# Language Fixtures（真实数据子集 + 完整 attribution）

> 这些文件是 **Starter Pack**（运行时经 `crates/application/src/language/starter.rs`
> 用真实 importer 导入）与 CI 解析器测试的共享 fixture。
> 内容从官方数据集**真实抽取**（子集），**不是** LLM 生成；许可与来源逐项登记如下，
> 完整资料见 `docs/language/DATA_SOURCES.md`（Phase 3 强制 Gate）。

| 目录 | 文件 | 来源 | 版本 | 许可 | Attribution |
| --- | --- | --- | --- | --- | --- |
| `en/` | `wordnet-entries.json` `wordnet-synsets.json` | Open English WordNet | 2025 Edition (2025-12-31) | CC BY 4.0 | The Open English WordNet Team (globalwordnet/english-wordnet) |
| `en/` | `cmudict.dict` | CMUdict | cmusphinx master (0.7b 系) | 免费（注明来源） | Copyright (C) 1993-2015 Carnegie Mellon University |
| `jp/` | `jmdict.xml` | JMdict (EDRDG) | daily (2026-08-31) | CC BY-SA 4.0 | JMDict © EDRDG |
| `jp/` | `kanjidic2.xml` | KANJIDIC2 (EDRDG) | daily (2026-08-31) | CC BY-SA 4.0 (+专项条件) | KANJIDIC2 © EDRDG |
| `zh/` | `cedict.txt` | CC-CEDICT (MDBG) | release (2026-08-31) | CC BY-SA 4.0 | CC-CEDICT © MDBG；CEDICT © Paul Andrew Denisowski |
| `yue/` | `words_hk_wordlist.json` | words.hk 粵典詞表 | 2026-03-30 | **Public Domain** | words.hk（公有領域） |
| `yue/` | `words_hk_charlist.json` | words.hk 粵典字表 | 2026 | **Public Domain** | words.hk（公有領域） |
| `yue/` | `words_hk_english_index.json` | words.hk 英粵對照表 | 2026 | **Public Domain** | words.hk（公有領域） |
| `yue/` | `cccanto.txt` | CC-Canto | 2017-02-02 (webdist) | CC BY-SA 3.0 | CC-Canto © 2015-17 Pleco Inc. |
| `sentences/` | `tatoeba-cc0.csv` | Tatoeba (CC0 子集) | daily (2026-08-31) | CC0 1.0 | Tatoeba Project |
| `sentences/` | `tatoeba-{eng,jpn,cmn,yue}.tsv` | Tatoeba per_language | daily (2026-08-31) | **CC BY 2.0 FR**（逐句署名：`#id` + 作者） | Tatoeba Project（contributors） |

## 约束（与实现一致）

- **words.hk 仅取三个 Public Domain 列表**；其完整释义/例句属 Non-Commercial Open Data
  License，**不包含**在本 fixture 中（#21–#23）；
- **KANJIDIC2 仅导入** `character/reading/meaning/stroke_count/grade/radical/kanjidic2_jlpt`；
  `dic_number/*_ref`、`skkip`、`query_code`、`misc.freq` 等第三方许可不明字段不导入（#15）；
- **Tatoeba CC BY 句逐句记录** `sentence_id + author + license + source`（#29/#30）；
- JLPT/HSK/CEFR/词频：fixture 中不含任何推断级别（#39/#40）；
- 测试（`cargo test --workspace`）不访问任何第三方网络（#78）——解析器输入即本目录文件。
