//! 英语课程学习子域（NCE 主课程）纯领域模型。
//!
//! 设计约束（与任务书对齐）：
//! - 课程 / 书本 / 课时 / 逐句时间轴是**语言无关**结构（未来日语/中文可复用同一张表）；
//! - NCE 只是 English 的一个 Course Provider（`source_type = "nce"`），不是整个 Language 域；
//! - 学习状态（`LessonProgress`）与教材内容分离，教材可重导入而进度不丢；
//! - id 全部为**内容派生的稳定 id**（`nce:2:17` / `nce:2:17#3`），导入幂等，天然去重；
//! - 单词学习状态（SRS / 掌握度）不在本文件——那是平台 `learning` 子域的能力。

use serde::{Deserialize, Serialize};

use crate::language::LanguageCode;

// ============================================================================
// 1. 课程结构
// ============================================================================

/// 一套课程（如 New Concept English）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Course {
    pub id: String,
    pub language: LanguageCode,
    /// 短代码（如 `nce`）。
    pub code: String,
    pub title: String,
    pub description: Option<String>,
    /// 数据来源类型（`nce` / 未来的其它 Provider）。
    pub source_type: String,
    pub created_at: i64,
}

/// 课程下的一册书（NCE1–4）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CourseBook {
    pub id: String,
    pub course_id: String,
    pub book_no: u32,
    pub title: String,
    pub subtitle: Option<String>,
    pub total_lessons: u32,
}

/// 一课（对应 NCE 的一对课文，如 Lesson 17 `Always Young`）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CourseLesson {
    pub id: String,
    pub book_id: String,
    pub lesson_no: u32,
    pub title: String,
    /// 本地音频路径（导入时拷贝进用户数据目录后的**绝对路径**）。
    pub audio_path: Option<String>,
    pub duration_ms: Option<i64>,
    /// 句子数（导入时统计，列表页不必联表）。
    pub sentence_count: u32,
    /// 生词数（导入时统计）。
    pub vocab_count: u32,
}

/// 单词在本课的用户自评（课前预习三态；持久化，重进课程可见）。
pub type VocabMark = Option<String>;

/// 列表页条目：课时 + 用户状态（无进度 = NotStarted）。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LessonListEntry {
    #[serde(flatten)]
    pub lesson: CourseLesson,
    pub status: LessonStatus,
    pub percent: u32,
}

/// 书本汇总（Book 页头部）。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BookSummary {
    pub total_lessons: u32,
    pub completed_lessons: u32,
    pub learning_lessons: u32,
    pub study_seconds: i64,
    pub vocab_total: u32,
}

/// 课时状态（由 `LessonProgress` 推导，列表页展示用）。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    #[default]
    NotStarted,
    Learning,
    Completed,
    /// 已完成且到期复习。
    Review,
}

impl LessonStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Learning => "learning",
            Self::Completed => "completed",
            Self::Review => "review",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "learning" => Self::Learning,
            "completed" => Self::Completed,
            "review" => Self::Review,
            _ => Self::NotStarted,
        }
    }
}

/// 逐句时间轴记录（LRC 解析结果落库）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonSentence {
    pub id: String,
    pub lesson_id: String,
    pub sequence: u32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub english: String,
    pub chinese: Option<String>,
}

/// 课时生词（从课文真实提取 + 词典富化；不是手工词表）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonVocab {
    pub lesson_id: String,
    /// lemma（小写原形，如 `hesitate`）。
    pub word: String,
    /// 课文中实际出现的形态（如 `hesitated`），可空。
    pub surface: Option<String>,
    /// 首次出现的句子 id。
    pub sentence_id: Option<String>,
    /// 课文中该词的例句（英文原句快照）。
    pub context: Option<String>,
    pub phonetic: Option<String>,
    pub pos: Option<String>,
    pub translation_zh: Option<String>,
    pub definition_en: Option<String>,
    /// 词频排名（ECDICT frq；越小越常用；0 = 无数据）。
    pub frequency: u32,
    /// 考试/难度标签（cet4/cet6/ky/toefl/ielts/gre…）。
    pub tags: Vec<String>,
    /// 0–100：稀有度越高越值得学（由词频推导）。
    pub importance: u32,
    /// 用户在本课的自评：`Some("know" | "fuzzy" | "unknown")`，未标过为 `None`。
    ///
    /// **必须持久化**：课前预习靠它把「认识」的词移出队列，
    /// 重进课程时也要能看到上次的判断，而不是重新来一遍。
    #[serde(default)]
    pub mark: Option<String>,
}

// ============================================================================
// 2. 学习进度
// ============================================================================

/// Lesson 学习阶段（单页工作台的阶段顺序）。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonStage {
    #[default]
    Vocabulary,
    Listen,
    Read,
    Sentence,
    Shadow,
    Quiz,
    Done,
}

impl LessonStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vocabulary => "vocabulary",
            Self::Listen => "listen",
            Self::Read => "read",
            Self::Sentence => "sentence",
            Self::Shadow => "shadow",
            Self::Quiz => "quiz",
            Self::Done => "done",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "listen" => Self::Listen,
            "read" => Self::Read,
            "sentence" => Self::Sentence,
            "shadow" => Self::Shadow,
            "quiz" => Self::Quiz,
            "done" => Self::Done,
            _ => Self::Vocabulary,
        }
    }

    /// 阶段顺序（进度百分比用）。
    #[must_use]
    pub const fn order(self) -> u32 {
        match self {
            Self::Vocabulary => 0,
            Self::Listen => 1,
            Self::Read => 2,
            Self::Sentence => 3,
            Self::Shadow => 4,
            Self::Quiz => 5,
            Self::Done => 6,
        }
    }
}

/// 一课的学习进度（断点续学的全部状态）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonProgress {
    pub lesson_id: String,
    pub stage: LessonStage,
    /// 音频播放位置（毫秒）。
    pub position_ms: i64,
    /// 逐句模式当前句。
    pub sentence_seq: u32,
    /// 课前单词已完成到第几个。
    pub vocab_index: u32,
    /// 跟读完成到第几句。
    pub shadow_seq: u32,
    pub quiz_score: Option<u32>,
    pub completed_at: Option<i64>,
    /// 累计学习秒数（粗略：每次心跳累加）。
    pub study_seconds: i64,
    pub updated_at: i64,
}

impl LessonProgress {
    #[must_use]
    pub fn new(lesson_id: &str, now: i64) -> Self {
        Self {
            lesson_id: lesson_id.to_string(),
            stage: LessonStage::Vocabulary,
            position_ms: 0,
            sentence_seq: 0,
            vocab_index: 0,
            shadow_seq: 0,
            quiz_score: None,
            completed_at: None,
            study_seconds: 0,
            updated_at: now,
        }
    }

    #[must_use]
    pub fn status(&self) -> LessonStatus {
        if self.completed_at.is_some() {
            LessonStatus::Completed
        } else if self.study_seconds > 0
            || self.stage != LessonStage::Vocabulary
            || self.position_ms > 0
            || self.vocab_index > 0
        {
            LessonStatus::Learning
        } else {
            LessonStatus::NotStarted
        }
    }

    /// 0–100 的阶段进度（Continue 卡片用）。
    #[must_use]
    pub fn percent(&self) -> u32 {
        (self.stage.order() * 100 / LessonStage::Done.order()).min(100)
    }
}

// ============================================================================
// 3. 学习计划
// ============================================================================

/// 用户英语学习计划（首页 Today 的输入）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LearningPlan {
    pub language: LanguageCode,
    /// 当前主课程（如 `nce`）。
    pub course_id: Option<String>,
    /// 当前书（如 `nce:2`）。
    pub book_id: Option<String>,
    /// 每日目标分钟数。
    pub daily_minutes: u32,
    /// 每日新词数。
    pub new_words_per_day: u32,
    /// 用户导入教材时选择的**源目录**（只用于界面显示「你的数据在哪」）。
    pub nce_source_dir: Option<String>,
    pub updated_at: i64,
}

impl Default for LearningPlan {
    fn default() -> Self {
        Self {
            language: LanguageCode::Eng,
            course_id: None,
            book_id: None,
            daily_minutes: 30,
            new_words_per_day: 10,
            nce_source_dir: None,
            updated_at: 0,
        }
    }
}

// ============================================================================
// 4. 词典（ECDICT 统一词条）
// ============================================================================

/// 词典查询结果（DictionaryService.lookup 的统一返回形状）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WordEntry {
    /// 查询命中的词形（小写）。
    pub word: String,
    /// 原形（经 exchange 词形还原；无词形数据时 = word）。
    pub lemma: String,
    pub phonetic: Option<String>,
    pub pos: Option<String>,
    /// 中文释义（可含多行，`\n` 分隔）。
    pub translation_zh: Option<String>,
    /// 英文释义。
    pub definition_en: Option<String>,
    /// 词频排名（frq；0 = 无数据）。
    pub frequency: u32,
    /// bnc 排名。
    pub bnc: u32,
    /// 标签（cet4/cet6/ky/toefl/ielts/gre…）。
    pub tags: Vec<String>,
    /// Collins 星级（0–5）。
    pub collins: u32,
    /// 词形变化（`p=过去式 d=过去分词 i=现在分词 3=三单 s=复数 r=比较级 t=最高级`）。
    pub forms: Vec<(String, String)>,
}

impl WordEntry {
    /// 词条是否「有内容」（查得到但全空视为未命中，避免误导 UI）。
    #[must_use]
    pub fn is_meaningful(&self) -> bool {
        self.translation_zh.is_some() || self.definition_en.is_some() || self.phonetic.is_some()
    }
}

/// 单词自评（课前预习三态；不是二元 Known/Unknown）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WordMark {
    Know,
    Fuzzy,
    Unknown,
}

impl WordMark {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Know => "know",
            Self::Fuzzy => "fuzzy",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "know" => Some(Self::Know),
            "fuzzy" => Some(Self::Fuzzy),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// 一次单词遇见记录（「这个词你在哪里见过」——self-tools 特色能力）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WordOccurrence {
    pub word: String,
    /// `lesson` / `reading` / `quiz` / `lookup`…
    pub source_type: String,
    /// 来源 id（如 `nce:2:17`）。
    pub source_id: String,
    /// 出现时的句子快照。
    pub sentence: Option<String>,
    pub occurred_at: i64,
}

// ============================================================================
// 5. Quiz
// ============================================================================

/// 一道 quiz 题（全部从本课真实数据生成）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuizItem {
    /// 词汇：给英文选中文。
    Vocabulary {
        word: String,
        phonetic: Option<String>,
        /// 4 个中文选项（含正确答案）。
        options: Vec<String>,
        /// 正确选项下标。
        answer: usize,
    },
    /// 填空：课文原句挖掉一个本课生词。
    FillBlank {
        /// 含 `_____` 的句子。
        sentence: String,
        chinese: Option<String>,
        answer: String,
    },
    /// 听写：播放句子音频区间，输入听到的内容。
    Dictation {
        lesson_id: String,
        sentence_seq: u32,
        start_ms: i64,
        end_ms: i64,
        answer: String,
    },
    /// 翻译：中文 → 英文（自评 / AI 辅助，不做严格字符串比较）。
    Translate { chinese: String, reference: String },
}

/// 用户对一题的作答结果。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuizAnswer {
    pub item_index: u32,
    pub correct: bool,
    /// 用户实际答案快照（错题本用）。
    pub user_answer: Option<String>,
}

/// Quiz 结果。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuizResult {
    pub lesson_id: String,
    pub total: u32,
    pub correct: u32,
    /// 0–100。
    pub score: u32,
    pub wrong_words: Vec<String>,
    pub finished_at: i64,
}

// ============================================================================
// 6. 英语文本工具（纯函数）
// ============================================================================

/// 从英文句子切出候选单词（小写、去标点、保留 `'` 内部连字符）。
///
/// 只做保守切分（字母 + 词内 apostrophe），不猜词边界——lematize 交给词典
/// exchange 表，而不是自造词形规则。
#[must_use]
pub fn tokenize_english(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() || (ch == '\'' && !current.is_empty()) {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            let trimmed = current.trim_matches('\'');
            if trimmed.len() >= 2 && trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
                words.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim_matches('\'');
    if trimmed.len() >= 2 && trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
        words.push(trimmed.to_string());
    }
    words
}

/// 英语最高频功能词（课前词表过滤用；词表应聚焦**值得学**的内容词）。
///
/// 只收没有争议的超功能词；宁可漏滤（词表多一个词）也不误滤（漏掉学习内容）。
#[must_use]
pub fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        "the" | "a" | "an" | "and" | "or" | "but" | "if" | "of" | "at" | "by" | "for"
            | "with" | "about" | "into" | "through" | "during" | "before" | "after" | "to"
            | "from" | "up" | "down" | "in" | "out" | "on" | "off" | "over" | "under"
            | "again" | "then" | "once" | "here" | "there" | "when" | "where" | "why"
            | "how" | "all" | "any" | "both" | "each" | "few" | "more" | "most" | "other"
            | "some" | "such" | "no" | "nor" | "not" | "only" | "own" | "same" | "so"
            | "than" | "too" | "very" | "can" | "will" | "just" | "should" | "now" | "is"
            | "are" | "was" | "were" | "be" | "been" | "being" | "have" | "has" | "had"
            | "having" | "do" | "does" | "did" | "doing" | "would" | "could" | "shall"
            | "may" | "might" | "must" | "ought" | "i" | "me" | "my" | "we" | "our"
            | "you" | "your" | "he" | "him" | "his" | "she" | "her" | "it" | "its"
            | "they" | "them" | "their" | "this" | "that" | "these" | "those" | "am"
            | "as" | "us" | "let" | "oh" | "yes" | "well" | "what" | "which" | "who"
            | "whom" | "whose" | "mr" | "mrs" | "miss" | "ms" | "dr" | "st" | "etc"
            // 常见缩写 contraction 整体（tokenizer 保留词内 apostrophe）。
            | "don't" | "didn't" | "doesn't" | "isn't" | "aren't" | "wasn't" | "weren't"
            | "won't" | "wouldn't" | "couldn't" | "shouldn't" | "hasn't" | "haven't"
            | "hadn't" | "mustn't" | "needn't" | "shan't" | "ain't" | "i'm" | "i've"
            | "i'll" | "i'd" | "you're" | "you've" | "you'll" | "you'd" | "he's"
            | "he'll" | "she's" | "she'll" | "it's" | "it'll" | "we're" | "we've"
            | "we'll" | "we'd" | "they're" | "they've" | "they'll" | "they'd"
            | "that's" | "there's" | "here's" | "what's" | "who's" | "let's"
            | "cannot" | "gonna" | "wanna"
    )
}

/// 由词频排名推导学习价值（0–100）：越稀有越值得学。
#[must_use]
pub fn importance_from_frequency(frq: u32) -> u32 {
    match frq {
        0 => 40, // 无词频数据：中等价值
        1..=500 => 10,
        501..=1500 => 25,
        1501..=3000 => 45,
        3001..=6000 => 65,
        6001..=12000 => 80,
        _ => 90,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_keeps_inner_apostrophes_and_lowercases() {
        assert_eq!(
            tokenize_english("Don't hesitate to ask questions."),
            vec!["don't", "hesitate", "to", "ask", "questions"]
        );
        assert_eq!(
            tokenize_english("Mrs. Smith's kitchen."),
            vec!["mrs", "smith's", "kitchen"]
        );
    }

    #[test]
    fn tokenize_skips_single_letters_and_digits() {
        // 单字母与数字被丢弃；正常单词保留。
        assert_eq!(tokenize_english("Lesson 17: A B C"), vec!["lesson"]);
    }

    #[test]
    fn stopword_list_filters_function_words_only() {
        assert!(is_stopword("the"));
        assert!(is_stopword("don't"));
        assert!(is_stopword("it's"));
        assert!(!is_stopword("hesitate"));
        assert!(!is_stopword("conversation"));
    }

    #[test]
    fn lesson_progress_status_flow() {
        let mut progress = LessonProgress::new("nce:2:17", 100);
        assert_eq!(progress.status(), LessonStatus::NotStarted);
        progress.position_ms = 5000;
        assert_eq!(progress.status(), LessonStatus::Learning);
        progress.completed_at = Some(200);
        assert_eq!(progress.status(), LessonStatus::Completed);
        assert_eq!(LessonStage::Done.order(), 6);
        assert_eq!(LessonProgress::new("x", 0).percent(), 0);
    }

    #[test]
    fn importance_decreases_with_frequency() {
        assert!(importance_from_frequency(100) < importance_from_frequency(5000));
        assert!(importance_from_frequency(5000) < importance_from_frequency(50000));
    }
}
