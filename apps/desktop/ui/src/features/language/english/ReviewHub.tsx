/**
 * 英语复习中心（Review Hub）。
 *
 * 复用平台 SRS（`learning.db` 的 review_cards），只过滤 module = language——
 * 与全局 Review Center 是**同一批卡片**，不建第二套复习。
 *
 * 一次复习会做的事（任务书 §21/§23）：
 * - 四档评分 Again / Hard / Good / Easy → 后端 SM-2 计算下次时间；
 * - 单词卡显示「在哪学过」（occurrence），答错时顺手回看原句。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, SpeakerHigh } from "@phosphor-icons/react";
import type { ReviewQueueItem, UniversalReviewRating } from "../../../types";
import { errorMessage } from "../../../utils";
import { cx } from "../languageUi";
import { speak } from "../tts";
import { learningClient } from "../../learning/learningClient";
import { englishClient } from "./englishClient";
import type { WordLookup } from "../../../types";

export interface ReviewHubProps {
  onBack: () => void;
  onDone: () => void;
}

const RATINGS: Array<{ value: UniversalReviewRating; label: string; hint: string }> = [
  { value: "again", label: "忘记", hint: "今天再来一次" },
  { value: "hard", label: "困难", hint: "明天再见" },
  { value: "good", label: "记得", hint: "3 天后" },
  { value: "easy", label: "轻松", hint: "更久以后" },
];

export function ReviewHub({ onBack, onDone }: ReviewHubProps) {
  const [queue, setQueue] = useState<ReviewQueueItem[] | null>(null);
  const [index, setIndex] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [context, setContext] = useState<WordLookup | null>(null);
  const [completedCount, setCompletedCount] = useState(0);

  const load = useCallback(async () => {
    setError(null);
    try {
      const items = await learningClient.getReviewQueue("language", 50);
      setQueue(items);
      setIndex(0);
      setRevealed(false);
      setCompletedCount(0);
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const current = queue?.[index] ?? null;

  // 单词卡顺带查一次词典 + 遇见历史（一次调用，缓存到本轮）。
  useEffect(() => {
    if (!current || current.card.entity_type !== "word") {
      setContext(null);
      return;
    }
    const word = current.card.entity_id.replace(/^en:/, "");
    let alive = true;
    englishClient
      .lookupWord(word, null, null)
      .then((result) => {
        if (alive) setContext(result);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [current]);

  const total = queue?.length ?? 0;

  const rate = useCallback(
    async (rating: UniversalReviewRating) => {
      if (!current || busy) return;
      setBusy(true);
      setError(null);
      try {
        await learningClient.submitReview(current.card.id, rating);
        setCompletedCount((previous) => previous + 1);
        setRevealed(false);
        setIndex((previous) => {
          const next = previous + 1;
          if (next >= total) {
            onDone();
            return previous;
          }
          return next;
        });
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setBusy(false);
      }
    },
    [busy, current, onDone, total],
  );

  const estimate = useMemo(() => Math.max(1, Math.ceil(total * 0.25)), [total]);

  if (error && !queue) {
    return (
      <div className="en-library">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-ghost-btn" onClick={() => void load()}>
          重试
        </button>
      </div>
    );
  }

  return (
    <div className="en-library en-review">
      <header className="en-page-head">
        <div className="en-title-row">
          <button type="button" className="en-icon-btn" onClick={onBack} title="返回">
            <ArrowLeft size={16} />
          </button>
          <div>
            <p className="en-greeting-hello">Review</p>
            <h1>
              {total > 0 ? `${total} 张卡片 · 约 ${estimate} 分钟` : "没有到期复习"}
            </h1>
          </div>
        </div>
        <div className="en-row-actions">
          <span className="en-muted">
            已完成 {completedCount}/{total}
          </span>
        </div>
      </header>

      {total === 0 ? (
        <section className="en-card">
          <p className="en-muted">
            今天没有到期的复习。学习新课时标记的「不认识 / 模糊」单词会出现在这里。
          </p>
          <button type="button" className="en-link-btn" onClick={onBack}>
            返回
          </button>
        </section>
      ) : null}

      {current ? (
        <section className="en-card en-review-card-body">
          <div className="en-progress-line">
            <div
              className="en-progress-track"
              role="progressbar"
              aria-valuenow={index + 1}
              aria-valuemax={total}
            >
              <i style={{ width: `${(completedCount / Math.max(1, total)) * 100}%` }} />
            </div>
            <span>
              {index + 1}/{total}
            </span>
          </div>

          <div className="en-review-prompt">
            <p className="en-review-kind">
              {current.card.entity_type === "word"
                ? "单词"
                : current.card.entity_type === "lesson"
                  ? "整课复习"
                  : current.card.entity_type}
            </p>
            <h2>{current.card.prompt}</h2>
            {current.card.hint ? (
              <p className="en-phonetic">/{current.card.hint}/</p>
            ) : null}
            <button
              type="button"
              className="en-speak-btn"
              onClick={() => speak(current.card.prompt, "eng")}
            >
              <SpeakerHigh size={14} /> 朗读
            </button>
          </div>

          {revealed ? (
            <div className="en-review-answer">
              <p className="en-translation">{current.card.answer}</p>
              {context && context.seen_count > 0 ? (
                <p className="en-muted">
                  见过 {context.seen_count} 次
                  {context.occurrences.find((item) => item.source_type === "lesson")
                    ? ` · 课程：${context.occurrences.find((item) => item.source_type === "lesson")?.source_id}`
                    : ""}
                </p>
              ) : null}
              {context?.entry?.translation_zh && context.entry.translation_zh !== current.card.answer ? (
                <p className="en-muted">词典释义：{context.entry.translation_zh}</p>
              ) : null}
            </div>
          ) : (
            <button
              type="button"
              className="en-primary-btn is-wide"
              onClick={() => setRevealed(true)}
            >
              显示答案
            </button>
          )}

          {revealed ? (
            <div className="en-rating-row">
              {RATINGS.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  className={cx("en-rating-btn", `is-${item.value}`)}
                  onClick={() => void rate(item.value)}
                  disabled={busy}
                  title={item.hint}
                >
                  <strong>{item.label}</strong>
                  <small>{item.hint}</small>
                </button>
              ))}
            </div>
          ) : null}

          {error ? <p className="en-inline-error">{error}</p> : null}
        </section>
      ) : null}
    </div>
  );
}