import { invoke } from "@tauri-apps/api/core";
import { Play } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type { LanguageCode, SentenceRecord } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { speak } from "./tts";

interface ListenPanelProps {
  language: LanguageCode;
  setNotice: (message: string) => void;
}

/** Listen（#79 简版）：Tatoeba 例句 + TTS 播放 + 显示答案。离线核心可用。 */
export function ListenPanel({ language, setNotice }: ListenPanelProps) {
  const [sentences, setSentences] = useState<SentenceRecord[]>([]);
  const [shown, setShown] = useState<Set<string>>(new Set());
  const [loaded, setLoaded] = useState(false);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const rows = await invoke<SentenceRecord[]>("language_sentences", {
        language,
        limit: 20,
      });
      setSentences(rows);
      setLoaded(true);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [language, setNotice]);

  useEffect(() => {
    setShown(new Set());
    void reload();
  }, [reload, language]);

  const toggleShown = (id: string) => {
    setShown((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div className="lang-panel">
      <header className="lang-card-head">
        <h3>
          听力 <small>Listening · Tatoeba 例句 · {sentences.length} 句</small>
        </h3>
      </header>
      {loaded && sentences.length === 0 ? (
        <p className="lang-empty">
          该语言暂无例句。请先在 Language 页安装 Starter Pack，或用
          <code> language-data import sentences … </code>导入 Tatoeba。
        </p>
      ) : null}
      <ul className="lang-listen-list">
        {sentences.map((sentence) => {
          const visible = shown.has(sentence.sentence_id);
          return (
            <li key={sentence.sentence_id} className="lang-card">
              <div className="lang-listen-row">
                <button
                  className="lang-primary-icon"
                  onClick={() => void speak(sentence.text, sentence.language)}
                  title="播放"
                >
                  <Play size={15} />
                </button>
                <button
                  className="lang-sentence lang-listen-text"
                  onClick={() => toggleShown(sentence.sentence_id)}
                  title="显示例句"
                >
                  {visible ? (
                    <span>{sentence.text}</span>
                  ) : (
                    <span className="lang-blank">点击显示例句</span>
                  )}
                </button>
              </div>
              <small className="lang-license">
                {sentence.license} · Tatoeba #{sentence.sentence_id}
                {sentence.author ? ` · ${sentence.author}` : ""}
              </small>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
