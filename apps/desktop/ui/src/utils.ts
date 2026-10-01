/** 跨 Feature 共享的纯工具函数(无 UI 依赖)。 */

export function isTauriRuntime() { return "__TAURI_INTERNALS__" in window; }

export function errorMessage(error: unknown) {
  return typeof error === "string" ? error : error && typeof error === "object" && "message" in error ? String(error.message) : "操作失败，请查看开发者日志。";
}

export function fileName(value: string) { return value.split(/[\\/]/).filter(Boolean).pop() || value; }

/** 句末标点：CJK 与 ASCII 并列，换行也算一句的边界。 */
const SENTENCE_BOUNDARY = "。！？\n.!?";

/**
 * 按句末标点切句，标点归入前一句，空白不保留。
 *
 * 与「在句末标点后切分并吞掉后续空白」等价，但**刻意不使用 lookbehind**：
 * lookbehind 需要 Safari 16.4+（macOS 13+）。macOS 12 的 WKWebView 仍是 Safari 15.6，
 * 会把 `(?<=` 当成空名捕获组，抛 `SyntaxError: invalid group specifier name`，
 * 使整个模块图解析失败、应用白屏。
 */
export function splitSentences(text: string): string[] {
  const sentences: string[] = [];
  let buffer = "";
  for (const char of text) {
    buffer += char;
    if (SENTENCE_BOUNDARY.includes(char)) {
      const sentence = buffer.trim();
      if (sentence) sentences.push(sentence);
      buffer = "";
    }
  }
  const tail = buffer.trim();
  if (tail) sentences.push(tail);
  return sentences;
}

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Unix 秒 → 相对时间(如「10 分钟前」),超过一周显示日期。 */
export function formatRelativeTime(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "未知时间";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);
  if (diff < MINUTE) return "刚刚";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} 分钟前`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)} 小时前`;
  if (diff < 7 * DAY) return `${Math.floor(diff / DAY)} 天前`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}

export function formatDateTime(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "未知时间";
  return new Date(unixSeconds * 1000).toLocaleString();
}

export function greetingByHour(hour: number): string {
  if (hour < 5) return "夜深了";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}
