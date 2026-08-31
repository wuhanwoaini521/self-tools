//! 网页抓取（需求 #十一）：普通 HTTP 为主，不做浏览器自动化。
//!
//! - `WebFetcher` 为可 mock 的 trait；`HttpWebFetcher` 用 reqwest 实现；
//! - 正文提取（`extract_text`）为纯函数：去噪（script/style/nav/footer/header/广告）、
//!   `<p>/<br>/<li>` 保持段落、解码常见编码（GBK/GB2312 中文站）；
//! - 抓不到可靠正文时**不伪造**：由上层把该来源标记为 SnippetOnly（需求 #四 / #二十七）。

use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;

use crate::error::InfrastructureError;
use devtoolbox_core::travel::{ContentState, TravelDocument};

const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
const USER_AGENT: &str = concat!(
    "DevToolbox/",
    env!("CARGO_PKG_VERSION"),
    " (+travel research)"
);

/// 网页抓取接口（可 mock）。
#[async_trait]
pub trait WebFetcher: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<TravelDocument, InfrastructureError>;
}

/// 普通 HTTP 抓取器。
pub struct HttpWebFetcher {
    client: reqwest::Client,
}

impl HttpWebFetcher {
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WebFetcher for HttpWebFetcher {
    async fn fetch(&self, url: &str) -> Result<TravelDocument, InfrastructureError> {
        let trimmed = url.trim().to_string();
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            return Err(InfrastructureError::TravelFetch(format!(
                "invalid url: {trimmed}"
            )));
        }
        let response = self
            .client
            .get(&trimmed)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header("accept-language", "zh-CN,zh;q=0.9")
            .timeout(FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| InfrastructureError::TravelFetch(error.to_string()))?;
        if !response.status().is_success() {
            return Err(InfrastructureError::TravelFetch(format!(
                "server returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| InfrastructureError::TravelFetch(error.to_string()))?;
        let bytes = &bytes[..bytes.len().min(MAX_BODY_BYTES)];
        // 编码探测：BOM > meta charset > 默认 UTF-8（兼容 GBK 中文站）
        let encoding = detect_encoding(bytes);
        let (text, _, _) = encoding.decode(bytes);
        let (title, content) = extract_text(&text);
        Ok(TravelDocument {
            url: trimmed,
            title,
            state: ContentState::Full,
            content: Some(content),
            snippet: None,
            published_at: None,
            fetched_at: now_unix(),
            provider: Some("http".to_string()),
        })
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

// ---------- 纯解析 / 提取函数（离线可单测） ----------

/// 依据 BOM 或 `<meta charset>` 探测编码；未命中回退 UTF-8。
#[must_use]
pub fn detect_encoding(bytes: &[u8]) -> &'static encoding_rs::Encoding {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return encoding_rs::UTF_8;
    }
    let lowered = String::from_utf8_lossy(head).to_ascii_lowercase();
    // 常见中文站编码按序探测（gb18030 兼容 gbk/gb2312，先判更全的标签）
    let charset = ["gb18030", "gbk", "gb2312", "big5", "utf-8", "utf8"]
        .into_iter()
        .find(|name| lowered.contains(name));
    match charset {
        Some("gb18030") | Some("gbk") | Some("gb2312") => encoding_rs::GBK,
        Some("big5") => encoding_rs::BIG5,
        _ => encoding_rs::UTF_8,
    }
}

/// 噪声标签：整块删除（无子内容保留价值）。
const NOISE_TAGS: &[&str] = &[
    "script", "style", "noscript", "iframe", "svg", "canvas", "form", "nav", "footer", "header",
    "aside",
];

fn noise_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let joined = NOISE_TAGS.join("|");
        Regex::new(&format!(r"(?is)<(?:{joined})\b[^>]*>.*?</(?:{joined})>")).expect("noise regex")
    })
}

/// 带内容的语义标签：转换为换行（保留段落边界）。
const BLOCK_TAGS: &[&str] = &[
    "p", "div", "li", "br", "h1", "h2", "h3", "h4", "h5", "h6", "tr",
];

fn block_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let joined = BLOCK_TAGS.join("|");
        Regex::new(&format!(r"(?i)</(?:{joined})>|<br\s*/?>")).expect("block regex")
    })
}

/// 广告 / 无关容器 class/id 启发式：整块移除（保守匹配，避免误杀正文）。
fn advert_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<(?:div|section|span)[^>]*(?:class|id)=["'][^"']*(?:advert|ad-|ads|banner|recommend|related|share|comment)[^"']*["'][^>]*>.*?</(?:div|section|span)>"#).expect("advert regex")
    })
}

/// 从 HTML 提取 `<title>`（回退 meta description）。
fn extract_title(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let Some(start) = lower.find("<title") else {
        return String::new();
    };
    let Some(open_end) = html[start..].find('>') else {
        return String::new();
    };
    let title_start = start + open_end + 1;
    let Some(close) = html[title_start..].find("</title") else {
        return String::new();
    };
    let title = clean_whitespace(&html[title_start..title_start + close]);
    if title.is_empty() {
        String::new()
    } else {
        title
    }
}

/// 清理空白（含 \u{00A0} 等）。
fn clean_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 正文提取（纯函数）：去噪 → 块转行 → 去标签 → 取语义最丰富的连续段落。
#[must_use]
pub fn extract_text(html: &str) -> (String, String) {
    let title = extract_title(html);
    let body = noise_regex().replace_all(html, " ");
    let body = advert_regex().replace_all(&body, " ");
    let body = block_regex().replace_all(&body, "\n");
    let without_tags = tag_re().replace_all(&body, " ");
    let mut paragraphs: Vec<String> = without_tags
        .split('\n')
        .map(clean_whitespace)
        .map(|line| {
            line.replace("&amp;", "&")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&nbsp;", " ")
        })
        .filter(|line| line.chars().count() >= 30)
        .collect();
    // 去相邻重复段（导航残留常见）
    paragraphs.dedup();
    // 取最长连续文本块（正文通常是最长的块）
    let content = paragraphs
        .iter()
        .max_by_key(|paragraph| paragraph.chars().count())
        .cloned()
        .unwrap_or_default();
    (title, content)
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag regex"))
}

#[cfg(test)]
mod tests {
    use super::{detect_encoding, extract_text};

    const ARTICLE_HTML: &str = r#"<!DOCTYPE html><html><head>
        <title>杭州西湖景区介绍 - 文旅局</title>
        <meta charset="utf-8">
      </head><body>
      <nav>首页 导航 菜单</nav>
      <header>网站头部标语</header>
      <div class="ad-banner">广告内容这里是广告</div>
      <article>
        <p>西湖位于浙江省杭州市，是世界文化遗产，也是杭州最著名的景点。</p>
        <p>西湖十景包括苏堤春晓、断桥残雪等经典景点，全年免费开放。</p>
        <p>建议游玩时间半天到一天，可以从断桥出发步行游览。</p>
      </article>
      <script>var fake = "不应出现在正文";</script>
      <div class="related">相关推荐：其他景点</div>
      <footer>备案信息 版权声明</footer>
      </body></html>"#;

    #[test]
    fn extract_text_removes_noise_and_keeps_content() {
        let (title, content) = extract_text(ARTICLE_HTML);
        assert_eq!(title, "杭州西湖景区介绍 - 文旅局");
        assert!(content.contains("西湖位于浙江省杭州市"));
        assert!(content.contains("世界文化遗产"));
        assert!(!content.contains("广告内容"));
        assert!(!content.contains("不应出现在正文"));
        assert!(!content.contains("nav"));
        assert!(!content.contains("相关推荐"));
    }

    #[test]
    fn extract_text_falls_back_to_longest_paragraph() {
        let html = "<html><body><div><span>短</span></div><div>这一段文字足够长，超过三十个字符，应该被选为正文内容的主体部分。</div></body></html>";
        let (_, content) = extract_text(html);
        assert!(content.contains("足够长"));
    }

    #[test]
    fn extract_text_returns_empty_for_sparse_html() {
        let (title, content) =
            extract_text("<html><head><title>x</title></head><body></body></html>");
        assert_eq!(title, "x");
        assert!(content.is_empty());
    }

    #[test]
    fn detect_encoding_sniffs_meta_charset() {
        assert_eq!(detect_encoding(b"<meta charset=UTF-8>"), encoding_rs::UTF_8);
        assert_eq!(
            detect_encoding(b"<meta charset=\"gb2312\">"),
            encoding_rs::GBK
        );
        assert_eq!(detect_encoding(b"<meta charset=gb18030>"), encoding_rs::GBK);
        assert_eq!(
            detect_encoding(&[0xEF, 0xBB, 0xBF, b'a']),
            encoding_rs::UTF_8
        );
        // 无 meta：回退 UTF-8
        assert_eq!(
            detect_encoding(b"<html><body>hello</body></html>"),
            encoding_rs::UTF_8
        );
    }

    #[test]
    fn gbk_bytes_decode_to_readable_text() {
        // 先按 GBK 编码再解码，验证 decode 往返
        let (gpk_bytes, _, _) = encoding_rs::GBK.encode("西湖");
        let (decoded, _, _) = encoding_rs::GBK.decode(&gpk_bytes);
        assert_eq!(decoded, "西湖");
    }
}
