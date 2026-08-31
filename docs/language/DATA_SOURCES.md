# Language 数据集来源与许可登记（DATA SOURCES）

> 状态：**已核验（2026-09）**。本文件是任何数据集进入代码前的强制 Gate（任务 #85）：
> 许可不明确的 → **DO NOT IMPORT**。外链信息以官方页面为准，导入代码中的版本号与下载链接与本表一致。
> 每个数据集的 raw 文件已实际下载到 `data/raw/`（gitignored），Fixtures 由其子集生成（`tests/fixtures/language/`）。

## 总则

```text
官方 Dataset > 官方开放数据 > 明确开放许可的数据 > 社区 Dataset > 网页抓取
```

- 语言核心数据 V1 **不使用网页爬虫**；禁止抓剑桥/牛津/柯林斯/韦氏/Weblio/百度等受版权保护内容；
- 所有 `LanguageItem / Sentence / AudioAsset` 均能追溯 `LanguageSource`（#5）；
- 许可不只存 `license = "CC"`，必须展开为 `SourceLicense` 的四个布尔能力（#6）；
- **Commercial-Safe 规则**：非商业许可的数据默认不进入 commercial-safe pack（#76/#77）。

---

## English Core Pack

### E1 Open English WordNet（OEWN）— 主词典

| 项 | 值 |
| --- | --- |
| 角色 | Word / Sense / POS / Synonym / Semantic Relation / Definition |
| 官方主页 | <https://en-word.net/> ，GitHub: <https://github.com/globalwordnet/english-wordnet> |
| 下载位置 | <https://en-word.net/static/english-wordnet-2025-json.zip（JSON；另有> LMF XML / RDF / WNDB） |
| 版本 | **2025 Edition**（2025-12-31 发布；2025 版起专有名词移入 Open English Namenet，故本版为词项版） |
| 格式 | GWA JSON：`entries-*.json`（word→pos→sense）+ 分 POS synset 文件（definition/members/example/hypernym…） |
| 许可 | **CC BY 4.0**（Attribution 4.0 International；COPYRIGHT 见官方 LICENSE.md） |
| 商业使用 | ✅（CC BY 4.0 允许） |
| 再分发 | ✅（需署名并保留许可说明） |
| 必需署名 | ✅ "The Open English WordNet Team"，注明数据集名称与链接 |
| 导入字段 | lemma / pos / sense(id,synset) / definition / example / members(=同义词) / hypernym,hyponym,antonym,attribute,domain_topic 关系 / pronunciation(IPA,GB/US) 等 entries 内可选字段 |
| 排除字段 | 暂无（2025 版词项包结构化字段均随数据本身提供）；relation 全集以 synset 键为准动态映射 |
| 备注 | 优先 JSON（任务 #7）；Princeton WordNet 仅作 legacy 参照，不双导 |

### E2 CMUdict（CMU Pronouncing Dictionary）— 发音

| 项 | 值 |
| --- | --- |
| 角色 | English pronunciation / phoneme / stress（**ARPABET**，非 IPA） |
| 官方主页 | <http://www.speech.cs.cmu.edu/cgi-bin/cmudict> ，维护: <https://github.com/cmusphinx/cmudict> |
| 下载位置 | <https://raw.githubusercontent.com/cmusphinx/cmudict/master/cmudict.dict> |
| 版本 | master（0.7b 系；约 13 万词） |
| 格式 | `WORD ARPABET…`（如 `NATURAL N AE1 CH ER0 AH0 L`） |
| 许可 | 免费用于研究/商业；要求使用或再分发注明来源（Copyright (C) 1993-2015 Carnegie Mellon Univ.；0.7b 指出编辑部许可按 BSD 系条款） |
| 商业使用 | ✅ |
| 再分发 | ✅（保留版权/许可声明） |
| 必需署名 | ✅ "The CMU Pronouncing Dictionary (© Carnegie Mellon University)" |
| 导入字段 | word → phonemes；`_variant` 多发音；stress 由 ARPABET 数字后缀保留 |
| 排除字段 | 无（格式单一）；不做 ARPABET→IPA 转换（独立 converter 属未来，不改源数据） |
| 备注 | `pronunciation_scheme = ARPABET`（#9） |

---

## Japanese Core Pack

### J1 JMdict — 日英词典

| 项 | 值 |
| --- | --- |
| 角色 | Japanese Word / Kana / Reading / POS / Meaning / Sense / Usage |
| 官方主页 | <https://www.edrdg.org/wiki/JMdict-EDICT_Dictionary_Project.html> （EDRDG） |
| 下载位置 | <https://www.edrdg.org/pub/Nihongo/JMdict_e.gz（English> Gloss 版；另 JMdict.gz 多语言） |
| 版本 | 每日生成（下载日 2026-08-31；含 ent_seq 到 1011900+） |
| 格式 | XML（`<entry><k_ele><keb>…<r_ele><reb>…<sense><pos><gloss>`） |
| 许可 | **CC BY-SA 4.0**（EDRDG License Statement；备注：非日语成分另有编译者版权） |
| 商业使用 | ✅（CC BY-SA 4.0 允许，需署名+相同方式共享） |
| 再分发 | ✅（Share Alike） |
| 必需署名 | ✅ 注明 EDRDG / JMdict（App 内 Settings → Language Data → Sources 展示 attribution） |
| 导入字段 | ent_seq / keb(表记) / reb(读音) / sense{pos,gloss,lsource,field,misc 简表} |
| 排除字段 | 不导入 VAT 等细碎标注的完整展开；sense 仅取 gloss/pos/有限 misc，避免把许可悬空的第三方注记当事实 |
| 备注 | 使用 `JMdict_e.gz`（English Gloss），多语言释义留待需要时 |

### J2 KANJIDIC2 — 汉字

| 项 | 值 |
| --- | --- |
| 角色 | Kanji / Reading / Meaning / Stroke Count / Radical 基础元数据 |
| 官方主页 | <https://www.edrdg.org/wiki/KANJIDIC_Project.html> （EDRDG） |
| 下载位置 | <https://www.edrdg.org/kanjidic/kanjidic2.xml.gz> |
| 版本 | 每日生成；覆盖 **13,108 汉字**（JIS X 0208/0212/0213） |
| 格式 | XML（`<character><literal>…<reading_meaning><rmgroup>`） |
| 许可 | **CC BY-SA 4.0** + KANJIDIC 专项条件（EDRDG 特殊条件第 8 条；部分字段含第三方许可） |
| 商业使用 | ✅ |
| 再分发 | ✅（Share Alike + 保留版权声明） |
| 必需署名 | ✅ |
| 导入字段 | **仅许可明确且学习必需**：`character` / `reading`(on, kun) / `meaning` / `stroke_count` / `grade` / `radical`(classical) / `jlpt`（EDRDG 文档说明传承自已发布的 JLPT 级别表） |
| 排除字段 | 因许可不清晰故 **SKIP**（任务 #15）：所有 `dic_number/*_ref`（nelson/halpern/heisig/gakken/moro…）、`skkip`(SKIP 码)、`query_code` 字段、`misc.freq`（词频标注，来源独立）默认不导入，除非另行验证 |
| 备注 | `jlpt` 以 `kanjidic2_jlpt` 元数据展示并归属 KANJIDIC2 来源；不向词汇级别外推 |

---

## Mandarin Core Pack

### Z1 CC-CEDICT — 汉英词典

| 项 | 值 |
| --- | --- |
| 角色 | Simplified / Traditional / Pinyin / Meaning |
| 官方主页 | <https://www.mdbg.net/chinese/dictionary?page=cc-cedict> |
| 下载位置 | <https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz（或同名> .zip） |
| 版本 | 发行版（下载日 2026-08-31，约 12.5 万词条；文件头含 `date` 与 `license` 行） |
| 格式 | `TRAD SIMP [pinyin] /meaning/`（pinyin 用 `u:` 表示 ü，tone digit 后缀） |
| 许可 | **CC BY-SA 4.0**（文件头声明；官方 wiki 历史版本写 3.0，现行下载头为 4.0，按下载头为准） |
| 商业使用 | ✅ |
| 再分发 | ✅（Attribution + Share Alike） |
| 必需署名 | ✅ CC-CEDICT / MDBG |
| 导入字段 | 简体 / 繁体 / pinyin(含 tone) / 释义（按 `/` 分割；`CL:` 标注为 measure word 前缀保留） |
| 排除字段 | 无（解析歧义的续行 `#` 结尾视为空释义丢弃）；不补抓百度百科/汉典等释义 |
| 备注 | HSK **不在** CC-CEDICT 中 → V1 HSK = None，不发明（#20） |

---

## Cantonese Core Pack

### Y1 words.hk 公有领域数据集（粤典）

| 项 | 值 |
| --- | --- |
| 角色 | 发音（Jyutping）主源 + English→Cantonese 检索索引 |
| 官方主页 | <https://words.hk/faiman/analysis/> （粵典 words.hk） |
| 下载位置 | 词表 <https://words.hk/faiman/analysis/wordslist.json> ｜ 字表 <https://words.hk/faiman/analysis/charlist.json> ｜ 英粵對照 <https://words.hk/faiman/analysis/englishindex.json（另有> CSV） |
| 版本 | 词表最后更新 2026-03-30（62,274 词）；字表 incl. 異體字 `*` 标记 |
| 格式 | 词表: `{word: [jyutping…]}`；字表: `{char: {jyutping: count}}`；英粵: `{english: [[`word:jyutping`, score]…]}` |
| 许可 | **Public Domain（公有領域）** —— 仅限上述三个列表（页面标 "Data License: public domain. Credits to words.hk appreciated."） |
| 商业使用 | ✅ |
| 再分发 | ✅ |
| 必需署名 | 建议（欢迎 credits words.hk） |
| 导入字段 | word→jyutping（tone 由音节尾部数字解析：`sik6`→tone 6）；char→jyutping；english→word 对照用于英文搜索 |
| 排除字段 | 粤语/英文**释义与例句**（`Non-Commercial Open Data License 1.0`）→ **默认不导入**（#23） |
| 备注 | 完整词典需到 <https://words.hk/faiman/request_data/> 单独申请，非商 OFL；V1 不触碰 |

### Y2 CC-Canto — 粤语释义/拼音词典

| 项 | 值 |
| --- | --- |
| 角色 | Cantonese meaning / Jyutping 增补词典 |
| 官方主页 | <https://cantonese.org/> （同 <https://cccanto.org/> ） |
| 下载位置 | <https://cantonese.org/cccanto-170202.zip（`cccanto-webdist.txt`，约> 2.2 万条）｜ Cantonese readings: <https://cantonese.org/cccedict-canto-readings-150923.zip> |
| 版本 | 2017-02-02（webdist）；readings 2015-09-23 |
| 格式 | `TRAD SIMP [pinyin] {jyutping each word} /meaning/`（扩展 CC-CEDICT 格式，Jyutping 在额外字段） |
| 许可 | **CC BY-SA 3.0**（头部声明；Copyright (c) 2015-17 Pleco Inc.） |
| 商业使用 | ✅（CC BY-SA 3.0） |
| 再分发 | ✅（Share Alike + 署名） |
| 必需署名 | ✅ Pleco / CC-Canto |
| 导入字段 | 繁体 / 简体 / jyutping / 释义（与 CC-CEDICT 同解析）；readings 文件无释义时仅读音 |
| 排除字段 | 无（格式内全部字段均源自该许可范围） |
| 备注 | 与 words.hk PD 列表并存：发音以 words.hk 优先（PD），释义来自 CC-Canto（BY-SA） |

---

## Sentence

### S1 Tatoeba — 例句

| 项 | 值 |
| --- | --- |
| 角色 | Sentence / Translation / Sentence Relation |
| 官方主页 | <https://tatoeba.org/en/downloads> ；导出: <https://downloads.tatoeba.org/exports/> |
| 下载位置 | CC0 全集 `sentences_CC0.tar.bz2`（或 `sentences_CC0.csv`）；分语言 `per_language/{lang}/{lang}_sentences.tsv.bz2`（及 `_detailed` 带作者） |
| 版本 | 每日生成（下载日 2026-08-31） |
| 格式 | CC0: `id\tlang\ttext\tdate_modified`；per-language: `id\tlang\ttext` |
| 许可 | 文本句默认 **CC BY 2.0 FR**（需署名）；另有 **CC0 1.0** 子集 → V1 优先 `sentences_CC0` |
| 商业使用 | CC0=✅；CC BY=✅（署名） |
| 再分发 | CC0=✅；CC BY=✅（署名） |
| 必需署名 | CC BY：每句必须保存 **sentence_id + author(用户名) + license + source**（#29/#30） |
| 导入字段 | sentence_id / lang / text；CC BY 句另存 author（来自 sentences_detailed）与 license 字段 |
| 排除字段 | 不运行时调 Tatoeba API；不把全量几百万句放仓库（仅少量 fixture） |
| 备注 | 翻译是衍生内容，**逐句独立记 license**（原句 CC0 ≠ 翻译 CC0，任务 #30） |

---

## Speech Research（不打包进应用）

### V1 Mozilla Common Voice — 仅研究/本地使用

| 项 | 值 |
| --- | --- |
| 角色 | STT 研究 / 发音研究 / ASR 数据集 / 用户本地自装 Language Pack |
| 官方主页 | <https://commonvoice.mozilla.org/> |
| 许可 | 数据集 **CC0-1.0**；但 Mozilla 附加条款：不识别说话人身份、不在其他平台重新托管/重新分享 |
| 结论 | **禁止把 Common Voice 音频复制进 self-tools 仓库随应用发布**（#34）。仅设计 `CommonVoiceLocalProvider` 读取用户本地下载的 Dataset（#35） |

---

## 预选数据源版本快照（实现时锁入代码常量）

| 数据集 | 版本常量 | raw 文件（data/raw/，gitignored） |
| --- | --- | --- |
| Open English WordNet | 2025 (2025-12-31) | `oewn-2025-json.zip` |
| CMUdict | cmusphinx master (0.7b 系) | `cmudict.dict` |
| JMdict | 下载日 2026-08-31 (daily) | `jmdict_e.gz` |
| KANJIDIC2 | 下载日 2026-08-31 (daily, 13108 字) | `kanjidic2.xml.gz` |
| CC-CEDICT | 下载日 2026-08-31 (~124,935 词) | `cedict.txt.gz` |
| words.hk word list | 2026-03-30 (62,274 词) | `words_hk_wordlist.json` |
| words.hk char list | 2026 (5,875 字) | `words_hk_charlist.json` |
| words.hk English index | 2026 (40,839 词) | `words_hk_english_index.json` |
| CC-Canto | 2017-02-02 + readings 2015-09-23 | `cccanto.zip`（webdist） |
| Tatoeba | 下载日 2026-08-31 (CC0 + per_language) | `tatoeba-cc0.tar.bz2`, `tatoeba_{jpn,cmn,yue,eng}_sentences.tsv.bz2` |

> 版本/许可若变化（项目停止维护、数据格式重大变更、许可收紧）→ 重新评估并更新本表（#86）。
