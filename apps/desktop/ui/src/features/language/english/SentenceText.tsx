/**
 * 课文句子渲染：单词可点击查词（浮层，不跳页）。
 *
 * 难度着色克制（任务书 §15）：New / Learning / Known 只用**下划线样式**区分，
 * 不把课文染成五颜六色——阅读体验优先。
 */
import { Fragment, useMemo } from "react";
import type { VocabWithState } from "../../../types";
import { cx } from "../languageUi";

export interface SentenceTextProps {
  text: string;
  /** 课文生词表（用于标记已知/学习中）。 */
  vocab?: VocabWithState[];
  /** 点击单词查词；不传则纯文本渲染。 */
  onWordClick?: (word: string) => void;
  /** 句子 id（用于标记当前播放句）。 */
  sentenceId?: string;
  active?: boolean;
  className?: string;
}

const TOKEN_PATTERN = /([A-Za-z][A-Za-z'-]*)/g;

export function SentenceText({
  text,
  vocab,
  onWordClick,
  sentenceId,
  active = false,
  className,
}: SentenceTextProps) {
  const stateByWord = useMemo(() => {
    const map = new Map<string, string>();
    (vocab ?? []).forEach((item) => map.set(item.word, item.state));
    return map;
  }, [vocab]);

  if (!onWordClick) {
    return (
      <p className={cx("en-sentence", active && "is-active", className)} data-sentence={sentenceId}>
        {text}
      </p>
    );
  }

  const parts = text.split(TOKEN_PATTERN);
  return (
    <p
      className={cx("en-sentence", active && "is-active", className)}
      data-sentence={sentenceId}
    >
      {parts.map((part, index) => {
        // 带捕获组的 split：奇数位是单词，偶数位是标点/空白。
        if (index % 2 === 0) {
          return <Fragment key={index}>{part}</Fragment>;
        }
        const word = part.toLowerCase();
        const state = stateByWord.get(word);
        return (
          <button
            key={index}
            type="button"
            className={cx("en-word", state && `is-${state.replace("_", "-")}`)}
            onClick={(event) => {
              event.stopPropagation();
              onWordClick(part);
            }}
            title={state === "known" ? "已掌握" : state === "learning" ? "学习中" : "新词"}
          >
            {part}
          </button>
        );
      })}
    </p>
  );
}