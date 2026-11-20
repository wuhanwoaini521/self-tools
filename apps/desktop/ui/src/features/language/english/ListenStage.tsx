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
  onToggleEnglish: () => void;
  onToggleChinese: () => void;
  onSentence: (seq: number) => void;
}

export function ListenStage({
  sentences,
  showEnglish,
  showChinese,
  activeSeq,
  onToggleEnglish,
  onToggleChinese,
  onSentence,
}: ListenStageProps) {
  const [guess, setGuess] = useState("");
  const [revealedGuess, setRevealedGuess] = useState(false);

  const heardText = sentences.map((item) => item.english).join(" ");
  // 严格全等几乎不可能命中：按词重合度给「接近 / 差很多」的诚实反馈。
  const overlap = similarity(heardText, guess);

  return (
    <section className="en-stage en-listen-stage">
      <header className="en-stage-head">
        <div>
          <h2>Listening</h2>
          <p className="en-muted">
            {showEnglish
              ? showChinese
                ? "原文和译文都开着，可以再听一遍对照"
                : "只显示英文。译文仍藏着，回忆完再打开。"
              : "盲听：先听，再写，最后才看原文。"}
          </p>
        </div>
        <div className="en-row-actions">
          <button type="button" className="en-ghost-btn" onClick={onToggleEnglish}>
            {showEnglish ? <EyeSlash size={14} /> : <Eye size={14} />}
            {showEnglish ? "隐藏原文" : "显示原文"}
          </button>
          {showEnglish ? (
            <button type="button" className="en-ghost-btn" onClick={onToggleChinese}>
              {showChinese ? <EyeSlash size={14} /> : <Eye size={14} />}
              {showChinese ? "隐藏译文" : "显示译文"}
            </button>
          ) : null}
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
            {showEnglish && showChinese && sentence.chinese ? (
              <p className="en-sentence-zh">{sentence.chinese}</p>
            ) : null}
          </li>
        ))}
      </ol>

      {revealedGuess && showEnglish ? (
        <p className={cx("en-guess-result", overlap >= 0.85 ? "is-good" : "is-partial")}>
          {overlap >= 0.85
            ? "听得很准！"
            : overlap >= 0.5
              ? "大体听出来了——对照原文看还差什么。"
              : "差得比较多，建议逐句复读这一段。"}
        </p>
      ) : null}

      {!showEnglish && guess.trim().length > 0 ? (
        <button
          type="button"
          className="en-primary-btn"
          onClick={() => {
            setRevealedGuess(true);
            onToggleEnglish();
          }}
        >
          对照原文
        </button>
      ) : null}
    </section>
  );
}

/** 词重合度 0..1（忽略大小写、标点与多余空白）。 */
export function similarity(heard: string, guess: string): number {
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