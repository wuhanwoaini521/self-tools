import {
  ArrowRight,
  BookmarkSimple,
  CaretLeft,
  MapPin,
} from "@phosphor-icons/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { HistoryDetailView, HistoryNode } from "../../../types";
import { isTauriRuntime } from "../../../utils";
import { historyEras, type HistoryEra } from "../historyEras";
import { KIND_LABELS, rangeText } from "../labels";
import { HistoryRelationGraph } from "../components/HistoryRelationGraph";

function eraForNode(node: HistoryNode): HistoryEra | null {
  return (
    historyEras.find(
      (era) => era.id === node.id || era.id === node.period_id,
    ) ?? null
  );
}

/**
 * 详情通用外壳：返回按钮、标题头（类型 · 年份）、收藏、「在地图中查看」、
 * 所属时代横条、正文 children、关系图、资料来源。
 */
export function DetailShell({
  node,
  view,
  favorite,
  onBack,
  onToggleFavorite,
  onOpenNode,
  onOpenEra,
  onMap,
  children,
}: {
  node: HistoryNode;
  view: HistoryDetailView;
  favorite: boolean;
  onBack: () => void;
  onToggleFavorite: () => void;
  onOpenNode: (node: HistoryNode) => void;
  onOpenEra: (era: HistoryEra) => void;
  onMap?: () => void;
  children: React.ReactNode;
}) {
  const era = eraForNode(node);
  return (
    <article className="history-detail">
      <button type="button" className="history-back" onClick={onBack}>
        <CaretLeft size={16} /> 返回探索
      </button>

      <header className="history-detail-head">
        <div>
          <small>
            {KIND_LABELS[node.kind]} ·{" "}
            {rangeText(node.start_year, node.end_year)}
          </small>
          <h1>{node.title}</h1>
          <p>{node.summary}</p>
        </div>
        <div className="history-detail-actions">
          {onMap ? (
            <button type="button" className="history-map-link" onClick={onMap}>
              <MapPin size={16} /> 在地图中查看
            </button>
          ) : null}
          <button
            type="button"
            className={`history-save${favorite ? " saved" : ""}`}
            onClick={onToggleFavorite}
          >
            <BookmarkSimple size={18} weight={favorite ? "fill" : "regular"} />
            {favorite ? "已收藏" : "收藏"}
          </button>
        </div>
      </header>

      {era ? (
        <aside className="history-era-context">
          <div>
            <small>所属时代</small>
            <b>{era.name}</b>
            <span>{era.yearLabel}</span>
          </div>
          <p>{era.description}</p>
          <button
            type="button"
            className="history-era-context-link"
            onClick={() => onOpenEra(era)}
          >
            了解{era.name} <ArrowRight size={14} />
          </button>
          <div className="history-era-context-tags">
            {era.tags.map((tag) => (
              <span key={tag}>{tag}</span>
            ))}
          </div>
        </aside>
      ) : null}

      {children}

      <HistoryRelationGraph
        center={node}
        relations={view.relations}
        onOpenNode={onOpenNode}
      />

      {view.sources.length ? (
        <section className="history-detail-section">
          <h2>资料来源</h2>
          <div className="history-sources">
            {view.sources.map((source) => (
              <button
                type="button"
                key={source.id}
                onClick={() => {
                  if (isTauriRuntime()) void openUrl(source.url);
                }}
              >
                <b>{source.title}</b>
                <span>
                  {source.authority} · {source.source_type}
                </span>
                <ArrowRight size={14} />
              </button>
            ))}
          </div>
        </section>
      ) : null}
    </article>
  );
}
