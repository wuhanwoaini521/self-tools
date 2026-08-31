//! Language Learning Hub 的纯领域模型。
//!
//! 设计约束（任务书核心原则）：
//! - 统一 `LanguageItem`，禁止为每种语言建独立对象（#51）；
//! - 学习数据（state/favorite/review）与词典数据分离（#59）；
//! - 级别（CEFR/JLPT/HSK）与词频默认 `None`，只允许带明确来源的数据写入（#39/#40）；
//! - 每条数据可追溯 `LanguageSource`（#5）。

use serde::{Deserialize, Serialize};

use crate::language::metadata::LanguageMetadata;

/// 语言代码（与 Tatoeba/ISO 对齐：`eng`/`jpn`/`cmn`/`yue`）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageCode {
    Eng,
    Jap,
    Zho,
    Yue,
}

impl LanguageCode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Eng => "English",
            Self::Jap => "Japanese",
            Self::Zho => "Mandarin",
            Self::Yue => "Cantonese",
        }
    }

    #[must_use]
    pub const fn native_label(self) -> &'static str {
        match self {
            Self::Eng => "English",
            Self::Jap => "日本語",
            Self::Zho => "普通话",
            Self::Yue => "廣東話",
        }
    }

    /// Tatoeba/ISO-639-3 代码（句子用）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Eng => "eng",
            Self::Jap => "jpn",
            Self::Zho => "cmn",
            Self::Yue => "yue",
        }
    }

    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.to_ascii_lowercase().as_str() {
            "eng" | "en" => Some(Self::Eng),
            "jpn" | "ja" | "jp" | "jap" => Some(Self::Jap),
            "cmn" | "zh" | "zho" => Some(Self::Zho),
            "yue" | "cantonese" => Some(Self::Yue),
            _ => None,
        }
    }
}

/// 词条类型（#52）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LanguageItemType {
    Word,
    Phrase,
    Sentence,
    Dialogue,
    Passage,
    Grammar,
    Pronunciation,
}

impl LanguageItemType {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Word => "单词",
            Self::Phrase => "短语",
            Self::Sentence => "句子",
            Self::Dialogue => "对话",
            Self::Passage => "篇章",
            Self::Grammar => "语法",
            Self::Pronunciation => "发音",
        }
    }
}

/// 统一语言词条。`id` 为稳定内容 id（如 `jmdict:1002990`、`wn:reservation%1:10:00::`），天然去重。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageItem {
    pub id: String,
    pub language: LanguageCode,
    pub item_type: LanguageItemType,
    /// 书写形式（lemma / 表记 / 简体 / 繁体原样）。
    pub text: String,
    /// 读音（假名 / IPA / jyutping…），可选。
    pub reading: Option<String>,
    /// 罗马字 / 宽式转写，可选（如 `taberu`、`lü3 xing2`）。
    pub romanization: Option<String>,
    /// 该语言专属元数据（serde_json 存储）。
    pub meta: Option<LanguageMetadata>,
    pub source: String,
}

impl LanguageItem {
    #[must_use]
    pub fn plain(
        language: LanguageCode,
        item_type: LanguageItemType,
        id: String,
        text: String,
        source: String,
    ) -> Self {
        Self {
            id,
            language,
            item_type,
            text,
            reading: None,
            romanization: None,
            meta: None,
            source,
        }
    }
}

/// 发音方案（#9/#54/#57）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PronunciationScheme {
    /// CMUdict ARPABET（英文，数字后缀为 stress）。
    Arpabet,
    /// IPA（OEWN entries 提供；英文）。
    Ipa,
    /// 汉语拼音（`lu:3 xing2`，tone digit 后缀）。
    Pinyin,
    /// 粤拼（`sik6 faan6`，tone digit 后缀）。
    Jyutping,
    /// 假名读音（日文）。
    Kana,
    /// 罗马字（日文）。
    Romaji,
}

/// 一条发音记录。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Pronunciation {
    pub id: String,
    pub item_id: String,
    pub scheme: PronunciationScheme,
    /// 音素串（ARPABET 用空格分隔；IPA 原串；拼音/粤拼原串）。
    pub phonemes: String,
    /// 音调（从拼音/粤拼音节尾部数字解析，如 `sik6` → Some(6)）。
    pub tone: Option<u8>,
    /// 方言/变体标签（如 OEWN 的 GB/US）。
    pub variant: Option<String>,
    pub source: String,
}

/// 词义/释义。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Meaning {
    pub id: String,
    pub item_id: String,
    pub pos: Option<String>,
    /// 英文释义（JMdict gloss / CEDICT gloss / CC-Canto gloss 等）。
    pub gloss: Option<String>,
    /// 释义原文（CC-CEDICT 的整段 `/…/` 内容，保留 `CL:` 等注记）。
    pub raw: Option<String>,
    /// OEWN sense key（如 `reservation%1:10:00::`），可空。
    pub sense_key: Option<String>,
    /// 释义语言（`en` / `yue`…），可取 None 表示按词条语言。
    pub lang: Option<String>,
    pub rank: i64,
    pub source: String,
}

/// 内容关系（#58）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LanguageRelationKind {
    Synonym,
    Antonym,
    FormOf,
    RelatedTo,
    UsedIn,
    TranslationOf,
    BelongsToTopic,
    /// OEWN 语义关系（收纳映射）。
    Hypernym,
    Hyponym,
    Attribute,
    DomainTopic,
    Derivation,
}

impl LanguageRelationKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Synonym => "同义词",
            Self::Antonym => "反义词",
            Self::FormOf => "词形",
            Self::RelatedTo => "相关",
            Self::UsedIn => "用于",
            Self::TranslationOf => "翻译",
            Self::BelongsToTopic => "主题",
            Self::Hypernym => "上位词",
            Self::Hyponym => "下位词",
            Self::Attribute => "属性",
            Self::DomainTopic => "领域",
            Self::Derivation => "派生",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageRelation {
    pub id: String,
    pub from_item_id: String,
    pub to_item_id: String,
    pub kind: LanguageRelationKind,
    pub note: Option<String>,
    pub source: String,
}

/// 句子记录（来自 Tatoeba；CC BY 句必须逐句记录 author/license，#29/#30）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SentenceRecord {
    pub sentence_id: String,
    pub language: LanguageCode,
    pub text: String,
    pub author: Option<String>,
    /// 数据包级许可（CC0 / CC BY 2.0 FR）——逐句保存，便于溯源。
    pub license: String,
    pub source: String,
}

/// 音频资产（#37）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AudioType {
    Recorded,
    Tts,
    UserRecording,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AudioAsset {
    pub id: String,
    pub item_id: String,
    pub language: LanguageCode,
    pub text: String,
    pub voice: Option<String>,
    pub provider: String,
    pub audio_type: AudioType,
    pub local_path: Option<String>,
    pub remote_source: Option<String>,
    pub generated_at: Option<i64>,
    pub source_license: Option<String>,
}

/// 每语言条目计数（Stats / Sources 页用）。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LanguageCount {
    pub language: LanguageCode,
    pub words: i64,
    pub phrases: i64,
    pub sentences: i64,
    pub total: i64,
}
