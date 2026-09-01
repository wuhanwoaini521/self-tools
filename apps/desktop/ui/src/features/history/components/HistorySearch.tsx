import { ArrowRight, MagnifyingGlass, X } from "@phosphor-icons/react";
import type { HistoryNode } from "../../../types";
import { historyEras } from "../historyEras";
import type { HistorySearch as SearchState } from "../hooks/useHistorySearch";
import { KIND_LABELS } from "../labels";

/**
 * History Search：Command Search。
 * 输入 → 结果按类型分组（人物 / 事件 / 朝代 / 地点 / 战争 / 制度 / 文物 / 文化），
 * 支持 ↑ ↓ 键在结果间移动、Enter 打开、Esc 清除；查询命中时代时提供「在时间轴上定位」。
 */
export function HistorySearch({
  search,
  onOpenNode,
  onLocateEra,
}: {
  search: SearchState;
  onOpenNode: (node: HistoryNode) => void;
  onLocateEra: (index: number) => void;
}) {
  const {
    query,
    setQuery,
    groups,
    searching,
    submit,
    clear,
    activeIndex,
    setActiveIndex,
    flattenedItems,
  } = search;

  const trimmed = query.trim();
  const matchedEraIndex = trimmed
    ? (() => {
        const exact = historyEras.findIndex((era) => era.name === trimmed);
        if (exact >= 0) return exact;
        const contains = historyEras.find(
          (era) => trimmed.includes(era.name) || era.id.includes(trimmed),
        );
        return contains ? historyEras.indexOf(contains) : -1;
      })()
    : -1;

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex(
        flattenedItems.length ? (activeIndex + 1) % flattenedItems.length : -1,
      );
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex(
        flattenedItems.length
          ? (activeIndex - 1 + flattenedItems.length) % flattenedItems.length
          : -1,
      );
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (activeIndex >= 0 && flattenedItems[activeIndex]) {
        onOpenNode(flattenedItems[activeIndex]);
      } else {
        void submit();
      }
    } else if (event.key === "Escape") {
      clear();
    }
  };

  return (
    <form
      className="history-search"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
      onKeyDown={handleKeyDown}
      role="search"
    >
      <MagnifyingGlass size={19} aria-hidden="true" />
      <input
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          if (!event.target.value.trim()) clear();
        }}
        placeholder="搜索人物、事件、年代、地点、战争、制度……"
        aria-label="搜索历史人物、事件、年代、地点等"
        disabled={searching}
      />
      {query ? (
        <button
          type="button"
          className="history-search-clear"
          onClick={clear}
          aria-label="清除搜索"
        >
          <X size={15} />
        </button>
      ) : null}
      <button
        type="submit"
        className="history-search-go"
        disabled={searching || !trimmed}
      >
        {searching ? "搜索中…" : "搜索"}
      </button>

      {groups.length || (matchedEraIndex >= 0 && trimmed) ? (
        <div
          className="history-search-panel"
          role="listbox"
          aria-label="搜索结果"
        >
          {matchedEraIndex >= 0 && trimmed ? (
            <button
              type="button"
              className="history-search-locate"
              onClick={() => onLocateEra(matchedEraIndex)}
            >
              <span>在时间轴上定位</span>
              <b>{historyEras[matchedEraIndex].name}</b>
              <small>{historyEras[matchedEraIndex].yearLabel}</small>
              <ArrowRight size={14} aria-hidden="true" />
            </button>
          ) : null}
          {groups.map((group) => (
            <section className="history-search-group" key={group.kind}>
              <h3>
                {KIND_LABELS[group.kind]}
                <span>{group.items.length}</span>
              </h3>
              <ul>
                {group.items.map((node) => {
                  const flatIndex = flattenedItems.indexOf(node);
                  return (
                    <li key={node.id}>
                      <button
                        type="button"
                        className={flatIndex === activeIndex ? "is-active" : ""}
                        onClick={() => onOpenNode(node)}
                        onMouseMove={() => setActiveIndex(flatIndex)}
                        role="option"
                        aria-selected={flatIndex === activeIndex}
                      >
                        <b>{node.title}</b>
                        <small>{node.summary}</small>
                        <span className="history-search-kind">
                          {KIND_LABELS[node.kind]}
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
          {!groups.length && trimmed && !searching ? (
            <p className="history-search-empty">
              没有匹配资料，换个关键词试试。
            </p>
          ) : null}
        </div>
      ) : null}
    </form>
  );
}
