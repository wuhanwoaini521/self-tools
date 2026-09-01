import { useId } from "react";
import type { HistoryEra } from "../historyEras";
import { compactYear } from "../labels";
import type { EraKeyEvent } from "../types/history";

/**
 * EraTimeline：「这个时代发生了什么？」
 * 水平时间轴：年份 → 事件。有 nodeId 的事件可点击进入 Detail；
 * 其余事件仅展示（不伪造内容），点击给出说明性提示。
 */
export function EraTimeline({
  era,
  onOpenNodeId,
  onInfo,
}: {
  era: HistoryEra;
  onOpenNodeId: (nodeId: string, title: string) => void;
  onInfo: (message: string) => void;
}) {
  const events = era.keyEvents ?? [];
  const titleId = useId();
  if (!events.length) return null;

  return (
    <section className="history-era-timeline" aria-labelledby={titleId}>
      <header className="history-section-head">
        <div>
          <span>TIMELINE</span>
          <h2 id={titleId}>这个时代发生了什么？</h2>
        </div>
        <p>{events.length} 个关键节点</p>
      </header>
      <div className="history-era-timeline-scroll">
        <ol className="history-era-timeline-track">
          {events.map((event) => (
            <EventNode
              key={`${event.year}-${event.title}`}
              event={event}
              onOpenNodeId={onOpenNodeId}
              onInfo={onInfo}
            />
          ))}
        </ol>
      </div>
    </section>
  );
}

function EventNode({
  event,
  onOpenNodeId,
  onInfo,
}: {
  event: EraKeyEvent;
  onOpenNodeId: (nodeId: string, title: string) => void;
  onInfo: (message: string) => void;
}) {
  const clickable = Boolean(event.nodeId);
  return (
    <li className={clickable ? "is-clickable" : "is-static"}>
      <span className="history-era-timeline-year">{compactYear(event.year)}</span>
      <i aria-hidden="true" />
      <button
        type="button"
        disabled={!clickable}
        aria-label={
          clickable
            ? `打开事件「${event.title}」详情`
            : `${event.title}（暂无独立资料）`
        }
        title={event.note}
        onClick={() => {
          if (clickable && event.nodeId) onOpenNodeId(event.nodeId, event.title);
          else onInfo(`「${event.title}」的独立资料正在整理中，可以从相关人物与事件继续探索。`);
        }}
      >
        <b>{event.title}</b>
        <span>{event.note}</span>
      </button>
    </li>
  );
}