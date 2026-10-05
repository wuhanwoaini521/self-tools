/**
 * 复习会话（`reviewQueue` + `submitReview`）。
 *
 * 支持卡片类型：`recall` / `fill_blank` / `multiple_choice` / `qa`
 * （`map_locate` 不属于语言模块，遇到时按 `recall` 的文本形态安全降级）。
 *
 * 快捷键：Space/Enter 揭晓答案，1→again 2→hard 3→good 4→easy，←/→ 切卡，Esc 退出。
 * 打分只在答案揭晓后生效；`userAnswer` 对非输入型卡片恒为空串，后端接受。
 */
import { useCallback, useMemo, useState } from "react";
import { CheckCircle, SkipForward, X } from "@phosphor-icons/react";
import type {
  Mistake,
  ReviewQueueItem,
  ReviewScheduleOutcome,
  UniversalReviewCard,
  UniversalReviewRating,
} from "../../types";
import { languageClient } from "./languageClient";
import { errorMessage } from "../../utils";
import { Action, Chip, ProgressTrack } from "./LanguagePrimitives";
import {
  EMPTY_COPY,
  RATING_LABELS,
  cx,
  formatStamp,
  useAsyncPanel,
  useLanguageShortcuts,
} from "./languageUi";
import { matchAnswer, matchFeedback } from "./reviewMatch";

const RATINGS: UniversalReviewRating[] = ["again", "hard", "good", "easy"];

/** 只有这两类卡片需要用户输入答案；其余的 `userAnswer` 传空串。 */
function needsInput(card: UniversalReviewCard): boolean {
  return card.card_type === "fill_blank" || card.card_type === "qa";
}

function cardKindLabel(card: UniversalReviewCard): string {
  switch (card.card_type) {
    case "recall":
      return "回忆";
    case "fill_blank":
      return "填空";
    case "multiple_choice":
      return "选择";
    case "qa":
      return "问答";
    default:
      return "卡片";
  }
}

export interface ReviewSessionProps {
  /** 由调用方提供队列（通常来自 reviewQueue(50)）。 */
  queue: ReviewQueueItem[];
  onClose: () => void;
  /** 每次成功提交后回调，让外层刷新「今日」等面板。 */
  onSubmitted?: (outcome: ReviewScheduleOutcome) => void;
  /** 队列里是否存在这张卡对应的错题（决定要不要显示「答对即清除错题」提示）。 */
  resolvesMistake?: boolean;
}

/**
 * 复习会话 UI。队列为空时给完整空态文案，而不是一个空白卡片。
 */
export function ReviewSession({
  queue,
  onClose,
  onSubmitted,
  resolvesMistake,
}: ReviewSessionProps) {
  const [index, setIndex] = useState(0);
  const [revealed, setRevealed] = useState(false);
  const [answer, setAnswer] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastOutcome, setLastOutcome] = useState<ReviewScheduleOutcome | null>(null);
  const [done, setDone] = useState(0);

  const entry = queue[index] ?? null;
  const card = entry?.card ?? null;

  const reset = useCallback((nextIndex: number) => {
    setIndex(nextIndex);
    setRevealed(false);
    setAnswer("");
    setError(null);
  }, []);

  const submit = useCallback(
    async (rating: UniversalReviewRating) => {
      if (!card || submitting) return;
      setSubmitting(true);
      setError(null);
      try {
        const outcome = await languageClient.submitReview(
          card.id,
          rating,
          // 非输入型卡片没有用户答案；传空串而不是 undefined，
          // 保证参数形状在所有卡片类型上一致。
          needsInput(card) ? answer : "",
        );
        setLastOutcome(outcome);
        setDone((count) => count + 1);
        onSubmitted?.(outcome);
        if (index + 1 < queue.length) reset(index + 1);
        else setRevealed(false);
      } catch (err) {
        setError(errorMessage(err));
      } finally {
        setSubmitting(false);
      }
    },
    [answer, card, index, onSubmitted, queue.length, reset, submitting],
  );

  useLanguageShortcuts({
    onReveal: () => {
      if (!card) return;
      if (!revealed) setRevealed(true);
      else void submit("good");
    },
    onRate: (rating) => {
      if (revealed && !submitting) void submit(rating);
    },
    onPrev: () => {
      if (index > 0) reset(index - 1);
    },
    onNext: () => {
      if (index + 1 < queue.length) reset(index + 1);
    },
    onClose,
  });

  // 选项为 null 的 multiple_choice 不能渲染按钮；退化成揭晓答案。
  const options = useMemo(
    () => (card && card.card_type === "multiple_choice" ? (card.options ?? []) : []),
    [card],
  );

  // 作答与标准答案的**词级**比对（句子卡必需：全等判定会把
  // 「excuse me」对「Excuse me!」判错）。
  const match = useMemo(
    () => matchAnswer(answer, card?.answer ?? ""),
    [answer, card],
  );

  if (queue.length === 0) {
    return (
      <div className="lang-review" role="dialog" aria-modal="true">
        <header className="lang-review-top">
          <h2>复习</h2>
          <button type="button" className="lang-link" onClick={onClose}>
            <X size={15} /> 退出
          </button>
        </header>
        <p className="lang-empty">{EMPTY_COPY.review}</p>
      </div>
    );
  }

  if (!card) {
    // 队列在会话进行中被外部清空（其它面板刷新过）：收尾而不是崩溃。
    return (
      <div className="lang-review" role="dialog" aria-modal="true">
        <p className="lang-empty">这一轮复习已经做完了。</p>
        <Action onClick={onClose}>回到首页</Action>
      </div>
    );
  }

  return (
    <div className="lang-review" role="dialog" aria-modal="true">
      <header className="lang-review-top">
        <div>
          <h2>
            复习 {index + 1} / {queue.length}
          </h2>
          <ProgressTrack value={index + 1} total={queue.length} />
        </div>
        <div className="lang-row-actions">
          <Chip tone={entry?.is_overdue ? "danger" : "plain"}>
            {entry?.is_overdue ? "已逾期" : "到期"}
          </Chip>
          <button type="button" className="lang-link" onClick={onClose}>
            <X size={15} /> 退出 (Esc)
          </button>
        </div>
      </header>

      <article className="lang-review-card">
        <p className="lang-muted">{cardKindLabel(card)}</p>
        <h3 className="lang-review-question">{card.prompt}</h3>
        {card.hint ? <p className="lang-muted">提示：{card.hint}</p> : null}
        {card.context ? (
          <blockquote className="lang-review-context">{card.context}</blockquote>
        ) : null}

        {needsInput(card) ? (
          <div className="lang-field">
            <textarea
              value={answer}
              onChange={(event) => setAnswer(event.target.value)}
              placeholder="写下你的答案（可直接揭晓对照）"
              rows={3}
            />
          </div>
        ) : null}

        {options.length > 0 ? (
          <ul className="lang-options">
            {options.map((option) => (
              <li key={option}>
                <button
                  type="button"
                  disabled={revealed}
                  onClick={() => {
                    setAnswer(option);
                    setRevealed(true);
                  }}
                >
                  {option}
                </button>
              </li>
            ))}
          </ul>
        ) : null}

        {revealed ? (
          <div className="lang-review-answer">
            <p className="lang-review-word">{card.answer}</p>
            {answer && needsInput(card) ? (
              <p className={cx("lang-muted", match.exact && "is-correct")}>
                你的答案：{answer}
              </p>
            ) : null}
            {answer && needsInput(card) && !match.exact ? (
              <p className="lang-muted">{matchFeedback(match)}</p>
            ) : null}
          </div>
        ) : (
          <p className="lang-muted">
            按 <kbd>空格</kbd> 或 <kbd>Enter</kbd> 揭晓答案。
          </p>
        )}
      </article>

      {error ? (
        <p className="lang-inline-error-text">
          {error} <button type="button" className="lang-link" onClick={() => void submit("again")}>重试</button>
        </p>
      ) : null}
      {lastOutcome ? (
        <p className="lang-muted">
          已记录：正确率 {lastOutcome.is_correct ? "命中" : "未命中"} · 下次复习{" "}
          {formatStamp(lastOutcome.due_at)}（间隔 {lastOutcome.interval_days} 天，复习{" "}
          {lastOutcome.repetition_count} 次）
          {resolvesMistake ? " · 答对后错题会一并清除" : null}
        </p>
      ) : null}
      {done > 0 ? <p className="lang-muted">本轮已完成 {done} 张。</p> : null}

      <footer className="lang-rate-actions">
        <Action variant="ghost" onClick={onClose}>
          <SkipForward size={14} /> 结束
        </Action>
        {RATINGS.map((rating, ratingIndex) => (
          <button
            key={rating}
            type="button"
            className={cx("lang-rate", `is-${rating}`)}
            disabled={!revealed || submitting}
            onClick={() => void submit(rating)}
          >
            {ratingIndex + 1} · {RATING_LABELS[rating]}
          </button>
        ))}
      </footer>
      {!revealed ? (
        <p className="lang-muted">揭晓答案后才会出现 1–4 打分。</p>
      ) : (
        <p className="lang-muted">
          <CheckCircle size={12} /> 1 重来 · 2 困难 · 3 良好 · 4 简单；←/→ 切换卡片。
        </p>
      )}
    </div>
  );
}

/**
 * 错题复习：列出 `mistakes()`，并把「Got it」路由到 `submitReview`。
 *
 * 后端在一次**正确**的复习提交里清除对应错题，所以这里的做法是：
 * 找到这条错题对应的卡片（按 `entity_id` 匹配），用 `good` + 正确答案提交，
 * 然后刷新列表——**不在前端把错题删掉**。
 */
export function MistakesPanel({
  mistakes,
  queue,
  loading = false,
  error,
  reload,
  onResolved,
}: {
  mistakes: readonly Mistake[];
  queue: readonly ReviewQueueItem[];
  loading?: boolean;
  error: string | null;
  reload: () => void;
  onResolved?: () => void;
}) {
  const [busyId, setBusyId] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const cardIdFor = useCallback(
    (itemId: string): string | null => {
      const match = queue.find((row) => row.card.entity_id === itemId);
      return match ? match.card.id : null;
    },
    [queue],
  );

  const resolve = async (itemId: string, correctAnswer: string) => {
    const cardId = cardIdFor(itemId);
    if (!cardId) {
      setFailure(
        "这条错题目前没有对应的复习卡片（可能已经到期被清除）。可以等它重新出现在复习队列里再确认。",
      );
      return;
    }
    setBusyId(itemId);
    setFailure(null);
    try {
      await languageClient.submitReview(cardId, "good", correctAnswer);
      onResolved?.();
      reload();
    } catch (err) {
      setFailure(errorMessage(err));
    } finally {
      setBusyId(null);
    }
  };

  if (error) {
    return (
      <div className="lang-inline-error">
        <p>{error}</p>
        <button type="button" className="lang-link" onClick={reload}>
          重试
        </button>
      </div>
    );
  }

  return (
    <ul className="lang-mistake-list">
      {mistakes.map((mistake) => (
        <li key={mistake.id}>
          <div className="lang-mistake-head">
            <b>{mistake.content}</b>
            <Chip tone="danger">错 {mistake.error_count} 次</Chip>
          </div>
          <p className="lang-mistake-question">{mistake.question}</p>
          <p className="lang-mistake-answer is-wrong">
            你的答案：{mistake.user_answer || "（未作答）"}
          </p>
          <p className="lang-mistake-answer is-right">
            正确答案：{mistake.correct_answer || "（未记录）"}
          </p>
          <div className="lang-row-actions">
            <button
              type="button"
              className="lang-primary"
              disabled={busyId === mistake.item_id}
              onClick={() => void resolve(mistake.item_id, mistake.correct_answer)}
            >
              {busyId === mistake.item_id ? "提交中…" : "Got it"}
            </button>
            {!cardIdFor(mistake.item_id) ? (
              <span className="lang-muted">暂无对应卡片</span>
            ) : null}
          </div>
        </li>
      ))}
      {failure ? <p className="lang-inline-error-text">{failure}</p> : null}
    </ul>
  );
}

/** 队列加载器（今日 / 复习页共用）。 */
export function useReviewQueue(limit = 50) {
  return useAsyncPanel<ReviewQueueItem[]>(
    () => languageClient.reviewQueue(limit),
    [limit],
  );
}