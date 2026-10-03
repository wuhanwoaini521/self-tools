/**
 * 听力阶段（Listening）：默认盲听，只播放音频。
 *
 * 「Show English / Show Translation」由学员自己控制——听不清先猜，
 * 再看答案，比直接看课文更接近真实听力训练。
 */
import { useState } from "react";
import { Eye, EyeSlash } from "@phosphor-icons/react";
import type { LessonSentence } from "../../../types";
import { cx } from "../languageUi";
import { formatTimestamp } from "./shared";

export interface ListenStageProps {
  sentences: LessonSentence[];
  showEnglish: boolean;
  showChinese: boolean;
  activeSeq: number | null;
  onToggleReveal: () => void;
  onSentence: (seq: number) => void;
}

export function ListenStage({
  sentences,
  showEnglish,
  showChinese,
  activeSeq,
  onToggleReveal,
  onSentence,
}: ListenStageProps) {
  const [guess, setGuess] = useState("");
  const [revealedGuess, setRevealedGuess] = useState(false);

  const heardText = showEnglish
    ? sentences.map((item) => item.english).join(" ")
    : "";
  // 严格全等几乎不可能命中：按词重合度给「接近 / 差很多」的诚实反馈。
  const overlap = similarity(heardText, guess);

  return (
    <section className="en-stage en-listen-stage">
      <header className="en-stage-head">
        <div>
          <h2>Listening</h2>
          <p className="en-muted">
            {showEnglish ? "已显示原文，再听一遍对照检查" : "盲听模式：只播放音频，不显示文字"}
          </p>
        </div>
        <div className="en-row-actions">
          <button type="button" className="en-ghost-btn" onClick={onToggleReveal}>
            {showEnglish ? <EyeSlash size={14} /> : <Eye size={14} />}
            {showEnglish ? "隐藏原文" : "显示原文"}
          </button>
        </div>
      </header>

      {/* 听写自测：先写下来再看 */}
      {!showEnglish ? (
        <div className="en-guess">
          <label htmlFor="en-guess-input">先写下你听到的（可选）</label>
          <textarea
            id="en-guess-input"
            value={guess}
            rows={3}
            placeholder="凭听力写下听到的句子…"
            onChange={(event) => setGuess(event.target.value)}
          />
          {revealedGuess ? (
            <p
              className={cx("en-guess-result", overlap >= 0.85 ? "is-good" : "is-partial")}
            >
              {showEnglish
                ? overlap >= 0.85
                  ? "听得很准！"
                  : overlap >= 0.5
                    ? "大体听出来了——对照原文看还差什么。"
                    : "差得比较多，建议逐句复读这一段。"
                : ""}
            </p>
          ) : null}
        </div>
      ) : null}

      <ol className="en-sentence-list">
        {sentences.map((sentence) => (
          <li
            key={sentence.id}
            className={cx(
              "en-sentence-row",
              activeSeq === sentence.sequence && "is-active",
            )}
          >
            <button
              type="button"
              className="en-sentence-time"
              onClick={() => onSentence(sentence.sequence)}
              title="从此句开始播放"
            >
              {formatTimestamp(sentence.start_ms)}
            </button>
            {showEnglish ? (
              <p className="en-sentence-en">{sentence.english}</p>
            ) : (
              <p className="en-sentence-hidden" aria-hidden="true">
                ████████ ██████ ████
              </p>
            )}
            {showChinese && sentence.chinese ? (
              <p className="en-sentence-zh">{sentence.chinese}</p>
            ) : null}
          </li>
        ))}
      </ol>

      {!showEnglish && guess.trim().length > 0 ? (
        <button
          type="button"
          className="en-primary-btn"
          onClick={() => setRevealedGuess(true)}
        >
          对照原文
        </button>
      ) : null}
    </section>
  );
}

/** 词重合度 0..1（忽略大小写、标点与多余空白）。 */
function similarity(heard: string, guess: string): number {
  const normalize = (value: string) =>
    value
      .toLowerCase()
      .replace(/[^a-z0-9\s']/g, " ")
      .split(/\s+/)
      .filter(Boolean);
  const heardWords = normalize(heard);
  const guessWords = new Set(normalize(guess));
  if (heardWords.length === 0) return 0;
  const matched = heardWords.filter((word) => guessWords.has(word)).length;
  return matched / heardWords.length;
}