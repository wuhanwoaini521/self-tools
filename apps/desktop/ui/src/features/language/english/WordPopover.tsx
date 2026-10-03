/**
 * 查词浮层：课内点击任意单词即可查释义，不跳页面（任务书 §30 动线要求）。
 *
 * 数据全部来自真实后端：`lookupWord` 返回词典条目 + 遇见历史 + 平台学习状态。
 * 词典没命中时**如实显示「未收录」**，不编造释义。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { SpeakerHigh, X, Sparkle } from "@phosphor-icons/react";
import type { WordLookup, WordMark } from "../../../types";
import { errorMessage } from "../../../utils";
import { speak } from "../tts";
import { englishClient } from "./englishClient";
import { Action, Chip } from "../LanguagePrimitives";

export interface WordPopoverProps {
  word: string;
  /** 词在课文中的原句（用于「在哪学的」与 AI 上下文）。 */
  sentence: string | null;
  lessonId: string | null;
  /** 加入生词本（三态）。 */
  onMark?: (word: string, mark: WordMark) => Promise<void> | void;
  /** 打开 AI 讲解（走全局 AI 面板，带上下文）。 */
  onAskAi?: (prompt: string) => void;
  /** AI 是否可用；不可用时隐藏入口（而不是禁用成灰）。 */
  aiAvailable?: boolean;
  onClose: () => void;
}

const SOURCE_LABELS: Record<string, string> = {
  lesson: "课程",
  reading: "阅读",
  lookup: "查词",
  quiz: "测验",
};

function describeSource(sourceType: string, sourceId: string): string {
  if (sourceType === "lesson") {
    const match = /^nce:(\d+):(\d+)$/.exec(sourceId);
    if (match) return `New Concept English ${match[1]} · Lesson ${match[2]}`;
    return `课程 ${sourceId}`;
  }
  return SOURCE_LABELS[sourceType] ?? sourceType;
}

export function WordPopover({
  word,
  sentence,
  lessonId,
  onMark,
  onAskAi,
  aiAvailable = false,
  onClose,
}: WordPopoverProps) {
  const [lookup, setLookup] = useState<WordLookup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [markBusy, setMarkBusy] = useState(false);
  const [marked, setMarked] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    englishClient
      .lookupWord(word, sentence, lessonId)
      .then((result) => {
        if (!alive) return;
        setLookup(result);
        setLoading(false);
      })
      .catch((cause: unknown) => {
        if (!alive) return;
        setError(errorMessage(cause));
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [word, sentence, lessonId]);

  // Esc 关闭；点击外部关闭。
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const onPointer = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) onClose();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onPointer);
    };
  }, [onClose]);

  const mark = useCallback(
    async (value: WordMark) => {
      if (!onMark) return;
      setMarkBusy(true);
      try {
        await onMark(word, value);
        setMarked(value);
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setMarkBusy(false);
      }
    },
    [onMark, word],
  );

  const entry = lookup?.entry ?? null;
  const firstSource = lookup?.occurrences.find(
    (item) => item.source_type === "lesson",
  );

  return (
    <div className="en-wordpop" ref={rootRef} role="dialog" aria-label={`${word} 释义`}>
      <header className="en-wordpop-head">
        <div>
          <h4>{word}</h4>
          {entry?.phonetic ? <span className="en-phonetic">/{entry.phonetic}/</span> : null}
        </div>
        <div className="en-row-actions">
          <button
            type="button"
            className="en-icon-btn"
            onClick={() => speak(word, "eng")}
            title="朗读"
            aria-label={`朗读 ${word}`}
          >
            <SpeakerHigh size={15} />
          </button>
          <button
            type="button"
            className="en-icon-btn"
            onClick={onClose}
            title="关闭"
            aria-label="关闭"
          >
            <X size={15} />
          </button>
        </div>
      </header>

      {loading ? <p className="en-muted">查询中…</p> : null}
      {error ? <p className="en-inline-error">{error}</p> : null}

      {!loading && !error && !entry ? (
        <p className="en-muted">
          词典里没有这个词（{word}）。可以先按「不认识」加入复习，之后在词典导入后自动补齐释义。
        </p>
      ) : null}

      {entry ? (
        <div className="en-wordpop-body">
          {entry.pos ? <p className="en-pos">{entry.pos}</p> : null}
          {entry.translation_zh ? (
            <p className="en-translation">{entry.translation_zh}</p>
          ) : null}
          {entry.definition_en && entry.definition_en !== entry.translation_zh ? (
            <p className="en-definition">{entry.definition_en}</p>
          ) : null}

          {entry.tags.length > 0 ? (
            <div className="en-chip-row">
              {entry.tags.slice(0, 6).map((tag) => (
                <Chip key={tag}>{tag}</Chip>
              ))}
              {entry.collins > 0 ? <Chip tone="accent">Collins {entry.collins}★</Chip> : null}
            </div>
          ) : null}

          {entry.forms.length > 0 ? (
            <p className="en-forms">
              {entry.forms
                .filter(([kind]) => kind !== "0")
                .slice(0, 4)
                .map(([kind, form]) => (
                  <span key={kind} className="en-form">
                    {form}
                  </span>
                ))}
            </p>
          ) : null}

          {lookup?.learning ? (
            <p className="en-muted">
              已学 · 掌握度 {Math.round(lookup.learning.mastery_score)}%
              {lookup.learning.next_review_at ? " · 已进入复习计划" : ""}
            </p>
          ) : null}

          {/* 「在哪见过」——self-tools 的特色：生词来源可追溯 */}
          <div className="en-occurrences">
            <p className="en-occurrences-title">
              见过 {lookup?.seen_count ?? 0} 次
              {firstSource ? ` · 首次：${describeSource(firstSource.source_type, firstSource.source_id)}` : ""}
            </p>
            {lookup && lookup.occurrences.length > 1 ? (
              <ul>
                {lookup.occurrences.slice(0, 4).map((item, index) => (
                  <li key={`${item.source_id}-${index}`}>
                    <span className="en-occ-source">
                      {describeSource(item.source_type, item.source_id)}
                    </span>
                    {item.sentence ? <em>{item.sentence}</em> : null}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>

          {onMark ? (
            <div className="en-wordpop-actions">
              <Action
                onClick={() => void mark("know")}
                disabled={markBusy || marked !== null}
                variant={marked === "know" ? "ghost" : "primary"}
              >
                认识
              </Action>
              <Action
                onClick={() => void mark("fuzzy")}
                disabled={markBusy || marked !== null}
                variant="ghost"
              >
                模糊
              </Action>
              <Action
                onClick={() => void mark("unknown")}
                disabled={markBusy || marked !== null}
                variant="danger"
              >
                不认识
              </Action>
            </div>
          ) : null}

          {aiAvailable && onAskAi ? (
            <button
              type="button"
              className="en-link-btn"
              onClick={() =>
                onAskAi(
                  [
                    `我在学 New Concept English，遇到单词 "${word}"。`,
                    entry.translation_zh ? `课本/词典释义：${entry.translation_zh}` : "",
                    sentence ? `课文原句：${sentence}` : "",
                    "请解释这个词在句中的含义、常见搭配，并给 2 个例句。",
                  ]
                    .filter(Boolean)
                    .join("\n"),
                )
              }
            >
              <Sparkle size={14} /> 让 AI 讲解这个词
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}