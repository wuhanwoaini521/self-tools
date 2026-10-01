import DOMPurify from "dompurify";

const MARKDOWN_LINK_PATTERN = /^\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)$/;
const MARKDOWN_LINKS_PATTERN = /\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g;

interface MarkdownLink {
  label: string;
  url: string;
}

function parseMarkdownLink(value: string): MarkdownLink | null {
  const match = MARKDOWN_LINK_PATTERN.exec(value.trim());
  return match ? { label: match[1], url: match[2] } : null;
}

/**
 * 少数 Feed 会把 HTML 再编码一层，导致 `<p>` 先变成普通文本。
 * 仅当净化结果完全没有元素且文本本身看起来是 HTML 时解码，避免扩大可执行内容范围。
 */
function sanitizeContent(input: string): string {
  const clean = DOMPurify.sanitize(input);
  const parsed = new DOMParser().parseFromString(clean, "text/html");
  const text = parsed.body.textContent ?? "";
  if (parsed.body.children.length === 0 && /<\/?(?:p|div|br|a|ul|ol|li)\b[^>]*>/i.test(text)) {
    return DOMPurify.sanitize(text);
  }
  return clean;
}

function normalizeMarkdownLinks(document: Document): void {
  document.querySelectorAll("a").forEach((anchor) => {
    const link = parseMarkdownLink(anchor.getAttribute("href") ?? "")
      ?? parseMarkdownLink(anchor.textContent ?? "");
    if (!link) return;
    anchor.setAttribute("href", link.url);
    anchor.textContent = link.label;
  });

  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const textNodes: Text[] = [];
  let current: Node | null;
  while ((current = walker.nextNode())) textNodes.push(current as Text);

  textNodes.forEach((textNode) => {
    if (textNode.parentElement?.closest("a, code, pre, script, style")) return;
    const value = textNode.nodeValue ?? "";
    MARKDOWN_LINKS_PATTERN.lastIndex = 0;
    if (!MARKDOWN_LINKS_PATTERN.test(value)) return;
    MARKDOWN_LINKS_PATTERN.lastIndex = 0;

    const fragment = document.createDocumentFragment();
    let cursor = 0;
    let match: RegExpExecArray | null;
    while ((match = MARKDOWN_LINKS_PATTERN.exec(value))) {
      if (match.index > cursor) fragment.append(document.createTextNode(value.slice(cursor, match.index)));
      const anchor = document.createElement("a");
      anchor.href = match[2];
      anchor.textContent = match[1];
      fragment.append(anchor);
      cursor = match.index + match[0].length;
    }
    if (cursor < value.length) fragment.append(document.createTextNode(value.slice(cursor)));
    textNode.replaceWith(fragment);
  });
}

function resolveUrl(href: string, baseUrl?: string): string {
  try { return new URL(href, baseUrl).toString(); } catch { return href; }
}

/**
 * 消除 RSS 源中因多图 alt/caption 重复导致的连续重复句或完全重复段落。
 */
export function deduplicateRepeatedText(text: string): string {
  if (!text || text.length < 15) return text;

  const trimmed = text.trim();

  // 1. 检测整段文本由同一模式周期性重复构成（例如 2~10 次重复的图片说明）
  for (let len = 10; len <= Math.floor(trimmed.length / 2); len++) {
    const unit = trimmed.slice(0, len).trim();
    if (unit.length < 8) continue;
    const parts = trimmed.split(unit);
    if (parts.length >= 3 && parts.every((p) => p.trim().length === 0 || p.trim() === unit)) {
      return unit;
    }
  }

  // 2. 按标点分句去重（消除相邻重复的句子）
  const sentences = trimmed.split(/(?<=[。！？\n.!?])\s*/);
  if (sentences.length > 1) {
    const unique: string[] = [];
    for (const raw of sentences) {
      const s = raw.trim();
      if (!s) continue;
      if (unique.length > 0 && unique[unique.length - 1] === s) {
        continue;
      }
      const existingCount = unique.filter((item) => item === s).length;
      if (existingCount >= 2 && s.length >= 8) {
        continue;
      }
      unique.push(s);
    }
    return unique.join(" ");
  }

  return trimmed;
}

function deduplicateDomParagraphs(document: Document): void {
  const seenTexts = new Set<string>();
  const elements = Array.from(document.querySelectorAll("p, div, blockquote, li"));

  for (const el of elements) {
    const hasImg = el.querySelectorAll("img").length > 0;
    const text = el.textContent?.trim() || "";

    if (text.length >= 8) {
      if (seenTexts.has(text)) {
        if (hasImg) {
          Array.from(el.childNodes).forEach((node) => {
            if (node.nodeType === Node.TEXT_NODE) node.remove();
          });
        } else {
          el.remove();
        }
      } else {
        seenTexts.add(text);
        const clean = deduplicateRepeatedText(text);
        if (clean !== text && !hasImg) {
          el.textContent = clean;
        }
      }
    }
  }
}

/** RSS 正文统一净化、修复 Markdown 链接，并补全相对链接与消除重复段落。 */
export function prepareRssContent(html: string, baseUrl?: string): string {
  const clean = sanitizeContent(html);
  const document = new DOMParser().parseFromString(clean, "text/html");
  normalizeMarkdownLinks(document);
  if (baseUrl) {
    document.querySelectorAll("a[href], img[src]").forEach((element) => {
      const attribute = element.tagName === "A" ? "href" : "src";
      const value = element.getAttribute(attribute);
      if (value) element.setAttribute(attribute, resolveUrl(value, baseUrl));
    });
  }
  document.querySelectorAll("img").forEach((image) => image.setAttribute("loading", "lazy"));
  deduplicateDomParagraphs(document);
  return document.body.innerHTML;
}

/** 列表和首页摘要使用纯文本，去除源站截断尾巴上的「查看全文」等链接文字，并消除重复句。 */
export function stripRssHtml(html: string, baseUrl?: string): string {
  const template = document.createElement("div");
  template.innerHTML = prepareRssContent(html, baseUrl);
  const rawText = (template.textContent || "").replace(/\s+/g, " ").trim();
  const text = deduplicateRepeatedText(rawText);
  return text
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)?\s*(?:查看全文|阅读全文|继续阅读|[Rr]ead\s*[Mm]ore)\s*$/u, "")
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)\s*$/u, "")
    .trim();
}
