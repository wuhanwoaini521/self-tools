//! 每语言专属元数据（#53–#57）。
//!
//! 级别字段（cefr/jlpt/hsk）**一律 Optional**：只有带明确来源与许可的数据才允许填入（#39）。
//! 来源说明写入对应字段文档注释，UI 展示时必须标注来源，绝不叫 LLM 猜级别。

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnglishMetadata {
    /// ARPABET（CMUdict，`pronunciation_scheme=ARPABET`，#9）。
    pub arpabet: Option<String>,
    /// 音素列表（空格分隔）。
    pub phonemes: Vec<String>,
    /// 重音位置标记（派生自 ARPABET 数字后缀；示例用 `0/1/2`）。
    pub stress: Vec<u8>,
    /// CEFR：Optional，仅允许带来源写入（V1 恒 None）。
    pub cefr: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct JapaneseMetadata {
    /// 假名读音。
    pub kana: Option<String>,
    /// 罗马字（Hepburn，由 kana 派生）。
    pub romaji: Option<String>,
    /// 汉字表记（keb）。
    pub kanji: Option<String>,
    /// JLPT：Optional。KANJIDIC2 的 `jlpt` 字段有 EDRDG 文档来源，
    /// 导入时标记为 `kanjidic2_jlpt`，UI 必须标注来源；词汇级一律不推导。
    pub jlpt: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MandarinMetadata {
    pub simplified: Option<String>,
    pub traditional: Option<String>,
    /// 拼音（原样，tone digit 后缀）。
    pub pinyin: Option<String>,
    /// 逐音节音调（如 `[3, 2]`）。
    pub tones: Vec<u8>,
    /// HSK：Optional，V1 恒 None（无明确来源，#20）。
    pub hsk: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CantoneseMetadata {
    pub traditional: Option<String>,
    pub simplified: Option<String>,
    /// 粤拼（每词一个读音串，如 `sik6 faan6`）。
    pub jyutping: Option<String>,
    /// 逐音节音调（由 jyutping 尾部数字解析，#26）。
    pub tones: Vec<u8>,
}

/// 语言元数据枚举（#53）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "lang", rename_all = "lowercase")]
pub enum LanguageMetadata {
    English(EnglishMetadata),
    Japanese(JapaneseMetadata),
    Mandarin(MandarinMetadata),
    Cantonese(CantoneseMetadata),
}

impl LanguageMetadata {
    /// 从拼音/粤拼音节串提取逐音节音调（`lu:3 xing2` → [3, 2]；`sik6 faan6` → [6, 6]）。
    pub fn tones_from_syllables(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .filter_map(|syllable| {
                syllable
                    .chars()
                    .last()
                    .and_then(|ch| ch.to_digit(10))
                    .filter(|digit| (1..=9).contains(digit))
                    .map(|digit| digit as u8)
            })
            .collect()
    }
}
