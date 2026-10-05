/**
 * 句子挖掘面板（V13 W3）——把**读过的课文**变成**要主动回忆的复习卡**。
 *
 * 为什么放在课末而不是复习页：读完一课正是「这句话我刚看过」的瞬间，
 * 此时把它变成卡片，复习时才会觉得「这句我会」。
 *
 * 诚实边界：
 * - 先**预览**再入库（用户看得见会挖出什么、为什么挖这个词）；
 * - 卡片 id 内容派生 → 重复点击不会产生重复卡（按钮会说明这一点）；
 * - 挖不出来的句子（太短 / 没译文）直接不出现，不硬凑数量。
 */
import { useCallback, useEffect, useState } from "react";
import { Cards, Sparkle } from "@phosphor-icons/react";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { cx } from "../languageUi";
import type { MinedCard, MinedCardKind } from "../miningTypes";

export interface MiningPanelProps {
  lessonId: string;
  /** 每种卡的挖掘上限。 */
  maxPerKind?: number;
}

const KIND_LABELS: Record<MinedCardKind, string> = {
  cloze: "填空",
  dictation: "听写",
  translate: "中译英",
};

const REASON_LABELS: Record<MinedCard["reason"], string> = {
  lesson_vocab: "本课生词",
  function_word: "功能词（最容易说错）",
  content_word: "实词",
  fallback: "普通",
};

export function MiningPanel({ lessonId, maxPerKind = 6 }: MiningPanelProps) {
  const [cards, setCards] = useState<MinedCard[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  // 预览（只读，不写库）。
  useEffect(() => {
    let alive = true;
    setCards(null);
    setNote(null);
    englishClient
      .miningPreview(lessonId, maxPerKind)
      .then((items) => {
        if (alive) setCards(items);
      })
      .catch((cause: unknown) => {
        if (alive) setError(errorMessage(cause));
      });
    return () => {
      alive = false;
    };
  }, [lessonId, maxPerKind]);

  const add = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const report = await englishClient.miningAdd(lessonId, maxPerKind);
      setNote(
        report.cards > 0
          ? `已加入 ${report.cards} 张复习卡（重复点击不会重复添加）`
          : "本课没有可挖的句子（句子太短或没有译文）",
      );
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [lessonId, maxPerKind]);

  if (error) {
    return (
      <section className="en-mining">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-ghost-btn" onClick={() => void add()} disabled={busy}>
          重试
        </button>
      </section>
    );
  }

  if (cards === null) {
    return (
      <section className="en-mining">
        <p className="en-muted">正在看本课能挖出哪些句子…</p>
      </section>
    );
  }

  if (cards.length === 0) {
    return (
      <section className="en-mining">
        <p className="en-muted">
          本课暂时挖不出句子卡：句子太短或没有中文译文（不硬凑 —— 没内容就是没有）。
        </p>
      </section>
    );
  }

  return (
    <section className="en-mining">
      <header className="en-mining-head">
        <div>
          <h4>
            <Cards size={14} /> 把本课句子变成复习卡
          </h4>
          <p className="en-muted">
            共 {cards.length} 张 · 复习时先看句子再回忆（测「会用」，不是「认得」）
          </p>
        </div>
        <button type="button" className="en-primary-btn" onClick={() => void add()} disabled={busy}>
          <Sparkle size={14} /> {busy ? "处理中…" : "加入复习"}
        </button>
      </header>

      <ul className="en-mining-list">
        {cards.map((card) => (
          <li key={`${card.sequence}-${card.kind}`} className={cx("en-mining-item", `is-${card.kind}`)}>
            <span className="en-mining-kind">{KIND_LABELS[card.kind]}</span>
            <span className="en-mining-prompt">
              {card.kind === "cloze" ? (card.prompt ?? card.sentence) : null}
              {card.kind !== "cloze" ? (
                <em className="en-mining-sentence">{card.sentence}</em>
              ) : null}
            </span>
            <span className="en-mining-answer">→ {card.answer}</span>
            <span className="en-mining-reason">{REASON_LABELS[card.reason]}</span>
          </li>
        ))}
      </ul>

      {note ? <p className="en-muted">{note}</p> : null}
    </section>
  );
}
