import DOMPurify from "dompurify";
import { splitSentences } from "../../utils";

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

function sanitizeHtml(input: string): string {
  const sanitizeFn =
    typeof DOMPurify?.sanitize === "function"
      ? DOMPurify.sanitize
      : typeof (DOMPurify as unknown as { default?: { sanitize?: typeof DOMPurify.sanitize } })?.default?.sanitize === "function"
        ? (DOMPurify as unknown as { default: { sanitize: typeof DOMPurify.sanitize } }).default.sanitize
        : (s: string) => s;
  return sanitizeFn(input);
}

function sanitizeContent(input: string): string {
  const clean = sanitizeHtml(input);
  if (typeof DOMParser === "undefined") {
    return clean;
  }
  const parsed = new DOMParser().parseFromString(clean, "text/html");
  const text = parsed.body.textContent ?? "";
  if (parsed.body.children.length === 0 && /<\/?(?:p|div|br|a|ul|ol|li)\b[^>]*>/i.test(text)) {
    return sanitizeHtml(text);
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
      if (match.index > cursor) fragment.append(document.createTextNode(value.slice(cursor)));
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

  // 2. 按标点分句去重（消除相邻重复的句子）；兼容性说明见 utils.splitSentences。
  const sentences = splitSentences(trimmed);
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

    if (unique.length === sentences.filter((s) => s.trim()).length) {
      return trimmed;
    }

    let reconstructed = "";
    for (let i = 0; i < unique.length; i++) {
      const s = unique[i];
      if (i === 0) {
        reconstructed = s;
      } else {
        const prev = unique[i - 1];
        const prevIsCjk = /[。！？，；：\u4e00-\u9fa5]$/.test(prev);
        const currIsCjk = /^[\u4e00-\u9fa5]/.test(s);
        if (prevIsCjk && currIsCjk) {
          reconstructed += s;
        } else {
          reconstructed += " " + s;
        }
      }
    }
    return reconstructed;
  }

  return trimmed;
}

function deduplicateDomParagraphs(document: Document): void {
  const seenTexts = new Set<string>();
  const elements = Array.from(document.querySelectorAll("p, div, blockquote, li"));

  for (const el of elements) {
    // 若段落内包含图片，不直接删除元素，但去重内部纯文本
    const hasImg = el.querySelectorAll("img").length > 0;
    const text = el.textContent?.trim() || "";

    if (text.length >= 8) {
      if (seenTexts.has(text)) {
        if (hasImg) {
          // 只保留 img，清空重复文本
          Array.from(el.childNodes).forEach((node) => {
            if (node.nodeType === Node.TEXT_NODE) node.remove();
          });
        } else {
          el.remove();
        }
      } else {
        seenTexts.add(text);
        // 对段落内部可能自带的多重重复进行清理
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
  if (typeof DOMParser === "undefined") {
    return clean;
  }
  const document = new DOMParser().parseFromString(clean, "text/html");
  normalizeMarkdownLinks(document);
  if (baseUrl) {
    document.querySelectorAll("a[href], img[src]").forEach((element) => {
      const attribute = element.tagName === "A" ? "href" : "src";
      const value = element.getAttribute(attribute);
      if (!value) return;
      try {
        element.setAttribute(attribute, new URL(value, baseUrl).toString());
      } catch {
        element.setAttribute(attribute, value);
      }
    });
  }
  document.querySelectorAll("img").forEach((image) => {
    image.setAttribute("loading", "lazy");
    // 源站图床防盗链 / 404 时隐藏,不留破图占位。
    // 必须用内联属性:addEventListener 不会随 innerHTML 序列化。
    image.setAttribute("onerror", "this.style.display='none'");
  });
  deduplicateDomParagraphs(document);
  return document.body.innerHTML;
}

/** 列表和首页摘要使用纯文本，去除源站截断尾巴上的「查看全文」等链接文字，并消除重复句。 */
export function stripRssHtml(html: string, baseUrl?: string): string {
  if (typeof document === "undefined") {
    const rawText = html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
    return deduplicateRepeatedText(rawText);
  }
  const template = document.createElement("div");
  template.innerHTML = prepareRssContent(html, baseUrl);
  const rawText = (template.textContent || "").replace(/\s+/g, " ").trim();
  const text = deduplicateRepeatedText(rawText);
  return text
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)?\s*(?:查看全文|阅读全文|继续阅读|[Rr]ead\s*[Mm]ore)\s*$/u, "")
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)\s*$/u, "")
    .trim();
}
