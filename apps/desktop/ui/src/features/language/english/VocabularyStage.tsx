/**
 * 课前单词预习（Vocabulary 阶段）。
 *
 * ## 交互原则（对照主流背词软件）
 *
 * - **标了「认识」就消失**：不再出现在本轮队列里。这是用户最直观的反馈——
 *   词不见了 = 学会了。原先是「沉到底部」，和未标记的词混在一起，
 *   用户根本分不清哪些标过（只看到一排含义不明的小圆点）。
 * - **「模糊 / 不认识」放队尾稍后再考**：不该立刻消失（还没掌握），但也不该
 *   挡在下一个词前面。
 * - **进度一眼可见**：「还剩 N 个」+ 三态图例（未看 / 认识 / 待复习），
 *   不再靠猜。
 * - **标记后自动跳下一个**：三连操作，不该每次再点一次「下一个」。
 * - **可回看已标记的**：万一手滑标错了，还能翻回去改。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowUUpLeft,
  Check,
  CircleDashed,
  Question,
  SpeakerHigh,
} from "@phosphor-icons/react";
import type { VocabWithState, WordMark } from "../../../types";
import { cx } from "../languageUi";
import { speak } from "../tts";
import { errorMessage } from "../../../utils";

export interface VocabularyStageProps {
  vocab: VocabWithState[];
  startIndex: number;
  onMark: (word: string, mark: WordMark) => Promise<void>;
  onProgressIndex: (index: number) => void;
  onFinish: () => void;
}

/** 队列里一个词的状态。 */
type Slot = { word: VocabWithState; mark: WordMark | null };

export function VocabularyStage({
  vocab,
  startIndex,
  onMark,
  onProgressIndex,
  onFinish,
}: VocabularyStageProps) {
  /** 本轮队列：已标「认识」的会被移出。 */
  const [queue, setQueue] = useState<Slot[]>(() =>
    vocab
      .filter((word) => word.mark !== "know")
      .map((word) => ({ word, mark: (word.mark as WordMark) ?? null })),
  );
  const [done, setDone] = useState<Slot[]>(() =>
    vocab
      .filter((word) => word.mark === "know")
      .map((word) => ({ word, mark: "know" as WordMark })),
  );
  const [cursor, setCursor] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showDone, setShowDone] = useState(false);

  const total = vocab.length;
  const current = queue[cursor] ?? null;
  const remaining = queue.length;
  const knownCount = done.length;
  const reviewCount = queue.filter((slot) => slot.mark !== null).length;

  useEffect(() => {
    onProgressIndex(cursor);
  }, [cursor, onProgressIndex]);

  const mark = useCallback(
    async (value: WordMark) => {
      const slot = queue[cursor];
      if (!slot || busy) return;
      setBusy(true);
      setError(null);
      try {
        await onMark(slot.word.word, value);
        // updater 必须是**纯函数**（React 严格模式会重复调用）：
        // 不能在 setQueue 的回调里再 setDone，否则一次点击会记两遍。
        const without = queue.filter((_, index) => index !== cursor);
        if (value === "know") {
          // 「认识」→ 直接消失（学会了就是学会了）
          setDone((all) => [...all, { ...slot, mark: value }]);
          setQueue(without);
          setCursor((previous) => Math.min(previous, Math.max(0, without.length - 1)));
        } else {
          // 「模糊 / 不认识」→ 挪到队尾，稍后再考（还没掌握，不该消失）
          setQueue([...without, { ...slot, mark: value }]);
          setCursor((previous) => Math.min(previous, Math.max(0, without.length - 1)));
        }
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setBusy(false);
      }
    },
    // remaining 变化不应重置回调语义，这里只依赖本词与队列。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [busy, cursor, onMark, queue, remaining],
  );

  /** 撤销最近一次标记（把词放回队列头部）。 */
  const undo = useCallback(() => {
    setQueue((previous) => {
      if (previous.length === 0) return previous;
      // 队尾最后一个被标记过的 = 最近处理的
      const lastIndex = previous.reduce(
        (found, slot, index) => (slot.mark !== null ? index : found),
        -1,
      );
      if (lastIndex === -1) return previous;
      const [restored] = previous.splice(lastIndex, 1);
      setCursor((position) => Math.max(0, position - 1));
      return [restored, ...previous];
    });
  }, []);

  const shown = showDone ? done : queue;
  const shownIndex = showDone ? Math.max(0, done.length - 1) : cursor;
  const card = shown[shownIndex] ?? shown[0] ?? null;

  if (total === 0) {
    return (
      <section className="en-stage">
        <p className="en-muted">这一课没有生词（可能整课都是已掌握的基础词）。</p>
        <button type="button" className="en-primary-btn" onClick={onFinish}>
          进入听力
        </button>
      </section>
    );
  }

  return (
    <section className="en-stage en-vocab-stage">
      <header className="en-stage-head">
        <div>
          <h2>Today's Words</h2>
          <p className="en-muted">
            本课 {total} 个生词 · 还剩 <strong>{remaining}</strong> 个
            {knownCount > 0 ? ` · 已掌握 ${knownCount} 个` : ""}
          </p>
        </div>
        <div className="en-row-actions">
          <button
            type="button"
            className="en-link-btn"
            onClick={undo}
            disabled={busy || queue.every((slot) => slot.mark === null)}
            title="撤销上一次标记"
          >
            <ArrowUUpLeft size={14} /> 撤销
          </button>
          <button
            type="button"
            className={cx("en-link-btn", showDone && "is-on")}
            onClick={() => setShowDone((previous) => !previous)}
          >
            {showDone ? "回到待学" : `已掌握 ${done.length}`}
          </button>
        </div>
      </header>

      {/* 进度条：一眼看出「还剩多少、哪些已经认识」 */}
      <div
        className="en-vocab-progress"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={knownCount}
        aria-label="预习进度"
      >
        {vocab.map((word) => {
          const state =
            word.mark === "know" ? "known" : word.mark ? "review" : "pending";
          return (
            <i key={word.word} className={`is-${state}`} title={word.word} />
          );
        })}
      </div>
      <p className="en-vocab-legend">
        <span className="is-pending" /> 未看
        <span className="is-known" /> 认识
        <span className="is-review" /> 待复习
      </p>

      {card ? (
        <>
          <div className="en-wordcard">
            <h3 className="en-wordcard-word">{card.word.word}</h3>
            {card.word.phonetic ? (
              <p className="en-phonetic">/{card.word.phonetic}/</p>
            ) : null}
            {card.word.pos ? <p className="en-pos">{card.word.pos}</p> : null}
            <p className="en-wordcard-zh">
              {card.word.translation_zh ?? "（词典未收录中文释义）"}
            </p>
            {card.word.context ? (
              <p className="en-wordcard-context">例：{card.word.context}</p>
            ) : null}
            {card.word.tags.length > 0 ? (
              <div className="en-chip-row">
                {card.word.tags.slice(0, 5).map((tag) => (
                  <span key={tag} className="en-chip is-plain">
                    {tag}
                  </span>
                ))}
              </div>
            ) : null}
            <button
              type="button"
              className="en-speak-btn"
              onClick={() => speak(card.word.word, "eng")}
              title="朗读"
            >
              <SpeakerHigh size={15} /> 听发音
            </button>
          </div>

          {error ? <p className="en-inline-error">{error}</p> : null}

          {showDone ? (
            <p className="en-muted">
              这里是你标为「认识」的词。点「回到待学」继续预习剩下的。
            </p>
          ) : (
            <div className="en-mark-actions">
              <button
                type="button"
                className="en-primary-btn"
                onClick={() => void mark("know")}
                disabled={busy}
                title="标记后从本轮消失"
              >
                <Check size={14} /> 认识
              </button>
              <button
                type="button"
                className="en-ghost-btn"
                onClick={() => void mark("fuzzy")}
                disabled={busy}
                title="放到队尾，稍后再考"
              >
                <CircleDashed size={14} /> 模糊
              </button>
              <button
                type="button"
                className="en-danger-btn"
                onClick={() => void mark("unknown")}
                disabled={busy}
                title="放到队尾，稍后再考"
              >
                <Question size={14} /> 不认识
              </button>
            </div>
          )}

          {!showDone ? (
            <div className="en-vocab-foot">
              <button
                type="button"
                className="en-link-btn"
                onClick={() => setCursor((previous) => Math.max(0, previous - 1))}
                disabled={cursor === 0 || busy}
              >
                上一个
              </button>
              <span className="en-muted">
                {cursor + 1}/{remaining}
                {reviewCount > 0 ? ` · ${reviewCount} 个待复习` : ""}
              </span>
              <button
                type="button"
                className="en-link-btn"
                onClick={() =>
                  setCursor((previous) => Math.min(remaining - 1, previous + 1))
                }
                disabled={cursor >= remaining - 1 || busy}
              >
                下一个
              </button>
              <button
                type="button"
                className="en-primary-btn"
                onClick={onFinish}
                disabled={busy}
              >
                {remaining === 0 ? "进入听力 →" : "跳过，进入听力 →"}
              </button>
            </div>
          ) : null}
        </>
      ) : (
        <>
          <div className="en-wordcard is-done">
            <p className="en-muted">
              {knownCount > 0
                ? `本课 ${total} 个生词都已过了一遍，其中 ${knownCount} 个你标了「认识」。`
                : "这一课的生词都过了一遍。"}
            </p>
          </div>
          <button
            type="button"
            className="en-primary-btn is-wide"
            onClick={onFinish}
            disabled={busy}
          >
            进入听力 →
          </button>
        </>
      )}
    </section>
  );
}
