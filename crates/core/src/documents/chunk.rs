//! 确定性分块（V6 §32/§33）。
//!
//! `chunk_text` 是**纯函数**：同样的输入永远得到同样的 chunk（可单测、可回归）。
//! 切分策略：
//! 1. 按目标字符数取窗口；
//! 2. 窗口末端回退到最近的换行/段界（不超过窗口的 30%），避免切断句子；
//! 3. 相邻 chunk 保留 `overlap_chars` 重叠，保持指代连续；
//! 4. 记录最近的 Markdown ATX 标题作为 `section`（§37 引用）；
//! 5. 字符索引为 UTF-8 字符位置（非字节），与前端 JS 一致。

use super::model::{ChunkConfig, DocumentChunk, DocumentLocation};

/// 分块入口。空文本 → 空结果（不产生空 chunk）。
#[must_use]
pub fn chunk_text(document_id: &str, text: &str, config: &ChunkConfig) -> Vec<DocumentChunk> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    let target = config.target_chars.max(64);
    let overlap = config.overlap_chars.min(target / 2);
    let headings = heading_positions(&chars);

    let mut chunks: Vec<DocumentChunk> = Vec::new();
    let mut start = 0usize;
    while start < total && chunks.len() < config.max_chunks {
        let hard_end = (start + target).min(total);
        let end = if hard_end == total {
            total
        } else {
            break_point(&chars, start, hard_end, target / 4)
        };
        let slice: String = chars[start..end].iter().collect();
        if !slice.trim().is_empty() {
            let ordinal = chunks.len();
            let location = DocumentLocation {
                section: section_for(&headings, start),
                page: None,
                char_start: start,
                char_end: end,
            };
            chunks.push(DocumentChunk {
                document_id: document_id.to_string(),
                chunk_id: chunk_id(document_id, ordinal),
                ordinal,
                text: slice,
                location,
                metadata: serde_json::Value::Null,
            });
        }
        if end == total {
            break;
        }
        let next = end.saturating_sub(overlap);
        start = if next > start { next } else { end };
    }
    chunks
}

/// chunk id（稳定：`document_id` + 顺序号）。
#[must_use]
pub fn chunk_id(document_id: &str, ordinal: usize) -> String {
    format!("{document_id}#{ordinal}")
}

/// 在 `[from, hard_end)` 内寻找安全断点：
/// 优先最后一个空行（段落界），其次最后一个换行，最后中文句读；都没有 → 硬切。
fn break_point(chars: &[char], from: usize, hard_end: usize, min_tail: usize) -> usize {
    let floor = hard_end.saturating_sub(min_tail.max(1)).max(from + 1);
    let window = &chars[floor..hard_end];
    for (index, ch) in window.iter().enumerate().rev() {
        if *ch == '\n' {
            let candidate = floor + index + 1;
            if candidate > from {
                return candidate;
            }
        }
    }
    for (index, ch) in window.iter().enumerate().rev() {
        if matches!(ch, '。' | '！' | '？' | '；' | '.' | '!' | '?' | ';') {
            return floor + index + 1;
        }
    }
    hard_end
}

/// 收集 Markdown ATX 标题的字符位置（`# ` / `## ` …）。
fn heading_positions(chars: &[char]) -> Vec<(usize, String)> {
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut line_start = 0usize;
    let mut index = 0usize;
    while index <= chars.len() {
        let at_end = index == chars.len();
        if at_end || chars[index] == '\n' {
            let line: String = chars[line_start..index].iter().collect();
            if let Some(title) = parse_heading(&line) {
                headings.push((line_start, title));
            }
            line_start = index + 1;
        }
        index += 1;
    }
    headings
}

fn parse_heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = trimmed[level..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let title = rest.trim_end_matches('#').trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

/// 位置 `at` 所属章节 = 不晚于 `at` 的最后一个标题。
fn section_for(headings: &[(usize, String)], at: usize) -> Option<String> {
    headings
        .iter()
        .rev()
        .find(|(position, _)| *position <= at)
        .map(|(_, title)| title.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ChunkConfig {
        ChunkConfig {
            target_chars: 200,
            overlap_chars: 20,
            max_document_bytes: 100_000,
            max_chunks: 100,
        }
    }

    #[test]
    fn empty_text_produces_no_chunks() {
        assert!(chunk_text("doc-1", "", &config()).is_empty());
        assert!(chunk_text("doc-1", "   \n  \n", &config()).is_empty());
    }

    #[test]
    fn short_text_is_single_chunk_with_full_range() {
        let text = "Docker volume 配置说明";
        let chunks = chunk_text("doc-1", text, &config());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
        assert_eq!(chunks[0].ordinal, 0);
        assert_eq!(chunks[0].location.char_start, 0);
        assert_eq!(chunks[0].location.char_end, text.chars().count());
        assert_eq!(chunks[0].chunk_id, "doc-1#0");
    }

    #[test]
    fn long_text_is_split_deterministically_with_overlap() {
        let paragraph = "这是一段用于测试分块的文本。";
        let text = (0..80)
            .map(|index| format!("{index} {paragraph}"))
            .collect::<Vec<_>>()
            .join("\n");
        let first = chunk_text("doc-1", &text, &config());
        let second = chunk_text("doc-1", &text, &config());
        assert_eq!(first, second, "分块必须确定性");
        assert!(first.len() > 1);

        // 覆盖：所有 chunk 的并集必须覆盖全文（允许重叠）。
        let total: usize = text.chars().count();
        let covered: usize = first
            .iter()
            .map(|chunk| chunk.location.char_end)
            .max()
            .unwrap();
        assert_eq!(covered, total);
        assert_eq!(first[0].location.char_start, 0);

        // 有序、连续、无空洞。
        for (index, chunk) in first.iter().enumerate() {
            assert_eq!(chunk.ordinal, index);
            assert!(chunk.location.char_end > chunk.location.char_start);
            if index > 0 {
                let previous = &first[index - 1];
                assert!(chunk.location.char_start < previous.location.char_end);
                assert!(chunk.location.char_start >= previous.location.char_start);
            }
        }
    }

    #[test]
    fn chunks_prefer_line_boundaries() {
        let line = "0123456789";
        let text = (0..40).map(|_| line).collect::<Vec<_>>().join("\n");
        let chunks = chunk_text("doc-1", &text, &config());
        assert!(chunks.len() > 1);
        // 每个 chunk 都应以换行结尾（段落界对齐）。
        for chunk in &chunks[..chunks.len() - 1] {
            assert!(
                chunk.text.ends_with('\n'),
                "chunk 未在行边界结束: {:?}",
                &chunk.text[chunk.text.len().saturating_sub(10)..]
            );
        }
    }

    #[test]
    fn sections_are_tracked_from_markdown_headings() {
        let text = "# 环境\nDocker 数据在 /Volumes/Data/docker\n\n## 网络\n端口 5432\n";
        let chunks = chunk_text("doc-1", text, &config());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].location.section.as_deref(), Some("环境"));

        let long = format!(
            "# 环境\n{}\n## 网络\n{}\n",
            "a".repeat(300),
            "b".repeat(300)
        );
        let chunks = chunk_text("doc-1", &long, &config());
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].location.section.as_deref(), Some("环境"));
        assert_eq!(
            chunks.last().unwrap().location.section.as_deref(),
            Some("网络")
        );
        assert!(chunks[0].location.describe().contains("§环境"));
    }

    #[test]
    fn max_chunks_is_respected() {
        let config = ChunkConfig {
            target_chars: 64,
            overlap_chars: 0,
            max_document_bytes: 1_000_000,
            max_chunks: 3,
        };
        let text = "字".repeat(2_000);
        let chunks = chunk_text("doc-1", &text, &config);
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn no_infinite_loop_when_overlap_exceeds_target() {
        let config = ChunkConfig {
            target_chars: 64,
            overlap_chars: 10_000,
            max_document_bytes: 1_000_000,
            max_chunks: 500,
        };
        let text = "x".repeat(1_000);
        let chunks = chunk_text("doc-1", &text, &config);
        assert!(chunks.len() < 500);
        assert!(chunks.len() > 1);
    }
}
