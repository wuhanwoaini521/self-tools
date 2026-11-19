/**
 * Evidence review_note 清洗：把给用户看的原文引文从内部 QA 批注里拆出来。
 *
 * 数据层（history-data-pipeline）的 `review_note` 同时承载两类信息：
 *   1. 真正对读者有价值的史料原文，形如 `引文：「……」`；
 *   2. 只对管线有意义的批注，例如 `major-batch01：source-grounded（…）`、
 *      `claim_field=background`、`anchor=段落精确锚`、`legacy QA …`。
 * 旧实现把整串直出到 UI，读者看到的是 `claim_field=background；anchor=段落精确锚`。
 * 这里做确定性拆分，数据层保持原样。
 */

export interface ParsedEvidenceText {
  /** 可直接展示的史料原文；没有则为 null。 */
  quote: string | null;
  /** 清洗后仍对读者有意义的说明；没有则为 null。 */
  note: string | null;
}

/** 内部批注特征：命中任一条即整段丢弃。 */
const INTERNAL_SEGMENT_PATTERNS: RegExp[] = [
  /claim_field\s*=/i,
  /anchor\s*=/i,
  /link_status\s*=/i,
  /link_method\s*=/i,
  /role\s*=/i,
  /source-grounded/i,
  /source_reference/i,
  /source-curated/i,
  /source-curated-semantic/i,
  /^legacy\b/i,
  /^historical_text\b/i,
  /文本语料不在本\s*checkout/i,
  /等待知识库/i,
  /知识层章节锚定/i,
  /^已审核/,
  /^候补|^待复核/,
  /needs_linking|pending_knowledge/,
  /（B\d+）|\(B\d+\)/,
  /悬空锚/,
  /重定位/,
  /双核实/,
];

/** 管线批次编号前缀，如 `major01-C：`、`batch02-11：`、`depth01-D2：`。 */
const PIPELINE_PREFIX = /^[A-Za-z][A-Za-z0-9_.-]*[：:]\s*/;

/** 去掉批次前缀后只剩「史记源」这类来源标签，对读者无价值。 */
const LABEL_MAX_CJK = 8;

function cjkCount(text: string): number {
  const matches = text.match(/[\u4e00-\u9fff]/g);
  return matches ? matches.length : 0;
}

const QUOTE_PATTERNS: RegExp[] = [
  /引文：「(.+?)」(?=\s*[；;]|\s*$)/s,
  /原文：「(.+?)」(?=\s*[；;]|\s*$)/s,
];

function isInternalSegment(segment: string): boolean {
  const text = segment.trim();
  if (!text) return true;
  if (INTERNAL_SEGMENT_PATTERNS.some((pattern) => pattern.test(text))) return true;
  // 没有汉字的片段（纯批次号、纯标点、纯英文键名）对读者无意义。
  if (cjkCount(text) === 0) return true;
  const withoutPipelinePrefix = text.replace(PIPELINE_PREFIX, "");
  if (
    withoutPipelinePrefix !== text &&
    cjkCount(withoutPipelinePrefix) <= LABEL_MAX_CJK
  ) {
    return true;
  }
  return false;
}

function stripTrailingPunctuation(text: string): string {
  return text.replace(/[\s，,、；;：:。.]+$/u, "").trim();
}

export function parseEvidenceNote(note?: string | null): ParsedEvidenceText {
  if (!note) return { quote: null, note: null };

  let rest = note;
  let quote: string | null = null;
  for (const pattern of QUOTE_PATTERNS) {
    const match = pattern.exec(rest);
    if (match) {
      quote = match[1].trim();
      rest = rest.replace(match[0], "");
      break;
    }
  }

  const segments = rest
    .split(/[；;]/)
    .map((segment) => stripTrailingPunctuation(segment))
    .filter((segment) => !isInternalSegment(segment));

  const cleaned = stripTrailingPunctuation(segments.join("；"));
  return {
    quote: quote && quote.length > 0 ? quote : null,
    note: cleaned.length > 0 ? cleaned : null,
  };
}
