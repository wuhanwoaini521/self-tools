/**
 * 精读阶段（Reading）：完整课文 + 可选翻译 + 生词标记。
 *
 * 克制原则（任务书 §15）：单词难度只用**下划线样式**区分
 * （known 实线 / learning 虚线 / new 点线 + 极淡底色），不把课文染色。
 */
import type { LessonSentence, VocabWithState } from "../../../types";
import { cx } from "../languageUi";
import { SentenceText } from "./SentenceText";
import { formatTimestamp } from "./shared";

export interface ReadingStageProps {
  sentences: LessonSentence[];
  vocab: VocabWithState[];
  showTranslation: boolean;
  showVocabHighlight: boolean;
  onToggleTranslation: () => void;
  onToggleVocab: () => void;
  onWordClick: (word: string, sentence: string) => void;
  activeSeq: number | null;
  onSentence: (seq: number) => void;
}

export function ReadingStage({
  sentences,
  vocab,
  showTranslation,
  showVocabHighlight,
  onToggleTranslation,
  onToggleVocab,
  onWordClick,
  activeSeq,
  onSentence,
}: ReadingStageProps) {
  return (
    <section className="en-stage en-read-stage">
      <header className="en-stage-head">
        <div>
          <h2>Reading</h2>
          <p className="en-muted">点击任意单词可查释义，不必离开本页</p>
        </div>
        <div className="en-row-actions">
          <button type="button" className="en-ghost-btn" onClick={onToggleVocab}>
            生词标记：{showVocabHighlight ? "开" : "关"}
          </button>
          <button type="button" className="en-ghost-btn" onClick={onToggleTranslation}>
            译文：{showTranslation ? "开" : "关"}
          </button>
        </div>
      </header>

      {showVocabHighlight ? (
        <p className="en-vocab-legend">
          <span className="en-word is-new is-legend">新词</span>
          <span className="en-word is-learning is-legend">学习中</span>
          <span className="en-word is-known is-legend">已掌握</span>
        </p>
      ) : null}

      <article className={cx("en-text", !showVocabHighlight && "no-highlight")}>
        {sentences.map((sentence) => (
          <div
            key={sentence.id}
            className={cx("en-text-line", activeSeq === sentence.sequence && "is-active")}
          >
            <button
              type="button"
              className="en-sentence-time"
              onClick={() => onSentence(sentence.sequence)}
              title="播放这一句"
            >
              {formatTimestamp(sentence.start_ms)}
            </button>
            <div className="en-text-body">
              <SentenceText
                text={sentence.english}
                vocab={showVocabHighlight ? vocab : undefined}
                onWordClick={(word) => onWordClick(word, sentence.english)}
                sentenceId={sentence.id}
                active={activeSeq === sentence.sequence}
              />
              {showTranslation && sentence.chinese ? (
                <p className="en-sentence-zh">{sentence.chinese}</p>
              ) : null}
            </div>
          </div>
        ))}
      </article>
    </section>
  );
}