/**
 * 单条内容的「学」视图：单词 / 短语 / 句子 / 文章。
 *
 * 三个视图共用同一个外壳（`ItemStudy`），差别只在正文：
 * - 单词/短语：原文 + 读音 + 罗马音 + 释义 + 例句。
 * - 句子：原文 + 翻译 + **逐词拆解**（`SentenceChunk`）。有 `item_id` 的块
 *   可点开词典详情；没有 `item_id` 的块明确标注「词典未收录」，绝不编造释义。
 * - 文章：分段 + 每句可点，点开即进入句子学习视图。
 */
import { useCallback, useMemo, useState, type ReactNode } from "react";
import { ArrowRight, BookOpenText, Translate } from "@phosphor-icons/react";
import type {
  LanguageLearningItem,
  SentenceChunk,
  SentenceStudy,
} from "../../types";
import { splitSentences } from "../../utils";
import { speak } from "./tts";
import { Chip } from "./LanguagePrimitives";
import {
  DIFFICULTY_LABELS,
  EMPTY_COPY,
  ITEM_TYPE_LABELS,
  cx,
  useAsyncPanel,
} from "./languageUi";
import { languageClient } from "./languageClient";

/** 焦点模式下正在学的东西：要么一条统一条目，要么一个句子学习视图。 */
export type StudyTarget =
  | { kind: "item"; item: LanguageLearningItem }
  | { kind: "sentence"; sentence: SentenceStudy };

// ---------------------------------------------------------------- 逐词拆解

/**
 * 句子拆解。**诚实优先**：
 * - `item_id` 非空 → 可点，点了去 `learningItem` 拿详情。
 * - `item_id` 为空 → 词典没有这个词，标灰 + 「词典未收录」，不显示释义。
 * - `meaning` 为 null → 显示「释义未收录」，不拿 AI 猜测填充。
 */
function WordBreakdown({
  chunks,
  onOpenWord,
}: {
  chunks: readonly SentenceChunk[];
  onOpenWord?: (entityId: string) => void;
}) {
  if (chunks.length === 0) {
    return (
      <p className="lang-muted">
        这句话还没有生成逐词拆解（词典里缺少对齐数据）。
      </p>
    );
  }
  return (
    <ul className="lang-chunks">
      {chunks.map((chunk, index) => {
        const known = chunk.item_id !== null;
        return (
          <li
            key={`${chunk.text}-${index}`}
            className={cx("lang-chunk", known ? "is-known" : "is-missing")}
          >
            <button
              type="button"
              disabled={!known || !onOpenWord}
              onClick={() => {
                if (chunk.item_id) onOpenWord?.(chunk.item_id);
              }}
              title={
                known
                  ? onOpenWord
                    ? `查看「${chunk.text}」的词典详情`
                    : "词典详情在完整页面里可用"
                  : EMPTY_COPY.chunkMissing
              }
            >
              <b>{chunk.text}</b>
              {chunk.reading ? <em>{chunk.reading}</em> : null}
              <span>
                {chunk.meaning ?? (known ? "释义未收录" : EMPTY_COPY.chunkMissing)}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/**
 * 按 `StudyTarget` 分发到具体视图。焦点模式与课程播放器共用它，
 * 保证两条路径渲染出的内容完全一致。
 */
export function StudyBody({
  target,
  onOpenWord,
  onOpenSentence,
  onOpenDetail,
  onAskAi,
  hasAi,
}: {
  target: StudyTarget;
  onOpenWord?: (entityId: string) => void;
  onOpenSentence?: (text: string) => void;
  onOpenDetail?: () => void;
  onAskAi?: (prompt: string) => void;
  hasAi: boolean;
}) {
  if (target.kind === "sentence") {
    return (
      <SentenceView
        sentence={target.sentence}
        onOpenWord={onOpenWord}
        onAskAi={onAskAi}
        hasAi={hasAi}
      />
    );
  }
  if (target.item.type === "article") {
    return (
      <ArticleView item={target.item} onOpenSentence={onOpenSentence} />
    );
  }
  return <LexemeView item={target.item} onOpenDetail={onOpenDetail} />;
}

/** 词典没收录语法说明时，给一个「问 AI」的出口，而不是编一段语法。 */
function AiGrammarHint({
  available,
  onAsk,
}: {
  available: boolean;
  onAsk: (prompt: string) => void;
}) {
  if (available) {
    return (
      <button
        type="button"
        className="lang-link"
        onClick={() => onAsk("请讲解这个句子的语法结构与用法要点。")}
      >
        <Translate size={13} /> 让 AI 讲解这句话
      </button>
    );
  }
  return (
    <p className="lang-muted">
      词典未收录这句的语法说明 / usage。AI 增强当前不可用，上面的原文、翻译与拆解不受影响。
    </p>
  );
}

/** 朗读按钮。朗读失败（无 Web Speech）时静默返回 false，不打断学习。 */
function SpeakButton({ text, language }: { text: string; language: string }) {
  return (
    <button
      type="button"
      className="lang-link"
      onClick={() => void speak(text, language)}
      title="朗读"
    >
      朗读
    </button>
  );
}

// ---------------------------------------------------------------- 句子视图

export function SentenceView({
  sentence,
  onOpenWord,
  onNextSentence,
  onAskAi,
  hasAi,
}: {
  sentence: SentenceStudy;
  onOpenWord?: (entityId: string) => void;
  onNextSentence?: () => void;
  onAskAi?: (prompt: string) => void;
  hasAi: boolean;
}) {
  const ask = useCallback((prompt: string) => onAskAi?.(prompt), [onAskAi]);
  return (
    <div className="lang-study lang-study-sentence">
      <header className="lang-study-head">
        <p className="lang-study-original">{sentence.original}</p>
        {sentence.reading ? <p className="lang-reading">{sentence.reading}</p> : null}
        {sentence.romanization ? (
          <p className="lang-roman">{sentence.romanization}</p>
        ) : null}
        <p
          className={cx(
            "lang-study-translation",
            !sentence.translation && "is-empty",
          )}
        >
          {sentence.translation ?? "这句话还没有译文（词典未收录）"}
        </p>
        <div className="lang-row-actions">
          <SpeakButton text={sentence.original} language={sentence.language} />
          {onNextSentence ? (
            <button type="button" className="lang-link" onClick={onNextSentence}>
              下一句 <ArrowRight size={13} />
            </button>
          ) : null}
        </div>
      </header>

      <section>
        <h4>逐词拆解</h4>
        <WordBreakdown chunks={sentence.chunks} onOpenWord={onOpenWord} />
      </section>

      {sentence.key_words.length > 0 ? (
        <section>
          <h4>关键词</h4>
          <div className="lang-chip-row">
            {/* key_words 是词典条目 id（供跳转用），直接展示会看到
                "jmdict:1" 这种内部主键。这里用 chunks 里对应的词形呈现。 */}
            {sentence.key_words.map((id) => {
              const chunk = sentence.chunks.find((entry) => entry.item_id === id);
              return (
                <Chip key={id}>{chunk?.text ?? id}</Chip>
              );
            })}
          </div>
        </section>
      ) : null}

      <section>
        <h4>语法与用法</h4>
        {sentence.grammar ? (
          <p className="lang-study-block">{sentence.grammar}</p>
        ) : null}
        {sentence.usage ? (
          <p className="lang-study-block">{sentence.usage}</p>
        ) : null}
        {!sentence.grammar && !sentence.usage ? (
          <AiGrammarHint available={hasAi} onAsk={ask} />
        ) : null}
      </section>

      {sentence.author || sentence.license ? (
        <footer className="lang-license">
          {sentence.author ? `作者 ${sentence.author}` : null}
          {sentence.license ? ` · ${sentence.license}` : null}
        </footer>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------- 文章视图

/** 文章：按段落分组，每句可点开进入句子学习视图。 */
export function ArticleView({
  item,
  onOpenSentence,
}: {
  item: LanguageLearningItem;
  onOpenSentence?: (text: string) => void;
}) {
  const paragraphs = useMemo(
    () =>
      item.content
        .split(/\n{1,}/)
        .map((block) => block.trim())
        .filter((block) => block.length > 0),
    [item.content],
  );

  if (paragraphs.length === 0) {
    return <p className="lang-empty">这篇文章还没有正文内容。</p>;
  }

  const sentenceCount = paragraphs.reduce(
    (sum, paragraph) => sum + splitSentences(paragraph).length,
    0,
  );
  const title = item.content.split("\n")[0] ?? item.content;

  return (
    <div className="lang-study lang-study-article">
      <header className="lang-study-head">
        <h3 className="lang-article-title">{title}</h3>
        <p className="lang-muted">
          {paragraphs.length} 段 · {sentenceCount} 句 · 点任意一句可以单独学习
        </p>
        {item.translation ? (
          <p className="lang-study-translation">{item.translation}</p>
        ) : null}
      </header>
      {paragraphs.map((paragraph, index) => (
        <p key={index} className="lang-article-paragraph">
          {splitSentences(paragraph).map((sentence, sentenceIndex) => (
            <button
              key={`${index}-${sentenceIndex}`}
              type="button"
              className="lang-article-sentence"
              disabled={!onOpenSentence}
              onClick={() => onOpenSentence?.(sentence)}
              title={onOpenSentence ? "单独学习这句话" : undefined}
            >
              {sentence}{" "}
            </button>
          ))}
        </p>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------- 词/短语视图

export function LexemeView({
  item,
  onOpenDetail,
}: {
  item: LanguageLearningItem;
  onOpenDetail?: () => void;
}) {
  return (
    <div className="lang-study lang-study-word">
      <header className="lang-word-head">
        <h2>{item.content}</h2>
        {item.pronunciation ? (
          <span className="lang-reading">/{item.pronunciation}/</span>
        ) : null}
        {item.romanization ? <p className="lang-roman">{item.romanization}</p> : null}
        <p
          className={cx("lang-study-translation", !item.translation && "is-empty")}
        >
          {item.translation ?? "暂无译文"}
        </p>
        <div className="lang-chip-row">
          <Chip tone="accent">{ITEM_TYPE_LABELS[item.type]}</Chip>
          <Chip tone={item.difficulty === "hard" ? "danger" : "plain"}>
            {DIFFICULTY_LABELS[item.difficulty]}
          </Chip>
          {item.tags.map((tag) => (
            <Chip key={tag}>{tag}</Chip>
          ))}
        </div>
      </header>
      <div className="lang-row-actions">
        <SpeakButton text={item.content} language={item.language} />
        {onOpenDetail ? (
          <button type="button" className="lang-link" onClick={onOpenDetail}>
            <BookOpenText size={13} /> 词典详情
          </button>
        ) : null}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- 详情按需加载

/**
 * 焦点模式里按需拉取单条 `LanguageLearningItem`（拆解块点击时用）。
 * 关闭时把 id 置空，`useAsyncPanel` 的 `enabled` 会同时清空 loading/error。
 */
export function useWordLoader() {
  const [entityId, setEntityId] = useState<string | null>(null);
  const panel = useAsyncPanel<LanguageLearningItem | null>(
    () =>
      entityId ? languageClient.learningItem(entityId) : Promise.resolve(null),
    [entityId],
    entityId !== null,
  );
  const open = useCallback((id: string) => setEntityId(id), []);
  const close = useCallback(() => setEntityId(null), []);
  return { ...panel, open, close, entityId };
}

/** `SentenceStudy` 为空时（`sentenceStudy()` 返回 null）的诚实占位。 */
export function SentenceMissing({ reason }: { reason: ReactNode }) {
  return <p className="lang-empty">{reason}</p>;
}