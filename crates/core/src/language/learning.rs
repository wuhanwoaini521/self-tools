//! Language 学习层领域模型：统一学习对象 / Lesson / 错题 / 句子拆解。
//!
//! 设计边界（与平台的分工）：
//!
//! - **平台**（`crate::learning`）拥有跨模块的 `LearningEvent` / `LearningProgress` /
//!   `UniversalReviewCard` / `Collection` 与 SRS、掌握度算法。Language **不复制**它们。
//! - **本模块**只拥有「语言学习内容」本身：把 word / phrase / sentence / article
//!   统一成 [`LanguageLearningItem`] 供学习流程消费；用 [`Lesson`] 把若干条目组织成
//!   一次完整学习；用 [`Mistake`] 记录答错的具体上下文（平台只记 `Incorrect` 事件，
//!   不记「用户当时答成了什么」）。
//!
//! 掌握度与复习排期**一律**由平台 `MasteryCalculator` / `SpacedRepetitionScheduler`
//! 决定，本模块不另算，也不持有第二份 `interval_days` / `ease`。

use serde::{Deserialize, Serialize};

use crate::language::model::{LanguageCode, LanguageItem, LanguageItemType};

// ============================================================================
// 1. 统一学习对象
// ============================================================================

/// 学习层看到的条目类别。刻意窄于 [`LanguageItemType`]：只有这四类进入学习闭环
/// （`Dialogue` / `Grammar` / `Pronunciation` 是词典侧的辅助条目，不单独成为学习单元）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningItemType {
    Word,
    Phrase,
    Sentence,
    Article,
}

impl LearningItemType {
    /// 稳定字符串（同时用作平台 `entity_type`，跨会话可对照）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Phrase => "phrase",
            Self::Sentence => "sentence",
            Self::Article => "article",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "word" => Some(Self::Word),
            "phrase" => Some(Self::Phrase),
            "sentence" => Some(Self::Sentence),
            "article" => Some(Self::Article),
            _ => None,
        }
    }

    /// 词典条目类型 → 学习条目类型；`None` 表示该类型不进入学习闭环。
    #[must_use]
    pub const fn from_item_type(item_type: LanguageItemType) -> Option<Self> {
        match item_type {
            LanguageItemType::Word => Some(Self::Word),
            LanguageItemType::Phrase | LanguageItemType::Grammar => Some(Self::Phrase),
            LanguageItemType::Sentence | LanguageItemType::Dialogue => Some(Self::Sentence),
            LanguageItemType::Passage => Some(Self::Article),
            // 读音表是词的附属属性，不作为独立学习单元。
            LanguageItemType::Pronunciation => None,
        }
    }
}

/// 难度分级。与具体考纲（JLPT / CEFR）解耦：原始数据没有明确来源的级别一律不写，
/// 这里只记录**由使用行为推导**的难度（见 [`Difficulty::derive`]）。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    #[default]
    Unknown,
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    /// 由「历史答错次数」推导难度：确定性、可解释，且不假装知道考纲级别。
    #[must_use]
    pub const fn derive(incorrect_count: u32) -> Self {
        match incorrect_count {
            0 => Self::Unknown,
            1 => Self::Medium,
            _ => Self::Hard,
        }
    }
}

/// 学习层统一描述一个可学习对象。
///
/// 这是 **adapter 产物**：word / phrase / sentence / article 在词典侧仍各有自己的
/// domain model（各自的含义、发音、关系字段不变），学习层只要求能一致地回答
/// 「学什么、学的是哪门语言、显示什么、来源是谁」。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LanguageLearningItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: LearningItemType,
    pub language: LanguageCode,
    /// 主展示文本（lemma / 短语 / 整句 / 标题）。
    pub content: String,
    /// 翻译 / 释义。词典没有给出时为 `None`——不编造。
    pub translation: Option<String>,
    /// 读音（假名 / IPA / 拼音 / jyutping）。
    pub pronunciation: Option<String>,
    /// 罗马字 / 转写。
    pub romanization: Option<String>,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    /// 数据来源 id（可溯源到数据包与许可）。
    pub source: String,
}

impl LanguageLearningItem {
    /// 从词典条目适配为学习对象。
    ///
    /// `translation` / `pronunciation` 由调用方从各自的 domain model 填入
    /// （`LanguageItem` 本身不携带释义与发音表）。
    #[must_use]
    pub fn from_item(
        item: &LanguageItem,
        translation: Option<String>,
        pronunciation: Option<String>,
        difficulty: Difficulty,
    ) -> Option<Self> {
        let item_type = LearningItemType::from_item_type(item.item_type)?;
        Some(Self {
            id: item.id.clone(),
            item_type,
            language: item.language,
            content: item.text.clone(),
            translation,
            pronunciation: pronunciation.or_else(|| item.reading.clone()),
            romanization: item.romanization.clone(),
            difficulty,
            tags: Vec::new(),
            source: item.source.clone(),
        })
    }

    /// 平台 `LearningProgress.entity_key` 的构成：与 `learning_store` 的
    /// `format!("{module}:{entity_type}:{entity_id}")` 保持一致。
    #[must_use]
    pub fn entity_key(&self) -> String {
        format!("language:{}:{}", self.item_type.as_str(), self.id)
    }
}

// ============================================================================
// 2. Lesson
// ============================================================================

/// Lesson 中的一步：只**引用**学习对象，不复制内容。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonStep {
    pub item_id: String,
    #[serde(rename = "type")]
    pub item_type: LearningItemType,
    /// 展示用文本快照（便于列表渲染而不必联表），不作为事实来源。
    pub content: String,
    pub translation: Option<String>,
}

/// 一组学习对象组成的一次完整学习过程。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub language: LanguageCode,
    pub description: Option<String>,
    pub steps: Vec<LessonStep>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Lesson {
    #[must_use]
    pub fn step_position(&self, item_id: &str) -> Option<usize> {
        self.steps.iter().position(|step| step.item_id == item_id)
    }
}

/// 上次学到哪里。持久化后才能「退出后继续」。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonPosition {
    pub lesson_id: String,
    /// 下一步应展示的步骤下标。
    pub step_index: usize,
    pub updated_at: i64,
}

impl LessonPosition {
    /// 越界时收敛到末尾，避免读到脏数据后前端空白。
    #[must_use]
    pub fn clamped(&self, step_count: usize) -> usize {
        if step_count == 0 {
            0
        } else {
            self.step_index.min(step_count - 1)
        }
    }
}

// ============================================================================
// 3. 错题
// ============================================================================

/// 一次答错。平台只记 `LearningAction::Incorrect` 事件；这里补上「当时答成什么」。
///
/// 同一 `(item_id, card_id)` 再次答错时**累加** `error_count` 并刷新时间戳，
/// 而不是插入新行——否则重复答错会无限堆积重复记录。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Mistake {
    pub id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub item_type: LearningItemType,
    pub language: LanguageCode,
    pub content: String,
    /// 题目（复习卡 prompt）。
    pub question: String,
    pub user_answer: String,
    pub correct_answer: String,
    pub error_count: u32,
    pub last_missed_at: i64,
}

impl Mistake {
    /// 去重键：同一条目 + 同一张卡视为同一个错误。
    #[must_use]
    pub fn dedup_key(&self) -> String {
        format!("{}:{}", self.item_id, self.id)
    }
}

// ============================================================================
// 4. 句子拆解
// ============================================================================

/// 句子中的一个成分。`explanation` 只有在词典里能查到该词条时才有值。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SentenceChunk {
    pub text: String,
    /// 词典命中时携带该词的 id，前端可跳转查看。
    pub item_id: Option<String>,
    /// 词典释义（来自 `meanings`）。查不到即 `None`。
    pub meaning: Option<String>,
    pub reading: Option<String>,
}

/// 句子学习视图：原文 / 译意 / 拆解 / 语法 / 用法。
///
/// `grammar` 与 `usage` 只在有**真实来源**时才有值：词典 `Grammar` 条目或用户笔记。
/// 没有就说没有，不生成看似合理的语法说明。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SentenceStudy {
    pub id: String,
    pub language: LanguageCode,
    pub original: String,
    pub translation: Option<String>,
    pub reading: Option<String>,
    pub romanization: Option<String>,
    /// 词级切分（尽量对齐词典切分，不引入分词器猜切结果）。
    pub chunks: Vec<SentenceChunk>,
    /// 句子覆盖到的关键词（词典命中，按出现顺序去重）。
    pub key_words: Vec<String>,
    pub grammar: Option<String>,
    pub usage: Option<String>,
    pub license: Option<String>,
    pub author: Option<String>,
}

impl SentenceStudy {
    /// 构造函数：可选字段一律 `None` / 空列表。
    ///
    /// 不给 `SentenceStudy` 派生 `Default`：`language` 没有任何合理的默认值，
    /// 悄悄默认成英语会让「查不到数据」变成「返回了一条英语句子」。
    #[must_use]
    pub fn new(id: impl Into<String>, language: LanguageCode, original: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            language,
            original: original.into(),
            translation: None,
            reading: None,
            romanization: None,
            chunks: Vec::new(),
            key_words: Vec::new(),
            grammar: None,
            usage: None,
            license: None,
            author: None,
        }
    }

    /// 是否有任何可展示的拆解信息；全空时前端应显示「词典无对应词条」而非空白。
    #[must_use]
    pub fn has_breakdown(&self) -> bool {
        !self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn word() -> LanguageItem {
        LanguageItem::plain(
            LanguageCode::Jap,
            LanguageItemType::Word,
            "jmdict:1002990".into(),
            "駅".into(),
            "jmdict".into(),
        )
    }

    #[test]
    fn adapter_maps_word_and_keeps_dictionary_fields_on_domain_model() {
        let item = LanguageLearningItem::from_item(
            &word(),
            Some("车站".into()),
            Some("エキ".into()),
            Difficulty::Medium,
        )
        .expect("word 是可学习类型");

        assert_eq!(item.item_type, LearningItemType::Word);
        assert_eq!(item.content, "駅");
        assert_eq!(item.translation.as_deref(), Some("车站"));
        assert_eq!(item.entity_key(), "language:word:jmdict:1002990");
    }

    #[test]
    fn pronunciation_items_are_not_standalone_learning_units() {
        let mut item = word();
        item.item_type = LanguageItemType::Pronunciation;
        assert!(LanguageLearningItem::from_item(&item, None, None, Difficulty::Unknown).is_none());
    }

    #[test]
    fn entity_key_is_stable_across_types() {
        let sentence = LanguageItem::plain(
            LanguageCode::Jap,
            LanguageItemType::Sentence,
            "tatoeba:4812".into(),
            "私は日本語が話せない。".into(),
            "tatoeba".into(),
        );
        let adapted = LanguageLearningItem::from_item(&sentence, None, None, Difficulty::Unknown)
            .expect("sentence 可学习");
        assert_eq!(adapted.entity_key(), "language:sentence:tatoeba:4812");
    }

    #[test]
    fn lesson_position_clamps_to_valid_step() {
        let position = LessonPosition {
            lesson_id: "l1".into(),
            step_index: 99,
            updated_at: NOW,
        };
        assert_eq!(position.clamped(3), 2);
        assert_eq!(position.clamped(0), 0);
        assert_eq!(position.clamped(1), 0);
    }

    #[test]
    fn difficulty_rises_with_repeated_errors() {
        assert_eq!(Difficulty::derive(0), Difficulty::Unknown);
        assert_eq!(Difficulty::derive(1), Difficulty::Medium);
        assert_eq!(Difficulty::derive(4), Difficulty::Hard);
    }

    #[test]
    fn mistake_dedup_key_separates_different_cards_of_same_item() {
        let base = Mistake {
            id: "card_a".into(),
            item_id: "w1".into(),
            item_type: LearningItemType::Word,
            language: LanguageCode::Eng,
            content: "station".into(),
            question: "駅".into(),
            user_answer: "train".into(),
            correct_answer: "station".into(),
            error_count: 1,
            last_missed_at: NOW,
        };
        let same = Mistake {
            id: "card_a".into(),
            ..base.clone()
        };
        let other = Mistake {
            id: "card_b".into(),
            ..base.clone()
        };
        assert_eq!(base.dedup_key(), same.dedup_key());
        assert_ne!(base.dedup_key(), other.dedup_key());
    }

    #[test]
    fn sentence_study_reports_absence_of_breakdown_honestly() {
        let empty = SentenceStudy::new("s", LanguageCode::Jap, "テスト");
        assert!(!empty.has_breakdown());
        assert!(empty.translation.is_none());
        assert!(empty.grammar.is_none());
    }
}
