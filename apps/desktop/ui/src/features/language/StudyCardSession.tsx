/**
 * 学习卡片会话（进 Language 的默认视图）。
 *
 * 一张卡片 = 一件今天该学的东西：
 * - **单词**（大号）
 * - **读音**：多个注音方案（KANA / 罗马音 / IPA / 拼音…），各自可朗读
 * - **解释**：词性与释义，逐条
 * - **例句**：每句都可单独朗读（这是「读音要单词和例句都有」）
 *
 * 底部两个选择：**不会** / **会** —— 直接决定下一张，并写回平台
 * （会 → Study；不会 → 记错题 + Again，进复习队列下次找你）。
 *
 * 队列来自 `studyQueue()`：先到期复习，再补新内容，所以刚装完数据进来就有卡片。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowRight,
  CheckCircle,
  SpeakerHigh,
  ThumbsDown,
  XCircle,
} from "@phosphor-icons/react";
import type {
  LanguageCode,
  LanguageLearningItem,
  StudyCard,
  WordDetail,
} from "../../types";
import { errorMessage } from "../../utils";
import { languageClient } from "./languageClient";
import { speak } from "./tts";
import { Action, Chip, Panel, PanelBody, Skeleton } from "./LanguagePrimitives";
import {
  DIFFICULTY_LABELS,
  ITEM_TYPE_LABELS,
  EMPTY_COPY,
  useAsyncPanel,
} from "./languageUi";

/** 注音方案的中文名（`PronunciationScheme` 的小写形式）。 */
const SCHEME_LABELS: Record<string, string> = {
  kana: "假名",
  romaji: "罗马音",
  ipa: "音标",
  pinyin: "拼音",
  jyutping: "粤拼",
  arpabet: "音标",
};

/** 朗读按钮：词、注音、例句共用。 */
function Speak({
  text,
  language,
  label = "朗读",
}: {
  text: string;
  language: LanguageCode;
  label?: string;
}) {
  return (
    <button
      type="button"
      className="lang-speak"
      title={`朗读：${text}`}
      onClick={() => void speak(text, language)}
    >
      <SpeakerHigh size={14} /> {label}
    </button>
  );
}

/**
 * 单张卡片。词条详情按需加载——只有真正显示这一张时才去取例句与全部释义。
 */
function Card({
  card,
  onKnow,
  onAgain,
  busy,
  onOpenDetail,
  onAskAi,
}: {
  card: StudyCard;
  onKnow: () => void;
  onAgain: () => void;
  busy: boolean;
  onOpenDetail?: () => void;
  onAskAi?: (prompt: string) => void;
}) {
  const item = card.item;
  const detail = useAsyncPanel<WordDetail | null>(
    () => languageClient.item(item.id),
    [item.id],
  );
  const word = detail.data?.item;
  const language = word?.language ?? item.language;
  // 详情还没回来时先用适配层给的字段兜底，来了再覆盖。
  const meanings = detail.data?.meanings ?? [];
  const pronunciations = detail.data?.pronunciations ?? [];
  const examples = detail.data?.examples ?? [];
  const sentences = detail.data?.sentences ?? [];

  return (
    <div className="lang-study lang-study-card">
      {/* ---------- 单词 + 读音 ---------- */}
      <header className="lang-card-word">
        <div className="lang-card-word-main">
          <h2>{item.content}</h2>
          <div className="lang-card-chip-row">
            <Chip tone="accent">{ITEM_TYPE_LABELS[item.type]}</Chip>
            <Chip tone={item.difficulty === "hard" ? "danger" : "plain"}>
              {DIFFICULTY_LABELS[item.difficulty]}
            </Chip>
            {card.from_review ? <Chip tone="warn">复习</Chip> : <Chip>新学</Chip>}
          </div>
        </div>
        <Speak text={item.content} language={language} label="朗读单词" />
      </header>

      {/* 多个注音方案，各自可朗读 */}
      {detail.loading ? (
        <Skeleton rows={1} />
      ) : pronunciations.length > 0 ? (
        <section className="lang-card-block">
          <h3>读音</h3>
          <ul className="lang-pron-list">
            {pronunciations.map((pron) => (
              <li key={pron.id}>
                <span className="lang-pron-scheme">
                  {SCHEME_LABELS[pron.scheme.toLowerCase()] ?? pron.scheme}
                </span>
                <b>{pron.phonemes}</b>
                <Speak text={item.content} language={language} label="朗读" />
              </li>
            ))}
          </ul>
        </section>
      ) : item.pronunciation ? (
        <section className="lang-card-block">
          <h3>读音</h3>
          <ul className="lang-pron-list">
            <li>
              <b>{item.pronunciation}</b>
              <Speak text={item.content} language={language} label="朗读" />
            </li>
          </ul>
        </section>
      ) : null}

      {/* ---------- 解释（词性 + 释义） ---------- */}
      <section className="lang-card-block">
        <h3>解释</h3>
        {detail.loading ? (
          <Skeleton rows={2} />
        ) : meanings.length > 0 ? (
          <ul className="lang-meaning-list">
            {meanings.map((meaning) => (
              <li key={meaning.id}>
                {meaning.pos ? <i>{meaning.pos}</i> : null}
                <span>{meaning.gloss ?? meaning.raw ?? "（无释义）"}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="lang-study-translation is-empty">
            {item.translation ?? "词典未收录释义"}
          </p>
        )}
      </section>

      {/* ---------- 例句（每句都能朗读） ---------- */}
      {(examples.length > 0 || sentences.length > 0) && (
        <section className="lang-card-block">
          <h3>例句</h3>
          <ul className="lang-example-list">
            {examples.map((example, index) => (
              <li key={`${example.text}-${index}`}>
                <div className="lang-example-line">
                  <span className="lang-example-text">{example.text}</span>
                  <Speak text={example.text} language={language} label="朗读例句" />
                </div>
                {example.translation ? (
                  <small className="lang-example-translation">
                    {example.translation}
                  </small>
                ) : null}
              </li>
            ))}
            {sentences.map((record) => (
              <li key={record.sentence_id}>
                <div className="lang-example-line">
                  <span className="lang-example-text">{record.text}</span>
                  <Speak text={record.text} language={language} label="朗读例句" />
                </div>
                <small className="lang-example-source">
                  {record.author ?? record.source} · {record.license}
                </small>
              </li>
            ))}
          </ul>
        </section>
      )}

      {/* ---------- 会不会 ---------- */}
      <footer className="lang-card-actions">
        <Action variant="ghost" onClick={onAgain} disabled={busy} title="记入错题，之后再练">
          <ThumbsDown size={15} /> 不会
        </Action>
        <Action onClick={onKnow} disabled={busy} title="记住了，进入下一张">
          <CheckCircle size={15} /> 会了
        </Action>
      </footer>

      <div className="lang-row-actions">
        {onOpenDetail ? (
          <button type="button" className="lang-link" onClick={onOpenDetail}>
            词典详情 <ArrowRight size={13} />
          </button>
        ) : null}
        {onAskAi ? (
          <button
            type="button"
            className="lang-link"
            onClick={() => onAskAi(`请讲解「${item.content}」的意思、读音和用法。`)}
          >
            问 AI
          </button>
        ) : null}
      </div>
    </div>
  );
}

/**
 * 学习会话。`onFinish` 回到首页 / 课程时调用。
 */
export function StudyCardSession({
  language,
  onOpenDetail,
  onFinish,
  onAskAi,
}: {
  language: LanguageCode;
  onOpenDetail?: (itemId: string) => void;
  onFinish?: () => void;
  onAskAi?: (prompt: string) => void;
}) {
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(0);
  const [wrong, setWrong] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const queue = useAsyncPanel<StudyCard[]>(
    () => languageClient.studyQueue(language, 20),
    [language],
  );

  /**
   * 队列换了就是新的一轮，进度归零。
   *
   * 此前只在「换语言」时重置，于是打完一轮（`index` 停在队尾）后点「再来一轮」，
   * 如果新队列更短（例如只剩 1 张到期复习），`index` 会越过新队长度，
   * 界面永远停在「本轮完成」——用户再也学不下去。按队列**引用**变化判断，
   * 换语言、点「再来一轮」、首次加载都会重置。
   */
  const lastQueue = useRef<StudyCard[] | null>(null);
  useEffect(() => {
    if (queue.data === null || queue.data === lastQueue.current) return;
    lastQueue.current = queue.data;
    setIndex(0);
    setDone(0);
    setWrong([]);
    setError(null);
  }, [queue.data]);

  const cards = queue.data ?? [];

  const card = cards[index] ?? null;
  const finished = queue.data !== null && index >= cards.length;

  const advance = useCallback(() => {
    setIndex((current) => {
      const next = current + 1;
      if (next >= cards.length) onFinish?.();
      return next;
    });
  }, [cards.length, onFinish]);

  /** 会了：记一次学习，进入下一张。 */
  const know = useCallback(async () => {
    if (!card || busy) return;
    setBusy(true);
    setError(null);
    try {
      await languageClient.recordStudy(card.item.id, "study");
      setDone((count) => count + 1);
      advance();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [card, busy, advance]);

  /** 不会：加入复习队列（后端会记错题），进入下一张。 */
  const again = useCallback(async () => {
    if (!card || busy) return;
    setBusy(true);
    setError(null);
    try {
      await languageClient.recordStudy(card.item.id, "view");
      await languageClient.addToReview(card.item.id);
      setWrong((list) => [...list, card.item.content]);
      advance();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [card, busy, advance]);

  const summary = useMemo(
    () => `本轮 ${done} 会 · ${wrong.length} 不会`,
    [done, wrong.length],
  );

  return (
    <div className="lang-study-session">
      <header className="lang-session-head">
        <div>
          <span className="lang-muted">今天学什么</span>
          <h2>
            {queue.loading
              ? "正在准备卡片…"
              : card
                ? `${index + 1} / ${cards.length}`
                : finished
                  ? "本轮完成"
                  : "—"}
          </h2>
        </div>
        {cards.length > 0 ? (
          <div className="lang-session-progress">
            <span className="lang-track">
              <i
                style={{
                  width: `${cards.length ? ((index + 1) / cards.length) * 100 : 0}%`,
                }}
              />
            </span>
            <span className="lang-muted">{summary}</span>
          </div>
        ) : null}
        {onFinish ? (
          <button type="button" className="lang-link" onClick={onFinish}>
            结束本轮
          </button>
        ) : null}
      </header>

      {error ? (
        <p className="lang-inline-error-text">
          {error}
          <button type="button" className="lang-link" onClick={() => void know()}>
            重试
          </button>
        </p>
      ) : null}

      {queue.loading ? (
        <Panel title="正在准备卡片…">
          <PanelBody loading error={null} reload={queue.reload} empty={null} skeletonRows={4}>
            <span />
          </PanelBody>
        </Panel>
      ) : queue.error ? (
        <Panel title="卡片加载失败">
          <PanelBody
            loading={false}
            error={queue.error}
            reload={queue.reload}
            empty={null}
          >
            <span />
          </PanelBody>
        </Panel>
      ) : finished ? (
        <Panel title="本轮完成">
          <PanelBody loading={false} error={null} reload={queue.reload} empty={null}>
            <div className="lang-empty">
              {done > 0 || wrong.length > 0 ? (
                <>
                  <p>
                    这一轮过了 {done} 个{wrong.length > 0 ? `，${wrong.length} 个记进错题本` : ""}。
                  </p>
                  <p>不会的已进复习队列，到时间会再找你。</p>
                </>
              ) : (
                <p>{EMPTY_COPY.review}</p>
              )}
              <button type="button" className="lang-link" onClick={queue.reload}>
                再来一轮
              </button>
            </div>
          </PanelBody>
        </Panel>
      ) : card ? (
        <Card
          card={card}
          busy={busy}
          onKnow={() => void know()}
          onAgain={() => void again()}
          onOpenDetail={onOpenDetail ? () => onOpenDetail(card.item.id) : undefined}
          onAskAi={onAskAi}
        />
      ) : (
        <Panel title="暂时没有可学的卡片">
          <PanelBody loading={false} error={null} reload={queue.reload} empty={null}>
            <div className="lang-empty">
              <XCircle size={16} />
              <p>词典里还没有这门语言的词。先在「词库来源」装一个数据包。</p>
            </div>
          </PanelBody>
        </Panel>
      )}
    </div>
  );
}

/** 供上层直接取当前卡片（无需时用不到）。 */
export type { LanguageLearningItem };
