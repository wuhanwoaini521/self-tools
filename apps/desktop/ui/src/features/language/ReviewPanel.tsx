import { invoke } from "@tauri-apps/api/core";
import { Check, Eye, Star, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type { LanguageCode, LearningStateKind, ReviewCard, ReviewOutcome, ReviewRating } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { speak } from "./tts";
import { stateLabel } from "./WordDetail";
import type { OpenDetail } from "./ExplorePanel";

interface ReviewPanelProps {
  language: LanguageCode;
  setNotice: (message: string) => void;
  onOpen: OpenDetail;
  onUpdated: () => void;
}

const RATINGS: { rating: ReviewRating; label: string }[] = [
  { rating: "again", label: "忘记" },
  { rating: "hard", label: "困难" },
  { rating: "good", label: "记得" },
  { rating: "easy", label: "轻松" },
];

/** Review：简单 SRS 卡片流（#60/#61）。到期复习优先，其次新词；完全离线。 */
export function ReviewPanel({ language, setNotice, onOpen, onUpdated }: ReviewPanelProps) {
  const [card, setCard] = useState<ReviewCard | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [outcome, setOutcome] = useState<ReviewOutcome | null>(null);
  const [empty, setEmpty] = useState(false);

  const loadNext = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setOutcome(null);
    setRevealed(false);
    try {
      const next = await invoke<ReviewCard | null>("language_review_next", { language });
      if (next) {
        setCard(next);
        setEmpty(false);
      } else {
        setCard(null);
        setEmpty(true);
      }
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [language, setNotice]);

  useEffect(() => { void loadNext(); }, [loadNext]);

  const rate = async (rating: ReviewRating) => {
    if (!card) return;
    try {
      const result = await invoke<ReviewOutcome>("language_review_rate", {
        request: { itemId: card.item.id, rating },
      });
      setOutcome(result);
      onUpdated();
      // 短延迟后进入下一张
      window.setTimeout(() => void loadNext(), 700);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const mark = (state: LearningStateKind) => {
    if (!card) return;
    void invoke<void>("language_set_state", { request: { itemId: card.item.id, state } })
      .catch((error) => setNotice(errorMessage(error)));
    onUpdated();
  };

  if (empty) {
    return <div className="lang-panel">
      <section className="lang-card">
        <h3>复习队列已清空</h3>
        <p className="lang-muted">
          本语言当前没有到期/新词卡片。去 Explore 搜索词条并查看详情，或在 Today 查看新词配额。
          （复习调度为本地 SRS，不联网。）
        </p>
        <button className="lang-primary" onClick={() => void loadNext()}>重新检查</button>
      </section>
    </div>;
  }

  if (!card) return <p className="lang-empty">正在准备卡片…</p>;

  return <div className="lang-panel lang-review">
    <section className="lang-card">
      <header className="lang-card-head">
        <h3>复习卡片 <small>{language.toUpperCase()} · {stateLabel(card.state)}</small></h3>
        <button className="lang-link" onClick={() => onOpen(card.item.id, false)}>查看详情</button>
      </header>

      <div className="lang-review-question">
        <p className="lang-review-word">{card.item.text}</p>
        {revealed
          ? <div>
              {card.item.reading ? <p className="lang-roman">{card.item.reading}</p> : null}
              {card.item.romanization ? <p className="lang-roman">{card.item.romanization}</p> : null}
              {outcome
                ? <p className="lang-muted">已评分：{outcome.interval_days >= 1 ? `${outcome.interval_days.toFixed(1)} 天后复习` : "今天再复习"}（ease {outcome.ease.toFixed(2)}）</p>
                : <div className="lang-state-actions lang-rate-actions">
                    {RATINGS.map((item) =>
                      <button key={item.rating} onClick={() => void rate(item.rating)}>
                        {item.rating === "again" ? <X size={14} /> : item.rating === "hard" ? <Star size={14} /> : item.rating === "good" ? <Check size={14} /> : <Check size={14} weight="bold" />}
                        {item.label}
                      </button>)}
                  </div>}
            </div>
          : <button className="lang-primary" onClick={() => { setRevealed(true); if (card.item.reading) void speak(card.item.reading, card.item.language); }}>
              <Eye size={16} />显示答案
            </button>}
      </div>

      <div className="lang-state-actions lang-mark-actions">
        <button onClick={() => onOpen(card.item.id, false)}>详情</button>
        <button onClick={() => mark("mastered")}>标记掌握</button>
      </div>
    </section>
  </div>;
}