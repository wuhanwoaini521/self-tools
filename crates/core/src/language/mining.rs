//! 句子挖掘（V13 W3）：把**读过的课文**变成**要主动回忆的复习卡**。
//!
//! ## 为什么这是最值钱的一步
//!
//! 现状是「认得词 ≠ 会用」。学完一课，认得 200 个词，但**没人能在没提示的
//! 情况下把课文的句子说出来**。而真实交流靠的就是整句产出，不是孤立词汇。
//!
//! 做法抄 Anki 的 retrieval practice 与 Language Reactor 的「句子挖掘」：
//! 从**你自己正在学的课文**里挑句，挖掉一个关键位置，做成卡片进同一套 SRS。
//! 好处：
//! - 语境是真的（不是编造例句），记得住；
//! - 复习的是**用**，不是认；
//! - 材料完全来自本地教材，不依赖任何在线服务。
//!
//! ## 挖哪个位置（不是随便挖）
//!
//! Cloze-deletion 的经典结论：**挖功能词（冠词/助动词/介词）比挖内容词更有效**，
//! 因为功能词是中国人说英语时最常错的地方（`the`/`a`/`do`/`is`/`to`/`of`），
//! 而内容词即使忘了也能靠上下文猜出来。
//!
//! 优先级：**本课生词 > 功能词 > 长实词 > 其它**。同优先级按句子内位置取第一个，
//! 保证同一句话重复挖掘得到同一张卡（幂等，不产生重复卡）。
//!
//! 纯函数，无 IO，可单测。

use serde::{Deserialize, Serialize};

use crate::language::course::LessonSentence;

/// 卡的三种形态（对应三种不同的记忆动作）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MinedCardKind {
    /// 填空：给出挖空句，回忆缺失的词（**主动产出**）。
    Cloze,
    /// 听写：听音频写整句（**输入 → 产出**）。
    Dictation,
    /// 中译英：给中文写英文（**翻译产出**，最接近真实交流）。
    Translate,
}

impl MinedCardKind {
    /// 稳定字符串（落库与前端展示共用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cloze => "cloze",
            Self::Dictation => "dictation",
            Self::Translate => "translate",
        }
    }
}

/// 挖空位置的选取理由（可解释：告诉用户「为什么挖这个」）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlankReason {
    /// 本课生词（学过但还没掌握）。
    LessonVocab,
    /// 功能词（冠词/助动词/介词/连词…）—— 最容易说错的地方。
    FunctionWord,
    /// 较长的实词。
    ContentWord,
    /// 兜底：句中没有更好的目标（短句）。
    Fallback,
}

/// 一张挖掘出来的卡（纯数据；落库由应用层负责）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MinedCard {
    pub kind: MinedCardKind,
    /// 句子 id（稳定 id 的来源）。
    pub sentence_id: String,
    pub sequence: u32,
    /// 完整原句。
    pub sentence: String,
    /// 中文译文（有则给；没有就不编造）。
    pub chinese: Option<String>,
    /// 挖空后的题干（`Cloze` 才有）。
    pub prompt: Option<String>,
    /// 答案（`Cloze` = 被挖掉的词；`Translate` = 原句；`Dictation` = 原句）。
    pub answer: String,
    /// 音频区间（`Dictation` 用）。
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    /// 挖空理由（可解释性）。
    pub reason: BlankReason,
}

/// 功能词表：中国学习者最常漏掉的那些。
///
/// 只收**真的会说错**的：冠词、助动词、介词、连词、代词。
/// 刻意不含介词搭配里的名词（`handbag`）——那些属于内容词。
const FUNCTION_WORDS: &[&str] = &[
    // 冠词
    "a", "an", "the", // 冠词是中国学习者最大痛点
    // 助动词与 be
    "is", "are", "am", "was", "were", "be", "been", "being", "do", "does", "did", "have", "has",
    "had", "will", "would", "can", "could", "shall", "should", "may", "might",
    "must", // 情态与时态
    // 介词
    "to", "of", "in", "on", "at", "for", "with", "from", "by", "about", "into", "over", "under",
    "after", "before", "between", "without", "near", "through", // 时间/方位/方式介词
    // 连词与关系词
    "and", "but", "or", "because", "if", "when", "that", "which", "who", "than",
    "so", // 逻辑连接
    // 代词与限定词
    "this", "that", "these", "those", "my", "your", "his", "her", "its", "our", "their", "not", "i",
    "me", "you", "he", "him", "she", "it", "we", "us", "they", "them", "mine", "yours",
];

/// 单词切分（保留原形与标点分离；`don't` / `hand-bag` 视作一个词）。
#[must_use]
pub fn split_words(sentence: &str) -> Vec<(usize, usize)> {
    let bytes = sentence.as_bytes();
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (index, byte) in bytes.iter().enumerate() {
        let is_word = byte.is_ascii_alphabetic() || *byte == b'\'' || *byte == b'-';
        match (is_word, start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                out.push((begin, index));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        out.push((begin, bytes.len()));
    }
    out
}

/// 挖空目标评分：分数越高越该挖（纯函数，便于测试与解释）。
#[must_use]
pub fn blank_score(normalized: &str, is_lesson_vocab: bool) -> u32 {
    if is_lesson_vocab {
        // 生词优先：本课学了但还没掌握的词，正是最该反复提取的。
        return 100;
    }
    if FUNCTION_WORDS.contains(&normalized) {
        return 80;
    }
    // 较长实词其次（短词如 "is" 已在功能词表，"car" 挖了没信息量）。
    if normalized.chars().count() >= 6 {
        return 60;
    }
    20
}

/// 从一句话挖出一张填空卡。
///
/// - `lesson_vocab`：本课生词（小写 lemma），挖它们优先级最高；
/// - 没有可挖目标（无英文 / 只有极短句）→ 返回 `None`（不硬造卡）。
#[must_use]
pub fn mine_cloze(sentence: &LessonSentence, lesson_vocab: &[String]) -> Option<MinedCard> {
    let text = sentence.english.trim();
    if text.is_empty() {
        return None;
    }
    let ranges = split_words(text);
    if ranges.is_empty() {
        return None;
    }
    let mut best: Option<(u32, usize, BlankReason, String)> = None;
    for (begin, end) in ranges.iter().copied() {
        let surface = &text[begin..end];
        let normalized = surface.to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        let is_vocab = lesson_vocab.iter().any(|word| word == &normalized);
        let score = blank_score(&normalized, is_vocab);
        let reason = if is_vocab {
            BlankReason::LessonVocab
        } else if FUNCTION_WORDS.contains(&normalized.as_str()) {
            BlankReason::FunctionWord
        } else if normalized.chars().count() >= 6 {
            BlankReason::ContentWord
        } else {
            BlankReason::Fallback
        };
        // `>` 保证同分取**先出现**的位置 → 同一句话重复挖掘结果稳定。
        let better = best
            .as_ref()
            .is_none_or(|(best_score, _, _, _)| score > *best_score);
        if better {
            best = Some((score, begin, reason, surface.to_string()));
        }
    }
    let (_, begin, reason, answer) = best?;
    // 整句就是一个词（如标题行 "Lesson 1"）：挖掉就没信息了，不做卡。
    if ranges.len() <= 1 {
        return None;
    }
    Some(MinedCard {
        kind: MinedCardKind::Cloze,
        sentence_id: sentence.id.clone(),
        sequence: sentence.sequence,
        sentence: text.to_string(),
        chinese: sentence.chinese.clone(),
        prompt: Some(join_blank(&text[..begin], &text[begin + answer.len()..])),
        answer,
        start_ms: Some(sentence.start_ms),
        end_ms: Some(sentence.end_ms),
        reason,
    })
}

/// 拼接挖空后的句子：去掉多余空格，标点前不留空。
///
/// 朴素拼接会得到 `Is this your  _____ ?`（双空格 + 标点前多一个空格）——
/// 读起来像排版事故，用户会以为页面坏了。
fn join_blank(left: &str, right: &str) -> String {
    let head = left.trim_end();
    let tail = right.trim_start();
    // 原句里标点紧贴单词时，挖空后也不要再补空格。
    let tight = tail.starts_with(['.', ',', ';', ':', '!', '?', ')', ']', '"', '\'']);
    if head.is_empty() {
        return format!("_____ {tail}");
    }
    if tail.is_empty() {
        return format!("{head} _____");
    }
    if tight {
        format!("{head} _____{tail}")
    } else {
        format!("{head} _____ {tail}")
    }
}

/// 听写卡（听音频写整句；要求有中文提示与音频区间）。
#[must_use]
pub fn mine_dictation(sentence: &LessonSentence) -> Option<MinedCard> {
    let text = sentence.english.trim();
    // 少于 3 个词的句子（"Yes?"）当听写题没有意义。
    if split_words(text).len() < 3 {
        return None;
    }
    Some(MinedCard {
        kind: MinedCardKind::Dictation,
        sentence_id: sentence.id.clone(),
        sequence: sentence.sequence,
        sentence: text.to_string(),
        chinese: sentence.chinese.clone(),
        prompt: None,
        answer: text.to_string(),
        start_ms: Some(sentence.start_ms),
        end_ms: Some(sentence.end_ms),
        reason: BlankReason::Fallback,
    })
}

/// 中译英卡（必须有中文译文；没有译文就不编）。
#[must_use]
pub fn mine_translate(sentence: &LessonSentence) -> Option<MinedCard> {
    let text = sentence.english.trim();
    let chinese = sentence
        .chinese
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    if split_words(text).len() < 3 {
        return None;
    }
    Some(MinedCard {
        kind: MinedCardKind::Translate,
        sentence_id: sentence.id.clone(),
        sequence: sentence.sequence,
        sentence: text.to_string(),
        chinese: Some(chinese),
        prompt: None,
        answer: text.to_string(),
        start_ms: None,
        end_ms: None,
        reason: BlankReason::Fallback,
    })
}

/// 一句话能挖出的全部卡（填空 + 听写 + 中译英）。
///
/// 「有中文才有中译英」：没有译文就不硬造翻译题（项目反复强调：不编造）。
#[must_use]
pub fn mine_sentence(sentence: &LessonSentence, lesson_vocab: &[String]) -> Vec<MinedCard> {
    let mut out = Vec::new();
    if let Some(card) = mine_cloze(sentence, lesson_vocab) {
        out.push(card);
    }
    if let Some(card) = mine_dictation(sentence) {
        out.push(card);
    }
    if let Some(card) = mine_translate(sentence) {
        out.push(card);
    }
    out
}

/// 一课的挖掘候选（按句子顺序，去重后返回；`max_per_kind` 限制每种卡的量）。
#[must_use]
pub fn mine_lesson(
    sentences: &[LessonSentence],
    lesson_vocab: &[String],
    max_per_kind: usize,
) -> Vec<MinedCard> {
    let mut out = Vec::new();
    for kind in [
        MinedCardKind::Cloze,
        MinedCardKind::Dictation,
        MinedCardKind::Translate,
    ] {
        let mut taken = 0;
        for sentence in sentences {
            if taken >= max_per_kind {
                break;
            }
            let card = match kind {
                MinedCardKind::Cloze => mine_cloze(sentence, lesson_vocab),
                MinedCardKind::Dictation => mine_dictation(sentence),
                MinedCardKind::Translate => mine_translate(sentence),
            };
            if let Some(card) = card {
                out.push(card);
                taken += 1;
            }
        }
    }
    out
}

/// 稳定卡片 id（同一句话 + 同一种卡 → 同一 id，重复挖掘不产生重复卡）。
#[must_use]
pub fn mined_card_id(lesson_id: &str, card: &MinedCard) -> String {
    format!(
        "language:nce-sentence:{}:{}:{}",
        lesson_id,
        card.sequence,
        card.kind.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(sequence: u32, english: &str, chinese: Option<&str>) -> LessonSentence {
        LessonSentence {
            id: format!("s-{sequence}"),
            lesson_id: "nce:1:1".into(),
            sequence,
            start_ms: sequence as i64 * 1_000,
            end_ms: sequence as i64 * 1_000 + 900,
            english: english.into(),
            chinese: chinese.map(str::to_string),
        }
    }

    #[test]
    fn split_words_keeps_contractions_and_hyphens() {
        let words: Vec<&str> = split_words("Don't put it in the hand-bag, please.")
            .iter()
            .map(|(b, e)| &"Don't put it in the hand-bag, please."[*b..*e])
            .collect();
        assert_eq!(
            words,
            vec!["Don't", "put", "it", "in", "the", "hand-bag", "please"]
        );
    }

    #[test]
    fn cloze_prefers_lesson_vocab_over_function_words() {
        // 句子里有生词 handbag，也有功能词 the。
        let card = mine_cloze(
            &sentence(0, "Is this your handbag?", Some("这是你的手提包吗？")),
            &["handbag".to_string()],
        )
        .expect("cloze");
        assert_eq!(card.answer, "handbag");
        assert_eq!(card.reason, BlankReason::LessonVocab);
        assert_eq!(card.prompt.as_deref(), Some("Is this your _____?"));
    }

    #[test]
    fn cloze_falls_back_to_function_word_when_no_new_vocab() {
        let card = mine_cloze(&sentence(1, "Excuse me!", Some("对不起！")), &[]).expect("cloze");
        assert_eq!(card.answer, "me");
        assert_eq!(card.reason, BlankReason::FunctionWord);
    }

    #[test]
    fn cloze_is_deterministic_for_the_same_sentence() {
        let s = sentence(2, "Whose handbag is it?", None);
        let first = mine_cloze(&s, &[]).expect("first");
        let second = mine_cloze(&s, &[]).expect("second");
        assert_eq!(
            first.answer, second.answer,
            "同分取先出现的位置 → 可重复挖掘"
        );
        assert_eq!(
            mined_card_id("nce:1:1", &first),
            mined_card_id("nce:1:1", &second)
        );
    }

    #[test]
    fn short_or_empty_sentences_are_not_mined() {
        // 单词句（标题行）挖掉就没信息了。
        assert!(mine_cloze(&sentence(0, "Lesson 1", None), &[]).is_none());
        assert!(mine_dictation(&sentence(0, "Yes?", None)).is_none());
        assert!(mine_translate(&sentence(0, "Yes it is.", None)).is_none());
        // 没有译文不做翻译卡（不编造）。
        assert!(mine_translate(&sentence(1, "It is my handbag.", None)).is_none());
        assert!(mine_cloze(&sentence(2, "   ", None), &[]).is_none());
    }

    #[test]
    fn dictation_and_translate_need_three_words() {
        let short = sentence(0, "Excuse me!", Some("对不起！"));
        assert!(
            mine_dictation(&short).is_none(),
            "两个词的句子当听写题没意义"
        );
        let long = sentence(1, "Is this your handbag?", Some("这是你的手提包吗？"));
        let dictation = mine_dictation(&long).expect("dictation");
        assert_eq!(dictation.answer, "Is this your handbag?");
        assert_eq!(dictation.start_ms, Some(1_000));
        let translate = mine_translate(&long).expect("translate");
        assert_eq!(translate.prompt, None);
        assert_eq!(translate.chinese.as_deref(), Some("这是你的手提包吗？"));
        assert_eq!(translate.answer, "Is this your handbag?");
    }

    #[test]
    fn mine_lesson_respects_per_kind_cap_and_order() {
        let sentences: Vec<LessonSentence> = (0..5)
            .map(|n| {
                sentence(
                    n,
                    &format!("Whose handbag number {n} is it exactly now?"),
                    Some(&format!("这是第 {n} 个手提包吗？")),
                )
            })
            .collect();
        let cards = mine_lesson(&sentences, &[], 2);
        let cloze = cards
            .iter()
            .filter(|c| c.kind == MinedCardKind::Cloze)
            .count();
        let dictation = cards
            .iter()
            .filter(|c| c.kind == MinedCardKind::Dictation)
            .count();
        let translate = cards
            .iter()
            .filter(|c| c.kind == MinedCardKind::Translate)
            .count();
        assert_eq!((cloze, dictation, translate), (2, 2, 2), "每种卡按上限截断");
        // 顺序：先全部 cloze，再 dictation，再 translate。
        assert!(cards[0].kind == MinedCardKind::Cloze);
        assert_eq!(cards[2].kind, MinedCardKind::Dictation);
    }

    #[test]
    fn blank_prompt_keeps_original_spacing_discipline() {
        // 标点紧贴：挖空后不应变成 "_____ ?"
        let card = mine_cloze(
            &sentence(0, "Is this your handbag?", None),
            &["handbag".into()],
        )
        .expect("cloze");
        assert_eq!(card.prompt.as_deref(), Some("Is this your _____?"));
        // 句中标点：前后空格各一个，不多不少
        let card = mine_cloze(
            &sentence(1, "Please open the door, and come in.", None),
            &["door".into()],
        )
        .expect("cloze");
        assert_eq!(
            card.prompt.as_deref(),
            Some("Please open the _____, and come in.")
        );
        // 句尾挖空：不留悬空空格
        let card = mine_cloze(
            &sentence(2, "I want to apologize.", None),
            &["apologize".into()],
        )
        .expect("cloze");
        assert_eq!(card.prompt.as_deref(), Some("I want to _____."));
        // 句首挖空
        let card =
            mine_cloze(&sentence(3, "Excuse me, sir.", None), &["excuse".into()]).expect("cloze");
        assert_eq!(card.prompt.as_deref(), Some("_____ me, sir."));
    }

    #[test]
    fn function_word_list_covers_the_actual_pain_points() {
        for word in [
            "the", "a", "is", "do", "to", "of", "and", "that", "me", "it",
        ] {
            assert!(FUNCTION_WORDS.contains(&word), "{word} 应当是功能词");
        }
        for word in ["handbag", "excuse", "answer"] {
            assert!(!FUNCTION_WORDS.contains(&word), "{word} 不是功能词");
        }
    }
}
