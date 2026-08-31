# Language Learning Hub — 架构分析（Phase 1 / 2）

> 状态：设计基线（2026-09）。以实际仓库代码为准；本文档记录「当前架构 → 插槽位置 → 领域/数据/技术决策」，
> 与 `DATA_SOURCES.md`、`tests/fixtures/language/` 配套阅读。
> 本项目真实仓库：`DevToolbox`（Rust + Tauri 2 + React 19）。

## 1. 当前架构（真实代码核对结果）

```
仓库根 = Rust workspace（resolver=3，edition 2024，rust 1.95+）
├── crates/core          # 纯领域规则：无 I/O / 无 UI / 无网络
│   ├── parser.rs / task_state.rs     # Markdown 任务行解析 + 状态注册表
│   ├── history/                      # 历史探索：模型 + 推荐（纯规则）
│   └── travel/                       # Travel：模型/攻略/规划/评分/去重/LLM 容错解析
├── crates/application  # 用例编排：文档工作流 + RSS 工作流 + Travel 研究服务 + History 服务
│   └── 各模块提供 DTO（serde Serialize 直接给 React）
├── crates/infrastructure # 适配器：文件 IO / settings.json / SQLite / feed-rs / reqwest /
│   └── travel/{search,fetcher,llm,data_provider,store}   # 每个模块自带独立 SQLite 文件
├── apps/desktop        # Tauri 2 命令适配层
│   ├── src/lib.rs      # AppState{store(FeedRepository), travel_store, history_store, client} + 命令
│   └── ui/             # React 19：外壳(App.tsx 导航注册表) + features/{home,markdown,rss,travel,history}
├── tests/fixtures/     # 跨实现共享 fixture（task_rules.json）
└── docs/               # migration/（PySide6→Rust 档案）、travel/v1-design.md
```

已有 feature 核对：**Home**（个人总览：最近笔记/最新 RSS/快捷入口）、**Markdown**（任务行解析 +
CodeMirror 装饰）、**RSS**（feed-rs + SQLite 持久化 + 定时刷新）、**Travel**（搜索→抓取→LLM→多源验证→
攻略缓存，LLM 未配置自动降级，绝不编造）、**History**（离线优先 SQLite + 推荐 + 收藏/浏览记录）。

基础设施事实（决定技术选型）：

- SQLite：`rusqlite 0.32 (bundled)`，**bundled 构建自带 `SQLITE_ENABLE_FTS5`、`SQLITE_ENABLE_JSON1`**（已核对
  libsqlite3-sys build.rs）→ 可直接用 FTS5，不必引入新数据库；
- 每模块独立 `.db`（`dashboard.db` / `travel.db` / `history.db`），`Connection` 非 `Sync`，由上层 `Mutex` 串行；
- HTTP 客户端：`reqwest`（rustls），`AppState.client` 共享；
- LLM：`LlmProvider` trait（OpenAI-Compatible），`is_configured()` 把关，未配置降级——**这是 Language 可复用的关键模式**；
- 设置：`settings.json`（`AppSettings.schema_version` 版本化，`#[serde(default)]` 向前兼容）；
- 命令错误：`CommandError { code, message }` 统一映射；前端 `errorMessage()` 消费；
- 前端：页面全部常驻挂载（`page-pane/` 显示隐藏），导航注册表一行加一个模块；主题 = CSS Design Tokens
  （`:root[data-theme=...]`），组件零主题分支；
- 质量门禁（CI 三平台）：`cargo fmt --check` / `cargo check` / `clippy -D warnings` / `cargo test`。

## 2. Language 应该放在哪

沿用四层插槽，**不新建技术栈、不重构**：

```
React  features/language/                 （新目录）
   ↓ invoke
Tauri 命令  apps/desktop/src/lib.rs  language_* 命令 + AppState.language_store
   ↓
crates/application/src/language/service.rs + 语言数据导入工作流（含 CLI 复用同一逻辑）
   ↓
crates/core/src/language/                 纯领域：模型/许可证/复习调度/发音转换/口语评分
crates/infrastructure/src/language/       store(language.db) + import/{oewn,cmudict,jmdict,
                                          kanjidic2,cedict,words_hk,cc_canto,tatoeba}
```

新文件：`crates/{core,application,infrastructure}/src/language/`、`apps/desktop/ui/src/features/language/`、
`docs/language/`、`tests/fixtures/language/`。修改文件：`Cargo.toml`（workspace 依赖）、三个 crate 的 `mod.rs`
`lib.rs`、`apps/desktop/src/lib.rs`、`apps/desktop/ui/src/{App.tsx,types.ts}`、`.gitignore`。

## 3. 可复用的现有能力

| 现有能力 | 出处 | Language 复用方式 |
| --- | --- | --- |
| SQLite + `Mutex` 串行 + 独立 db 文件 | rss_store / travel/store / history/store | `language.db`，同款 `open/ensure_schema` |
| `Connection` 两段式（无锁网络 → 短锁落库） | rss_workflows | 导入工作流：解析在无锁阶段，落库短锁 |
| `LlmProvider` trait + `is_configured` 降级 | infrastructure/travel/llm.rs | `LlmEnhancement` 可选增强（生成例句/阅读/纠错），未配置降级本地能力 |
| LLM JSON 容错解析 `extract_json` | core/travel/llm_parse.rs | AI 生成内容解析复用 |
| serde DTO + `CommandError` 映射 | application / desktop lib.rs | 全部 `language_*` 命令同款直传 |
| `#[serde(default)]` 版本化设置 | settings_store | `AppSettings.language`（默认全关，旧配置兼容） |
| 前端导航注册表 + 常驻页面 | App.tsx NAV_ITEMS | 注册 `Language`（放 History 之后） |
| Design Tokens 主题 | styles.css | Language UI 只消费 token，不做主题分支 |
| 测试体系（`#[cfg(test)]` + tempdir + fixtures） | 各 crate | parser/store/service 测试；fixtures 有 attribution |
| `now_unix()` / InfrastructureError | infrastructure | 时间戳与错误路径 |

## 4. Domain Model（core）

```
LanguageItem            —— 统一对象（WORD/PHRASE/SENTENCE/DIALOGUE/PASSAGE/GRAMMAR/PRONUNCIATION）
│                        禁止 EnglishWord/JapaneseWord... 四套对象
├── LanguageMetadata    —— enum：English/Japanese/Mandarin/Cantonese 元数据
│    EN: arpabet, phonemes, stress, cefr?
│    JA: kana, romaji, kanji, jlpt?
│    ZH: simplified, traditional, pinyin, tones, hsk?
│    YUE: traditional, simplified, jyutping, tones
├── Pronunciation      —— scheme=ARPABET|IPA|PINYIN|JYUTPING|KANA + phonemes + tone + variant
├── Meaning             —— pos, definition, gloss, sense_id, rank, lang（OEWN/JMdict/CC-CEDICT/CC-Canto）
├── LanguageRelation    —— SYNONYM/ANTONYM/FORM_OF/USED_IN/TRANSLATION_OF/RELATED_TO/BELONGS_TO_TOPIC
│                         + OEWN 的 hypernym/hyponym/attribute/domain（映射收纳）
├── SourceLicense       —— PublicDomain/CC0/CCBY/CCBYSA/CCBYNC/Custom/Unknown
│                         + attribution_required/commercial_use_allowed/redistribution_allowed/share_alike_required
├── LanguageSource      —— id/name/homepage/download_source/dataset_version/downloaded_at/license/
│                         license_url/attribution/commercial_use/redistribution/notes
├── DatasetManifest     —— name/language/version/downloaded_at/source_id/checksum/raw_file/
│                         record_count/importer_version/imported_at
├── AudioAsset          —— item_id/language/text/voice/provider/audio_type(RECORDED|TTS|USER_RECORDING)/
│                         local_path/remote_source/generated_at/source_license
├── UserLearningState   —— NEW/LEARNING/REVIEW/MASTERED（与词典数据分离，独立表）
└── ReviewLog / ReviewScheduler —— 简单 SRS（SM-2 变体），接口预留 FSRS
```

学习数据（state/favorite/review log/knowledge level）与词典数据（item/meaning/pronunciation）**用独立表隔离**，
字典更新只替换词典表，不触碰用户进度（对应 History 的「种子幂等不覆盖用户数据」先例）。

## 5. Database Schema（language.db）

见 `crates/infrastructure/src/language/store.rs`。要点：

- `languages` / `language_items` / `meanings` / `pronunciations` / `examples` / `relations` /
  `topics` + `item_topics` / `sources` / `dataset_manifests` / `audio_assets` /
  `learning_states` / `review_logs` / `favorites` / `learning_sessions`；
- 主键用**稳定内容 id**（如 `jmdict:1002990`、`wn:reservation%1:10:00::`、`cedict:旅行`、`whk:食飯`、
  `tatoeba:5080`）→ 天然去重、可增量更新；
- FTS5 虚拟表 `item_fts(search_key)`（unicode61）覆盖 text/reading/romanization/pinyin/jyutping/meaning；
- 元数据 `meta_json TEXT`（serde_json 存 LanguageMetadata 枚举）；来源 `source TEXT` 引用 sources.id；
- 每张词典表都带 `source_id` → 可追溯（任务 #5）。

## 6. Dataset Import Strategy

```
Raw（data/raw/，gitignored）→ Checksum → Parser(纯函数) → Normalizer → Validator → 去重 → SQLite
```

- 每个数据集一个 `*Importer`（`LanguageDatasetImporter` trait：`source()/version()/parse()/normalize()/
  validate()/import()`），解析器为**纯函数**（bytes → 中间结构），可在无网络单测；
- 导入不发生在 App 启动：`language-data` CLI（application crate bin）+ App 内「安装数据包」按钮共用同一工作流；
- **内置 Starter Pack = tests/fixtures/language 全量**（`include_str!` 嵌入），经真实 importer 走完整管线落库
  —— 首次使用离线可装，同时是 CI 的 fixture；完整数据由用户按 DATA_SOURCES.md 下载后 CLI 导入；
- 更新：新建临时 DB → 导入 → 校验 → 成功才原子切换（失败保留旧库）。

## 7. Audio / TTS / STT Strategy

- **TTS**：`TtsProvider` 抽象；V1 实际 Provider = Web Speech API（`speechSynthesis`，前端实现，零依赖，
  离线可用取决于 OS voice）；骨架保留 Local TTS / Cloud TTS 插槽。音频缓存今后落 `audio_assets` 表；
- **不把 Tatoeba Audio 作 V1 核心音频源**（许可混杂）；Common Voice 仅 `CommonVoiceLocalProvider`
  （用户自装，绝不重新托管/随包分发）；
- **STT**：`SpeechRecognitionProvider` 抽象；V1 提供「用户输入转写」Provider（离线可用、无第三方）+ 前端
  Web Speech Recognition 自动填充（若可用）。评分全部在 Rust core 纯函数完成：
  Accuracy/Completeness/Fluency ← Missing/Wrong/Extra Word + Duration + Long Pause（不做口音分）。

## 8. Learning / Review Strategy

- `UserLearningState` + `ReviewScheduler`（SM-2 变体：rating 0-3、interval/ease、due_at）独立于词典；
- `Today` = due review + 新词配额 + 句子 + 听/说任务，全部离线可算；统计入 `learning_sessions`；
- 学习目标可配置（每日新词、每日复习上限），存储于 AppSettings.language。

## 9. Frontend Information Architecture

```
Language（导航项，常驻页面）
├── Today      今日学习（n 新词 / n 复习 / n 句子 / 听力 / 口语 / Daily Expression）[Start Learning]
├── Explore    搜索（text/reading/romanization/meaning，全部离线）
├── Review     简单 SRS 卡片流（词卡 → 答案 → 评分 0-3）
├── Listen     句子听力（播放/显示答案/标记）
├── Speak      口语（目标句 → TTS → 录音/输入转写 → 评分反馈）
└── Library    收藏 / 学习状态 / 进度统计
+ Word Detail  （Word/Reading/Pronunciation/Meaning/POS/Examples/Related/Audio/Favorite/State/Source）
+ Settings → Language Data（Sources 表：版本/许可/导入条目数；安装数据包）
```

页面围绕学习目标组织（非数据库类型）；组件复用 WordDetail 于 Explore/Review/Library；未安装数据包时
显示引导（安装 Starter Pack），断网可用（#69 离线优先）。

## 10. 新增文件

请见最终报告 §新增文件（实现后以 `git status` 为准），类型上分：

- `docs/language/{architecture.md, DATA_SOURCES.md}`
- `crates/core/src/language/{mod.rs, model.rs, metadata.rs, license.rs, review.rs, romaji.rs, speaking.rs}`
- `crates/infrastructure/src/language/{mod.rs, store.rs, import/{mod,oewn,cmudict,jmdict,kanjidic2,cedict,words_hk,cc_canto,tatoeba}.rs}`（含 embedded starter pack）
- `crates/application/src/language/{mod.rs, service.rs, dto.rs}` + `src/bin/language_data.rs`（CLI）
- `apps/desktop/src/lib.rs`（language_* 命令）
- `apps/desktop/ui/src/features/language/*`（页面组件）+ `types.ts` 类型
- `tests/fixtures/language/**`（真实数据子集，attribution）

## 11. 修改文件

- `Cargo.toml`（workspace deps：+quick-xml/flate2/zip）
- `crates/{core,application,infrastructure}/src/lib.rs`（挂 language 模块）
- `crates/application/Cargo.toml` + `crates/infrastructure/Cargo.toml`（新依赖）
- `apps/desktop/src/lib.rs`（AppState + 命令注册）
- `apps/desktop/ui/src/App.tsx`（导航项 + 页面挂载 + Settings 传参）
- `apps/desktop/ui/src/types.ts`（DTO 类型）
- `apps/desktop/ui/src/SettingsDialog.tsx`（Language Data 面板入口）
- `.gitignore`（data/raw、data/generated）

## 关键取舍（避免失控）

- V1 完整闭环 = **English + Japanese**；Mandarin + Cantonese = 统一架构下的 Importer/搜索/词详情验证；
- JLPT/HSK/CEFR 全部 Optional：只有带明确来源与许可的数据才入库（KANJIDIC2 的 jlpt 字段有 EDRDG 文档来源，
  标记为 `kanjidic2_jlpt` 展示；不发明级别）；
- 词频 V1 不导入；AI 仅增强（例句/阅读/纠错），不负责任何基础事实；
- 不引 Graph DB / Vector DB / RAG / 游戏化；不把 Common Voice 音频或大语料提交进 Git。
