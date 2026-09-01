import { invoke } from "@tauri-apps/api/core";
import { MagnifyingGlass } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  LanguageCode,
  LanguageItem,
  LanguageSearchHit,
} from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";

export type OpenDetail = (id: string | null, reset: boolean) => void;

interface ExplorePanelProps {
  language: LanguageCode;
  onOpen: OpenDetail;
  setNotice: (message: string) => void;
}

const EMPTY_RESULTS: LanguageSearchHit[] = [];

/** Explore：统一搜索（text/reading/romanization/meaning + 英语索引，#49），全部离线。 */
export function ExplorePanel({
  language,
  onOpen,
  setNotice,
}: ExplorePanelProps) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<LanguageSearchHit[]>(EMPTY_RESULTS);
  const [searched, setSearched] = useState(false);
  const [searching, setSearching] = useState(false);
  const searchSeq = useRef(0);

  const runSearch = useCallback(
    async (q: string) => {
      if (!isTauriRuntime()) return;
      const trimmed = q.trim();
      if (!trimmed) {
        setHits(EMPTY_RESULTS);
        setSearched(false);
        return;
      }
      const seq = ++searchSeq.current;
      setSearching(true);
      try {
        const result = await invoke<LanguageSearchHit[]>("language_search", {
          language,
          query: trimmed,
          limit: 40,
        });
        if (seq === searchSeq.current) {
          setHits(result);
          setSearched(true);
        }
      } catch (error) {
        if (seq === searchSeq.current) setNotice(errorMessage(error));
      } finally {
        if (seq === searchSeq.current) setSearching(false);
      }
    },
    [language, setNotice],
  );

  // 语言切换时清空
  useEffect(() => {
    setHits(EMPTY_RESULTS);
    setSearched(false);
  }, [language]);

  return (
    <div className="lang-panel">
      <div className="lang-search">
        <MagnifyingGlass size={17} />
        <input
          autoFocus={!isTauriRuntime()}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void runSearch(query);
          }}
          placeholder="搜索词汇 / 读音 / 罗马字 / 释义 — 如 taberu・たべる・旅行・sik6 faan6・food"
          aria-label="搜索语言词条"
        />
        <button
          onClick={() => void runSearch(query)}
          disabled={searching || !query.trim()}
        >
          搜索
        </button>
      </div>

      {searched && hits.length === 0 ? (
        <p className="lang-empty">
          没有找到「{query}」的结果（安装完整数据包后可获得更大词库）。
        </p>
      ) : null}

      {hits.length > 0 ? (
        <ul className="lang-hit-list">
          {hits.map((hit) => (
            <li key={hit.item.id}>
              <button
                className="lang-hit"
                onClick={() => onOpen(hit.item.id, false)}
              >
                <span className="lang-hit-text">{hit.item.text}</span>
                {hit.item.reading ? <i>{hit.item.reading}</i> : null}
                {hit.item.romanization ? <i>{hit.item.romanization}</i> : null}
                <small>
                  {matchLabel(hit.matched)} · {hit.item.item_type}
                </small>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function matchLabel(matched: string): string {
  switch (matched) {
    case "exact":
      return "精确";
    case "reading":
      return "读音";
    case "romanization":
      return "罗马字";
    case "meaning":
      return "释义";
    case "text-like":
      return "包含";
    case "english-index":
      return "英粤索引";
    default:
      return matched;
  }
}

export type { LanguageItem };
