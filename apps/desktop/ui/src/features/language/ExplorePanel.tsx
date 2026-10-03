/**
 * 搜索 / 探索。
 *
 * 输入防抖 250ms，按 `item_type` 分组展示 `search()` 的结果。
 * 语言选择器传 `null` 表示跨全部语言搜索。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { MagnifyingGlass } from "@phosphor-icons/react";
import type { LanguageCode, LanguageItemType, LanguageSearchHit } from "../../types";
import { languageClient } from "./languageClient";
import { Panel, PanelBody, Skeleton } from "./LanguagePrimitives";
import { EMPTY_COPY, ITEM_TYPE_LABELS, useAsyncPanel } from "./languageUi";

/** 搜索结果分组。`item_type` 是后端枚举，未知值归到「其它」。 */
const GROUP_ORDER: LanguageItemType[] = [
  "WORD",
  "PHRASE",
  "SENTENCE",
  "DIALOGUE",
  "PASSAGE",
  "GRAMMAR",
  "PRONUNCIATION",
];

const GROUP_LABELS: Record<LanguageItemType, string> = {
  WORD: "单词",
  PHRASE: "短语",
  SENTENCE: "句子",
  DIALOGUE: "对话",
  PASSAGE: "段落",
  GRAMMAR: "语法",
  PRONUNCIATION: "发音",
};

export interface ExplorePanelProps {
  language: LanguageCode | null;
  /** 由语言选择器变化触发搜索的依赖。 */
  searchNonce: number;
  onOpenItem: (itemId: string) => void;
  onAddToLesson: (itemId: string, content: string) => void;
}

export function ExplorePanel({
  language,
  searchNonce,
  onOpenItem,
  onAddToLesson,
}: ExplorePanelProps) {
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [limit, setLimit] = useState(40);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(query.trim()), 250);
    return () => window.clearTimeout(timer);
  }, [query]);

  const active = debounced.length > 0;
  const results = useAsyncPanel<LanguageSearchHit[]>(
    () => languageClient.search(language, debounced, limit),
    [debounced, language, limit, searchNonce],
    active,
  );

  const groups = useMemo(() => {
    const buckets = new Map<LanguageItemType, LanguageSearchHit[]>();
    for (const hit of results.data ?? []) {
      const type = hit.item.item_type;
      const bucket = buckets.get(type);
      if (bucket) bucket.push(hit);
      else buckets.set(type, [hit]);
    }
    const known = GROUP_ORDER.filter((type) => buckets.has(type)).map(
      (type) => [type, buckets.get(type) ?? []] as const,
    );
    const rest = [...buckets.entries()].filter(
      ([type]) => !GROUP_ORDER.includes(type),
    );
    return [...known, ...rest];
  }, [results.data]);

  const hitCount = results.data?.length ?? 0;

  return (
    <div className="lang-explore">
      <div className="lang-search">
        <MagnifyingGlass size={15} />
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="搜索单词、句子或短语…"
          type="search"
          aria-label="搜索语言词库"
        />
        <button
          type="button"
          onClick={() => setLimit((value) => (value >= 100 ? 20 : value + 20))}
          title="调整结果数量"
        >
          {limit} 条
        </button>
        {query ? (
          <button type="button" onClick={() => setQuery("")}>
            清空
          </button>
        ) : null}
      </div>

      <Panel
        title="搜索结果"
        hint={
          active && !results.loading
            ? `${hitCount} 条结果${language ? "" : " · 全部语言"}`
            : undefined
        }
      >
        {!active ? (
          <p className="lang-empty">{EMPTY_COPY.search}</p>
        ) : (
          <PanelBody
            loading={results.loading}
            error={results.error}
            reload={results.reload}
            empty={!results.loading && hitCount === 0 ? EMPTY_COPY.searchNone : null}
            skeletonRows={6}
          >
            <div className="lang-search-groups">
              {groups.map(([type, hits]) => (
                <section key={type}>
                  <h4>
                    {GROUP_LABELS[type] ?? type}
                    <small>{hits.length}</small>
                  </h4>
                  <ul className="lang-hit-list">
                    {hits.map((hit) => (
                      <li key={hit.item.id} className="lang-hit">
                        <button
                          type="button"
                          className="lang-hit-text"
                          onClick={() => onOpenItem(hit.item.id)}
                        >
                          <b>{hit.item.text}</b>
                          <i>{hit.matched}</i>
                          {hit.item.reading ? <small>{hit.item.reading}</small> : null}
                          {hit.item.romanization ? (
                            <small>{hit.item.romanization}</small>
                          ) : null}
                        </button>
                        <button
                          type="button"
                          className="lang-link"
                          onClick={() => onAddToLesson(hit.item.id, hit.item.text)}
                          title="加入课程草稿"
                        >
                          加入课程
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          </PanelBody>
        )}
      </Panel>
      {active && results.loading ? <Skeleton rows={2} /> : null}
    </div>
  );
}

/** 句库浏览（`sentences(language, limit)`）：搜索之外的第二种内容来源。 */
export function SentenceLibrary({
  language,
  limit = 30,
  onOpenSentence,
}: {
  language: LanguageCode;
  limit?: number;
  onOpenSentence: (sentenceId: string) => void;
}) {
  const panel = useAsyncPanel(
    () => languageClient.sentences(language, limit),
    [language, limit],
  );
  return (
    <PanelBody
      loading={panel.loading}
      error={panel.error}
      reload={panel.reload}
      empty={panel.data?.length === 0 ? EMPTY_COPY.sentences : null}
      skeletonRows={5}
    >
      <ul className="lang-hit-list">
        {(panel.data ?? []).map((record) => (
          <li key={record.sentence_id} className="lang-hit">
            <button
              type="button"
              className="lang-hit-text"
              onClick={() => onOpenSentence(record.sentence_id)}
            >
              <b>{record.text}</b>
              {record.author ? <small>{record.author}</small> : null}
              <small>{record.license}</small>
            </button>
          </li>
        ))}
      </ul>
    </PanelBody>
  );
}