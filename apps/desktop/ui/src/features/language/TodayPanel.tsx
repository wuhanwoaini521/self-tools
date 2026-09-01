import { invoke } from "@tauri-apps/api/core";
import {
  ArrowRight,
  CalendarCheck,
  Quotes,
  Sparkle,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type { LanguageCode, SentenceRecord, TodayView } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { speak } from "./tts";

interface TodayPanelProps {
  language: LanguageCode;
  onStart: () => void;
  onRefresh: () => void;
  setNotice: (message: string) => void;
}

/** Today（#61/#83）：今日学习计划 + Daily Expression，完全离线。 */
export function TodayPanel({
  language,
  onStart,
  onRefresh,
  setNotice,
}: TodayPanelProps) {
  const [today, setToday] = useState<TodayView | null>(null);
  const [daily, setDaily] = useState<SentenceRecord | null>(null);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const [view, sentences] = await Promise.all([
        invoke<TodayView>("language_today", { language }),
        invoke<SentenceRecord[]>("language_sentences", { language, limit: 8 }),
      ]);
      setToday(view);
      setDaily(sentences[0] ?? null);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [language, setNotice]);

  useEffect(() => {
    void reload();
  }, [reload, onRefresh]);

  if (!today) return <p className="lang-empty">加载中…</p>;
  const plan = today.plan;

  return (
    <div className="lang-panel">
      <section className="lang-today-hero">
        <div>
          <h2>
            今日学习 <small>Today · 15 min</small>
          </h2>
          <ul className="lang-today-stats">
            <li>{plan.due_reviews} 复习</li>
            <li>{plan.new_words} 新词</li>
            <li>{plan.sentences} 句子</li>
            <li>{plan.listening} Listening</li>
            <li>{plan.speaking} Speaking</li>
          </ul>
        </div>
        <button className="lang-primary" onClick={onStart}>
          <ArrowRight size={18} />
          Start Learning
        </button>
      </section>

      <section className="lang-card">
        <header className="lang-card-head">
          <h3>
            <Sparkle size={16} />
            Daily Expression
          </h3>
          <CalendarCheck size={15} />
        </header>
        {daily ? (
          <div className="lang-daily">
            <p className="lang-daily-text">{daily.text}</p>
            <small>
              {daily.license} · Tatoeba #{daily.sentence_id}
              {daily.author ? ` · ${daily.author}` : ""}
            </small>
            <div className="lang-row-actions">
              <button onClick={() => speak(daily.text, language)} title="播放">
                <PlayGlyph />
              </button>
              <button disabled title="学习">
                Learn
              </button>
              <button disabled title="口语">
                Speak
              </button>
              <button disabled title="收藏">
                Favorite
              </button>
            </div>
          </div>
        ) : (
          <p className="lang-empty">
            该语言暂无例句（安装数据包或导入 Tatoeba 后可用）。
          </p>
        )}
      </section>

      <section className="lang-card">
        <header className="lang-card-head">
          <h3>
            <Quotes size={16} />
            例句练习
          </h3>
        </header>
        <p className="lang-muted">
          在 Listen / Speak 标签页练习真实例句（Tatoeba，许可逐句标注）。
        </p>
      </section>
    </div>
  );
}

function PlayGlyph() {
  return (
    <svg width="15" height="15" viewBox="0 0 256 256" aria-hidden>
      <path
        fill="currentColor"
        d="M240 128a15.74 15.74 0 0 1-7.6 13.51L88.32 229.65a16 16 0 0 1-24.32-14v-167.3a16 16 0 0 1 24.32-14l144.08 88.14A15.74 15.74 0 0 1 240 128Z"
      />
    </svg>
  );
}
