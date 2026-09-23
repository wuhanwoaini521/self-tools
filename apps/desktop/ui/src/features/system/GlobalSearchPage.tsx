/**
 * Global Search（V11 §119-§122）：跨模块统一检索。
 *
 * 铁律：
 * - 不依赖 LLM（§121）：本次检索零模型调用，即使 AI 不可用也能工作；
 * - 结果带 action_target，前端直接导航到来源模块；
 * - 单源失败只降级该源（degraded_sources），不影响其它结果。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { MagnifyingGlass } from "@phosphor-icons/react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, isTauriRuntime } from "../../utils";

export type SearchSourceName =
  | "history"
  | "travel"
  | "geography"
  | "language"
  | "memory"
  | "documents"
  | "files"
  | "study_board"
  | "applications";

export interface GlobalSearchHitDto {
  source: SearchSourceName;
  kind: string;
  title: string;
  snippet: string;
  action_target: Record<string, unknown>;
  score: number;
}

export interface GlobalSearchResultDto {
  query: string;
  hits: GlobalSearchHitDto[];
  degraded_sources: SearchSourceName[];
  total: number;
}

const SOURCE_LABELS: Record<string, string> = {
  history: "History",
  travel: "Travel",
  geography: "Geography",
  language: "Language",
  memory: "Memory",
  documents: "Documents",
  files: "Files",
  study_board: "Study Board",
  applications: "Applications",
};

export interface GlobalSearchPageProps {
  active: boolean;
  /** 打开来源模块（action_target 的 module 字段）。 */
  onNavigate: (module: string, target: Record<string, unknown>) => void;
}

export function GlobalSearchPage({ active, onNavigate }: GlobalSearchPageProps) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<GlobalSearchResultDto | null>(null);
  const [searching, setSearching] = useState(false);
  const [notice, setNotice] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (active) inputRef.current?.focus();
  }, [active]);

  const run = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) {
      setResult(null);
      setNotice("");
      return;
    }
    setSearching(true);
    setNotice("");
    try {
      // 真实后端命令（V11-O：零 LLM，AI 未配置也能搜）。
      const response = await invoke<GlobalSearchResultDto>("global_search", {
        query: trimmed,
        limit: 5,
      });
      setResult(response);
    } catch (error) {
      setResult(null);
      setNotice(`搜索失败：${errorMessage(error)}`);
    } finally {
      setSearching(false);
    }
  }, []);

  return (
    <div className="page-scroll search-page">
      <header className="search-head">
        <h1>Global Search</h1>
        <p>不需要知道信息在哪个模块；一次搜索覆盖学习、知识与家庭服务。</p>
      </header>
      <div className="search-box">
        <MagnifyingGlass size={18} />
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void run(query);
          }}
          placeholder="搜索历史、旅行、地理、语言、记忆、文档、文件、学习板、应用…"
          aria-label="全局搜索"
        />
        <button type="button" onClick={() => void run(query)} disabled={searching || !query.trim()}>
          {searching ? "搜索中" : "搜索"}
        </button>
      </div>

      {result ? (
        <section className="search-results" aria-label="搜索结果">
          <div className="search-meta">
            共 {result.total} 条
            {result.degraded_sources.length > 0
              ? ` · ${result.degraded_sources.length} 个源暂不可用`
              : ""}
          </div>
          {result.total === 0 ? (
            <p className="search-empty">
              没有命中。若显示多个源不可用，说明对应索引尚未在本地建立或后端检索命令尚未接入。
            </p>
          ) : (
            <ul>
              {result.hits.map((hit, index) => (
                <li key={`${hit.source}-${index}`}>
                  <button
                    type="button"
                    onClick={() =>
                      onNavigate(
                        String(hit.action_target.module ?? hit.source),
                        hit.action_target,
                      )
                    }
                  >
                    <span className="search-hit-source">{SOURCE_LABELS[hit.source] ?? hit.source}</span>
                    <span className="search-hit-title">{hit.title}</span>
                    <span className="search-hit-snippet">{hit.snippet}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}
    </div>
  );
}
