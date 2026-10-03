# NCE 导入与学习数据说明

> 适用范围：`Language → English` 的新概念英语主线。
> 教材音频与课文属**用户本地学习资料**，不随仓库分发（`config/` 已在 `.gitignore`）。

## 1. 一次导入，两处数据源

| 数据 | 来源 | 用途 | 缺了会怎样 |
| --- | --- | --- | --- |
| 教材（课文 / 中文 / 时间轴 / 音频） | 本地 NCE 文件夹 | 课程主体 | English 首页显示「从新概念英语开始」引导 |
| 词典（音标 / 中文释义 / 词频 / 词形） | 本地 ECDICT csv | 生词卡、查词 | 生词只有英文，无音标与释义；学习流程不受影响 |

两者都在 **English → 导入弹窗**里选择本地路径完成，不依赖任何远程服务器。

## 2. 支持的 NCE 目录布局

导入器（`crates/infrastructure/src/language/nce.rs`）按以下顺序探测，三种都支持：

```text
① book.json（iChochy/NCE 风格）
   NCE1/
     book.json              {"units":[{"title":"001&002.Excuse Me","filename":"001&002.Excuse Me"}]}
     001&002.Excuse Me.lrc
     001&002.Excuse Me.mp3

② 根 data.json（NCE-Flow 风格）
   data.json                {"1":[{"title":"...","filename":"..."}],"2":[...]}
   static/data.json         同上（NCE-Flow 原仓库位置）
   NCE1/*.lrc  NCE1/*.mp3

③ 裸文件（无任何清单）
   NCE1/001&002－Excuse Me.lrc
   NCE1/001&002－Excuse Me.mp3
```

- 册目录名匹配 `NCE1`…`NCE4`（大小写 / 空格宽容）；只有一册时，根目录本身也可作为册目录。
- 课时编号与标题从文件名解析：`001&002－Excuse Me` → `(1, "Excuse Me")`；
  `17－Always Young` → `(17, "Always Young")`。

## 3. LRC 解析规则

`crates/core/src/language/lrc.rs`（纯函数，有单测）：

- 时间戳 `[mm:ss.xx]` / `[mm:ss.xxx]`；
- 元数据标签（`[ti:]` `[ar:]` `[al:]` `[by:]` `[offset:]` …）跳过；
- 同行多时间戳 `[t1][t2]text` 展开成多条；
- `英文|中文` 取竖线右侧为译文；无竖线时译文为空（**不编造**）；
- `end_ms = 下一句 start_ms`，最后一句按词数估算；
- 编码：UTF-8 优先，失败回退 **GBK**（中文 Windows 工具生成的字幕常见）；
- 畸形行跳过并记入 `issues`，**不会让一整册导入失败**。

## 4. 导入行为

- **幂等**：id 内容派生（`nce:{book}:{lesson}`、`nce:{book}:{lesson}#{seq}`），
  重复导入 = 覆盖教材内容，**用户进度不受影响**（进度在 `language_lesson_progress`）。
- **媒体落地**：`.mp3` 拷贝到 `config/language/nce/NCE{book}/`，同名同大小即跳过。
  导入后可离线使用，不依赖源文件夹。
- **可取消**：每课之间检查 cancel flag；进度通过 `language-nce-progress` 事件推送。
- **容错**：缺 MP3 → 记录 issue，听力/跟读禁用，其余功能正常；缺 LRC → 跳过该课。
- **错误可见**：所有 issue 在导入报告里可展开查看，不静默吞掉。

## 5. 生词从哪来（不是预置词单）

导入每课时，从**该课真实句子**提取：

1. 切词（保守：字母 + 词内撇号）→ 过滤功能词（`crates/core/src/language/course.rs::is_stopword`）；
2. 用 ECDICT 的 `exchange` 词形表还原 lemma（`hesitated` → `hesitate`），不自造词形规则；
3. 同一课内按 lemma 合并，释义/音标取 **lemma 自己的词条**；
4. 按词频排名推导 `importance`（越稀有越值得学），课前预习按它排序。

词典未导入时仍会收录（只有英文单词），导入词典后**重导入**即可补齐释义。

## 6. ECDICT 导入

- 文件：`ecdict.csv`（ECDICT 官方发行版，约 77 万词条，含中文释义、音标、词频、
  Collins 星级、`exchange` 词形）。
- 流式解析 + 每 2 万行一个事务；实测 debug 构建约 9 秒完成全量导入。
- 字段里字面量 `\n` 会在导入时归一化成真换行（前端按行取第一条释义）。
- 全量重导入 = 清空重建（词典没有用户状态，安全）。
- 可取消，进度按批推送 `language-dict-progress` 事件。

## 7. 数据落在哪

```text
config/language.db
  ├─ language_courses / language_books / language_lessons / language_sentences
  ├─ language_lesson_vocab        ← 教材派生物（可重导入）
  ├─ language_lesson_progress      ← 用户状态（不随重导入清空）
  ├─ language_word_occurrences     ← 「这个词在哪见过」
  ├─ language_plan                ← 每日目标 / 当前册
  └─ dict_entries                 ← ECDICT
config/language/nce/NCE{1..4}/*.mp3   ← 音频（gitignored）
config/learning.db
  └─ review_cards / learning_progress   ← 平台 SRS（所有模块共用）
```

单词的复习排期**只有一份**，在平台 `learning.db`；语言库不持有第二份 SRS。

## 8. 离线能力

导入完成后，以下全部离线可用：课程列表、课文、中文、时间轴、音频、词典查词、SRS、
学习记录与统计。仅 AI 讲解需要网络；未配置 AI 时 Lesson 全部功能照常可用。

## 9. 不做的事

- 不下载教材（版权与体积都属用户本地资料，应用只读取用户选定的目录）。
- 不做语音识别打分（当前没有可靠能力，不制造假分数）；跟读提供播放 / 录音 / 回放。
- 不把教材内容写进 Git。