/**
 * 课前单词预习（Vocabulary 阶段）。
 *
 * 词表来自**本课课文真实内容**（导入时由词典 + 词形还原提取），不是预置词单。
 * 三态自评（认识 / 模糊 / 不认识）立即进入平台 SRS，学员无需二次操作。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Check, CircleDashed, Question, SpeakerHigh } from "@phosphor-icons/react";
import type { VocabWithState, WordMark } from "../../../types";
import { cx } from "../languageUi";
import { errorMessage } from "../../../utils";
import { speak } from "../tts";

export interface VocabularyStageProps {
  vocab: VocabWithState[];
  startIndex: number;
  onMark: (word: string, mark: WordMark) => Promise<void>;
  onProgressIndex: (index: number) => void;
  onFinish: () => void;
}

export function VocabularyStage({
  vocab,
  startIndex,
  onMark,
  onProgressIndex,
  onFinish,
}: VocabularyStageProps) {
  const words = useMemo(
    () => (vocab.length > 0 ? vocab : []),
    [vocab],
  );
  const [index, setIndex] = useState(Math.min(Math.max(0, startIndex), Math.max(0, words.length - 1)));
  const [marks, setMarks] = useState<Record<string, WordMark>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 词表按「未标记优先」排序，标记过的沉到后面，学员一屏内一直有新词。
  const ordered = useMemo(() => {
    return [...words].sort((left, right) => {
      const leftMarked = marks[left.word] ? 1 : 0;
      const rightMarked = marks[right.word] ? 1 : 0;
      if (leftMarked !== rightMarked) return leftMarked - rightMarked;
      return left.importance - right.importance;
    });
  }, [marks, words]);

  useEffect(() => {
    onProgressIndex(index);
  }, [index, onProgressIndex]);

  const current = ordered[index];
  const knownCount = Object.keys(marks).length;

  const mark = useCallback(
    async (value: WordMark) => {
      if (!current) return;
      setBusy(true);
      setError(null);
      try {
        await onMark(current.word, value);
        setMarks((previous) => ({ ...previous, [current.word]: value }));
        setIndex((previous) => Math.min(previous + 1, Math.max(0, ordered.length - 1)));
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setBusy(false);
      }
    },
    [current, onMark, ordered.length],
  );

  if (!current) {
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
            本课 {words.length} 个生词 · 已标记 {knownCount} 个
          </p>
        </div>
        <div className="en-vocab-dots" aria-hidden="true">
          {ordered.map((word, wordIndex) => (
            <span
              key={word.word}
              className={cx(
                "en-vocab-dot",
                wordIndex === index && "is-current",
                marks[word.word] && `is-${marks[word.word]}`,
              )}
            />
          ))}
        </div>
      </header>

      <div className="en-wordcard">
        <h3 className="en-wordcard-word">{current.word}</h3>
        {current.phonetic ? <p className="en-phonetic">/{current.phonetic}/</p> : null}
        {current.pos ? <p className="en-pos">{current.pos}</p> : null}
        <p className="en-wordcard-zh">
          {current.translation_zh ?? "（词典未收录中文释义）"}
        </p>
        {current.context ? (
          <p className="en-wordcard-context">
            例：{current.context}
          </p>
        ) : null}
        {current.tags.length > 0 ? (
          <div className="en-chip-row">
            {current.tags.slice(0, 5).map((tag) => (
              <span key={tag} className="en-chip is-plain">
                {tag}
              </span>
            ))}
          </div>
        ) : null}
        <button
          type="button"
          className="en-speak-btn"
          onClick={() => speak(current.word, "eng")}
          title="朗读"
        >
          <SpeakerHigh size={15} /> 听发音
        </button>
      </div>

      {error ? <p className="en-inline-error">{error}</p> : null}

      <div className="en-mark-actions">
        <button
          type="button"
          className="en-primary-btn"
          onClick={() => void mark("know")}
          disabled={busy}
        >
          <Check size={14} /> 认识
        </button>
        <button
          type="button"
          className="en-ghost-btn"
          onClick={() => void mark("fuzzy")}
          disabled={busy}
        >
          <CircleDashed size={14} /> 模糊
        </button>
        <button
          type="button"
          className="en-danger-btn"
          onClick={() => void mark("unknown")}
          disabled={busy}
        >
          <Question size={14} /> 不认识
        </button>
      </div>

      <div className="en-vocab-foot">
        <button
          type="button"
          className="en-link-btn"
          onClick={() => setIndex((previous) => Math.max(0, previous - 1))}
          disabled={index === 0}
        >
          上一个
        </button>
        <span className="en-muted">
          {index + 1}/{ordered.length}
        </span>
        <button
          type="button"
          className="en-link-btn"
          onClick={() => setIndex((previous) => Math.min(ordered.length - 1, previous + 1))}
          disabled={index >= ordered.length - 1}
        >
          下一个
        </button>
        <button type="button" className="en-primary-btn" onClick={onFinish}>
          {knownCount >= ordered.length * 0.6 ? "进入听力 →" : "跳过，进入听力 →"}
        </button>
      </div>
    </section>
  );
}

