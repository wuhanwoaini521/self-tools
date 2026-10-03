/**
 * 逐句精听（Sentence 阶段）。
 *
 * 当前播放句高亮并自动滚入视野；支持上一句 / 重复 / 下一句；
 * A-B 复读由工作台的 AudioPlayer 提供（repeat 打开后每句自动回到句首）。
 */
import { useEffect, useRef } from "react";
import { ArrowLeft, ArrowRight, Repeat } from "@phosphor-icons/react";
import type { LessonSentence, VocabWithState } from "../../../types";
import { cx } from "../languageUi";
import { SentenceText } from "./SentenceText";
import { formatTimestamp } from "./shared";

export interface SentenceStageProps {
  sentences: LessonSentence[];
  vocab: VocabWithState[];
  activeSeq: number | null;
  positionMs: number;
  repeat: boolean;
  onWordClick: (word: string, sentence: string) => void;
  onSelect: (seq: number) => void;
  onPrev: () => void;
  onNext: () => void;
  onFinish: () => void;
}

export function SentenceStage({
  sentences,
  vocab,
  activeSeq,
  positionMs,
  repeat,
  onWordClick,
  onSelect,
  onPrev,
  onNext,
  onFinish,
}: SentenceStageProps) {
  const listRef = useRef<HTMLOListElement | null>(null);
  const activeRowRef = useRef<HTMLLIElement | null>(null);

  // 没有显式选中时，用音频位置推导当前句（真正「跟音频走」）。
  const derivedSeq =
    activeSeq ??
    (() => {
      const found = sentences.find(
        (item) => positionMs >= item.start_ms && positionMs < item.end_ms,
      );
      return found?.sequence ?? null;
    })();

  // 当前句滚入视野（跟随音频，不打断阅读位置太远）。
  useEffect(() => {
    if (derivedSeq === null) return;
    const row = activeRowRef.current;
    const list = listRef.current;
    if (!row || !list) return;
    const rowTop = row.offsetTop;
    const viewTop = list.scrollTop;
    const viewBottom = viewTop + list.clientHeight;
    if (rowTop < viewTop + 24 || rowTop > viewBottom - 60) {
      list.scrollTo({ top: Math.max(0, rowTop - list.clientHeight / 3), behavior: "smooth" });
    }
  }, [derivedSeq]);

  return (
    <section className="en-stage en-sentence-stage">
      <header className="en-stage-head">
        <div>
          <h2>Sentence by Sentence</h2>
          <p className="en-muted">
            当前第 {derivedSeq !== null ? derivedSeq + 1 : "–"} / {sentences.length} 句
            {repeat ? " · 逐句复读已开启" : ""}
          </p>
        </div>
        <div className="en-row-actions">
          <button type="button" className="en-icon-btn" onClick={onPrev} title="上一句">
            <ArrowLeft size={15} />
          </button>
          <button
            type="button"
            className="en-icon-btn"
            onClick={() => derivedSeq !== null && onSelect(derivedSeq)}
            title="重听本句"
          >
            <Repeat size={15} />
          </button>
          <button type="button" className="en-icon-btn" onClick={onNext} title="下一句">
            <ArrowRight size={15} />
          </button>
        </div>
      </header>

      <ol className="en-sentence-list is-scrollable" ref={listRef}>
        {sentences.map((sentence) => {
          const active = derivedSeq === sentence.sequence;
          return (
            <li
              key={sentence.id}
              ref={active ? activeRowRef : undefined}
              className={cx("en-sentence-row", active && "is-active")}
              onClick={() => onSelect(sentence.sequence)}
            >
              <span className="en-sentence-time">{formatTimestamp(sentence.start_ms)}</span>
              <div>
                <SentenceText
                  text={sentence.english}
                  vocab={vocab}
                  onWordClick={(word) => onWordClick(word, sentence.english)}
                  sentenceId={sentence.id}
                  active={active}
                />
                {sentence.chinese ? (
                  <p className="en-sentence-zh">{sentence.chinese}</p>
                ) : null}
              </div>
            </li>
          );
        })}
      </ol>

      <button type="button" className="en-primary-btn" onClick={onFinish}>
        逐句听完了，进入跟读 →
      </button>
    </section>
  );
}