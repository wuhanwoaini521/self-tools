//! 日语假名 → 罗马字（Hepburn）转换与拼音/粤拼音调提取（纯函数，可离线单测）。

/// 假名 → 罗马字（修正 Hepburn）。
/// 覆盖清音/濁音/半濁音/拗音(きゃ系)/促音っ/長音ー/撥音ん 及「を」。
/// 拗音在「大假名 + 小假名」相遇时合并（きょう → kyou），未知假名原样保留。
#[must_use]
pub fn kana_to_romaji(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        // 拗音：大假名 + 小假名（ゃゅょ）→ 合并为一个音节
        if let Some((base, vowel)) = youon_base(ch).and_then(|base| {
            chars
                .get(index + 1)
                .and_then(|&small| small_vowel(small).map(|vowel| (base, vowel)))
        }) {
            out.push_str(base);
            // し/ち/じ 系直接拼元音（sha/cha/ja），其余插 y（kya/nya…）
            if !matches!(base, "sh" | "ch" | "j") {
                out.push('y');
            }
            out.push_str(vowel);
            index += 2;
            continue;
        }
        // 促音っ
        if matches!(ch, 'っ' | 'ッ') {
            if let Some(&next) = chars.get(index + 1) {
                let double = consonant_of(next);
                if let Some(d) = double {
                    out.push_str(d);
                    index += 1;
                    continue;
                }
            }
            out.push('\'');
            index += 1;
            continue;
        }
        // 撥音：ん + ばぱま行 → m
        if matches!(ch, 'ん' | 'ン') {
            if let Some(&next) = chars.get(index + 1) {
                if matches!(
                    next,
                    'ば' | 'び'
                        | 'ぶ'
                        | 'べ'
                        | 'ぼ'
                        | 'ぱ'
                        | 'ぴ'
                        | 'ぷ'
                        | 'ぺ'
                        | 'ぽ'
                        | 'ま'
                        | 'み'
                        | 'む'
                        | 'め'
                        | 'も'
                ) {
                    out.push('m');
                } else {
                    out.push('n');
                }
            } else {
                out.push('n');
            }
            index += 1;
            continue;
        }
        if ch == 'ー' {
            index += 1;
            continue;
        }
        // 普通假名
        if let Some(roman) = kana_syllable(ch) {
            out.push_str(roman);
            index += 1;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    out
}

/// 可构成拗音的大假名 → 辅音前缀（×+ゃ/ゅ/ょ）。
fn youon_base(ch: char) -> Option<&'static str> {
    match ch {
        'き' | 'キ' => Some("k"),
        'し' | 'シ' => Some("sh"),
        'ち' | 'チ' => Some("ch"),
        'に' | 'ニ' => Some("n"),
        'ひ' | 'ヒ' => Some("h"),
        'み' | 'ミ' => Some("m"),
        'り' | 'リ' => Some("r"),
        'ぎ' | 'ギ' => Some("g"),
        'じ' | 'ジ' => Some("j"),
        'ぢ' | 'ヂ' => Some("j"),
        'び' | 'ビ' => Some("b"),
        'ぴ' | 'ピ' => Some("p"),
        _ => None,
    }
}

/// 小假名 → 元音罗马字（拗音第二拍）。
fn small_vowel(ch: char) -> Option<&'static str> {
    match ch {
        'ゃ' | 'ャ' => Some("a"),
        'ゅ' | 'ュ' => Some("u"),
        'ょ' | 'ョ' => Some("o"),
        _ => None,
    }
}

/// 大假名的辅音开头（用于っ 的促音双写）。
fn consonant_of(ch: char) -> Option<&'static str> {
    match ch {
        'か' | 'き' | 'く' | 'け' | 'こ' | 'カ' | 'キ' | 'ク' | 'ケ' | 'コ' => Some("k"),
        'さ' | 'し' | 'す' | 'せ' | 'そ' | 'サ' | 'シ' | 'ス' | 'セ' | 'ソ' => Some("s"),
        'た' | 'ち' | 'つ' | 'て' | 'と' | 'タ' | 'チ' | 'ツ' | 'テ' | 'ト' => Some("t"),
        'は' | 'ひ' | 'ふ' | 'へ' | 'ほ' | 'ハ' | 'ヒ' | 'フ' | 'ヘ' | 'ホ' => Some("h"),
        'ば' | 'び' | 'ぶ' | 'べ' | 'ぼ' | 'バ' | 'ビ' | 'ブ' | 'ベ' | 'ボ' => Some("b"),
        'ぱ' | 'ぴ' | 'ぷ' | 'ぺ' | 'ぽ' | 'パ' | 'ピ' | 'プ' | 'ペ' | 'ポ' => Some("p"),
        'ま' | 'み' | 'む' | 'め' | 'も' | 'マ' | 'ミ' | 'ム' | 'メ' | 'モ' => Some("m"),
        _ => None,
    }
}

/// 单个假名的读音（修正 Hepburn 主表）。
fn kana_syllable(ch: char) -> Option<&'static str> {
    match ch {
        'あ' | 'ア' => Some("a"),
        'い' | 'イ' => Some("i"),
        'う' | 'ウ' => Some("u"),
        'え' | 'エ' => Some("e"),
        'お' | 'オ' => Some("o"),
        'か' | 'カ' => Some("ka"),
        'き' | 'キ' => Some("ki"),
        'く' | 'ク' => Some("ku"),
        'け' | 'ケ' => Some("ke"),
        'こ' | 'コ' => Some("ko"),
        'さ' | 'サ' => Some("sa"),
        'し' | 'シ' => Some("shi"),
        'す' | 'ス' => Some("su"),
        'せ' | 'セ' => Some("se"),
        'そ' | 'ソ' => Some("so"),
        'た' | 'タ' => Some("ta"),
        'ち' | 'チ' => Some("chi"),
        'つ' | 'ツ' => Some("tsu"),
        'て' | 'テ' => Some("te"),
        'と' | 'ト' => Some("to"),
        'な' | 'ナ' => Some("na"),
        'に' | 'ニ' => Some("ni"),
        'ぬ' | 'ヌ' => Some("nu"),
        'ね' | 'ネ' => Some("ne"),
        'の' | 'ノ' => Some("no"),
        'は' | 'ハ' => Some("ha"),
        'ひ' | 'ヒ' => Some("hi"),
        'ふ' | 'フ' => Some("fu"),
        'へ' | 'ヘ' => Some("he"),
        'ほ' | 'ホ' => Some("ho"),
        'ま' | 'マ' => Some("ma"),
        'み' | 'ミ' => Some("mi"),
        'む' | 'ム' => Some("mu"),
        'め' | 'メ' => Some("me"),
        'も' | 'モ' => Some("mo"),
        'や' | 'ヤ' => Some("ya"),
        'ゆ' | 'ユ' => Some("yu"),
        'よ' | 'ヨ' => Some("yo"),
        'ら' | 'ラ' => Some("ra"),
        'り' | 'リ' => Some("ri"),
        'る' | 'ル' => Some("ru"),
        'れ' | 'レ' => Some("re"),
        'ろ' | 'ロ' => Some("ro"),
        'わ' | 'ワ' => Some("wa"),
        'を' | 'ヲ' => Some("o"),
        'が' | 'ガ' => Some("ga"),
        'ぎ' | 'ギ' => Some("gi"),
        'ぐ' | 'グ' => Some("gu"),
        'げ' | 'ゲ' => Some("ge"),
        'ご' | 'ゴ' => Some("go"),
        'ざ' | 'ザ' => Some("za"),
        'じ' | 'ジ' => Some("ji"),
        'ず' | 'ズ' => Some("zu"),
        'ぜ' | 'ゼ' => Some("ze"),
        'ぞ' | 'ゾ' => Some("zo"),
        'だ' | 'ダ' => Some("da"),
        'ぢ' | 'ヂ' => Some("ji"),
        'づ' | 'ヅ' => Some("zu"),
        'で' | 'デ' => Some("de"),
        'ど' | 'ド' => Some("do"),
        'ば' | 'バ' => Some("ba"),
        'び' | 'ビ' => Some("bi"),
        'ぶ' | 'ブ' => Some("bu"),
        'べ' | 'ベ' => Some("be"),
        'ぼ' | 'ボ' => Some("bo"),
        'ぱ' | 'パ' => Some("pa"),
        'ぴ' | 'ピ' => Some("pi"),
        'ぷ' | 'プ' => Some("pu"),
        'ぺ' | 'ペ' => Some("pe"),
        'ぽ' | 'ポ' => Some("po"),
        _ => None,
    }
}

/// 从拼音/粤拼串提取逐音节音调（`lu:3 xing2` → [3, 2]；`sik6 faan6` → [6, 6]）。
#[must_use]
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

/// 拼音/粤拼规范化（一线多用）：
/// - `u:` / `ü` 系列 → `v`（ü 的键盘表示，便于搜索）；
/// - 移除 tone 数字（`xing2` → `xing`）与声调符号（`ā á ǎ à` → `a`）。
#[must_use]
pub fn normalize_roman(text: &str) -> String {
    // 先处理 u: → v（字符级无法表达双字符映射）
    let mut normalized = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(':') {
        normalized.push_str(&rest[..pos]);
        // 冒号前一字符若是 u/U → 视作 ü → 替换为 v
        if normalized.ends_with('u') {
            normalized.pop();
            normalized.push('v');
        } else {
            normalized.push(':');
        }
        rest = &rest[pos + 1..];
    }
    normalized.push_str(rest);

    let mut out = String::with_capacity(normalized.len());
    for ch in normalized.chars() {
        match ch {
            'ü' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' => out.push('v'),
            '0'..='9' => {}
            // 带声调元音 → 去声调
            'ā' | 'á' | 'ǎ' | 'à' => out.push('a'),
            'ē' | 'é' | 'ě' | 'è' => out.push('e'),
            'ī' | 'í' | 'ǐ' | 'ì' => out.push('i'),
            'ō' | 'ó' | 'ǒ' | 'ò' => out.push('o'),
            'ū' | 'ú' | 'ǔ' | 'ù' => out.push('u'),
            _ => out.push(ch.to_ascii_lowercase()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hepburn_basics() {
        assert_eq!(kana_to_romaji("たべる"), "taberu");
        assert_eq!(kana_to_romaji("にほん"), "nihon");
        assert_eq!(kana_to_romaji("がっこう"), "gakkou");
        assert_eq!(kana_to_romaji("きょう"), "kyou");
        assert_eq!(kana_to_romaji("しょ"), "sho");
        assert_eq!(kana_to_romaji("じゅぎょう"), "jugyou");
    }

    #[test]
    fn hepburn_mixed_kana() {
        assert_eq!(kana_to_romaji("とうきょう"), "toukyou");
        assert_eq!(kana_to_romaji("ありがとう"), "arigatou");
    }

    #[test]
    fn tones_extraction() {
        assert_eq!(tones_from_syllables("lu:3 xing2"), vec![3, 2]);
        assert_eq!(tones_from_syllables("sik6 faan6"), vec![6, 6]);
        assert_eq!(tones_from_syllables("nihongo"), Vec::<u8>::new());
    }

    #[test]
    fn roman_normalization() {
        assert_eq!(normalize_roman("lu:3 xing2"), "lv xing");
        assert_eq!(normalize_roman("sik6 faan6"), "sik faan");
        assert_eq!(normalize_roman("taberu"), "taberu");
    }
}
