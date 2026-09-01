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

/** RSS 正文统一净化、修复 Markdown 链接，并补全相对链接。 */
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
  return document.body.innerHTML;
}

/** 列表和首页摘要使用纯文本，去除源站截断尾巴上的「查看全文」等链接文字。 */
export function stripRssHtml(html: string, baseUrl?: string): string {
  const template = document.createElement("div");
  template.innerHTML = prepareRssContent(html, baseUrl);
  const text = (template.textContent || "").replace(/\s+/g, " ").trim();
  return text
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)?\s*(?:查看全文|阅读全文|继续阅读|[Rr]ead\s*[Mm]ore)\s*$/u, "")
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)\s*$/u, "")
    .trim();
}
