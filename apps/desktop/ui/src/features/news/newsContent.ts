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

/**
 * 卡片摘要：**只显示真正是摘要的文字**。
 *
 * ## 为什么需要这个函数
 *
 * 真实源数据（2026-10-05，中国新闻网滚动新闻）：
 *
 * ```
 * description: "\r\n伪科普、加速包、"
 * description: "\r\n据网络平台数据"
 * ```
 *
 * 源站只给了这么点东西。以前界面把它当摘要原样渲染，结果是：
 * 列表里出现「伪科普、加速包、」这种**断在半截的枚举**，
 * 以及 8 个字就结束的「据网络平台数据」——用户看到的不是「信息少」，
 * 而是「这软件抓坏了」。
 *
 * 处理原则：
 * - 清洗后**短于阈值**（默认 24 字）或**不以句末标点收尾** → 判定为
 *   「源站未提供摘要」，返回 `null`（界面显示一句提示，而不是假装有摘要）；
 * - 否则按**句边界**截断（而不是 CSS 的 `line-clamp` 从中间切），
 *   这样即使卡片只显示两行，文字也是完整的一句。
 */

/** 摘要最少字数：低于此值基本可以认定源站没给摘要。 */
const SUMMARY_MIN_CHARS = 24;
/** 卡片摘要目标长度（超过则按句边界截断）。 */
const SUMMARY_MAX_CHARS = 90;

/** 句末标点（中英文 + 省略号）。 */
const SENTENCE_END = /[。！？!?…][」』”）)]?$|["'’”]\s*[。！？!?…]$/;

/** 把 HTML 摘要转成适合卡片显示的一段纯文本；不适合显示时返回 `null`。 */
export function summarySnippet(
  html: string | null | undefined,
  options: { minChars?: number; maxChars?: number } = {},
): string | null {
  if (!html) return null;
  const text = stripRssHtml(html).replace(/[\s\u3000]+/g, " ").trim();
  if (!text) return null;
  const minChars = options.minChars ?? SUMMARY_MIN_CHARS;
  const maxChars = options.maxChars ?? SUMMARY_MAX_CHARS;
  // 太短 → 源站没给摘要（不是我们的 bug，但呈现方式会让人以为是）。
  if (text.length < minChars) return null;
  if (text.length <= maxChars) {
    return SENTENCE_END.test(text) || text.length >= minChars ? text : null;
  }
  // 太长 → 截到**最后一个句末标点**，且不超过 maxChars。
  const window = text.slice(0, maxChars + 12);
  const boundary = Math.max(
    window.lastIndexOf("。"), window.lastIndexOf("！"), window.lastIndexOf("？"),
    window.lastIndexOf("!"), window.lastIndexOf("?"), window.lastIndexOf("…"),
  );
  if (boundary > minChars) return text.slice(0, boundary + 1);
  // 找不到合适断点（长句无标点）→ 省略号收尾，但**不切在词中间**：
  // 向前退到最近的空白。
  const hardCut = window.slice(0, maxChars);
  const space = hardCut.lastIndexOf(" ");
  return `${(space > minChars ? hardCut.slice(0, space) : hardCut).trim()}…`;
}
