/**
 * 朝代长卷：一屏看完 4000 年的朝代时空总览（不滚动、按真实年代等比）。
 *
 * - 横向 = 时间轴（前 2100 — 2010），纵向 = 分轨，重叠政权（宋辽金夏、十六国/东晋）各占一轨
 * - 背景色带 = 大时代（上古·夏商周 / 秦汉 / 隋唐五代 …），小带宽时只留色带不写字
 * - 条形宽度 = 在位年数；窄条（秦、新、五代十国）名称放到条外，上/下自动避让
 * - 有典故的时期在条尾带一个圆点；点任意一条切到该时期
 */
import { useEffect, useRef, useState } from "react";
import type { SemanticPeriod } from "./semanticTypes";
import { buildDynastyStrip } from "./dynastyStripLayout";
import { rangeText } from "./periodDetailHelpers";

const LANE_HEIGHT = 56;
const BAR_HEIGHT = 26;
const AXIS_HEIGHT = 28;
const ERA_HEAD = 20;

export function DynastyStrip({
  periods,
  currentId,
  storyCounts,
  eraGroups,
  onSelect,
}: {
  periods: SemanticPeriod[];
  currentId: string;
  storyCounts: Record<string, number>;
  eraGroups: { label: string; ids: string[] }[];
  onSelect: (period: SemanticPeriod) => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const node = hostRef.current;
    if (!node) return;
    const measure = () => setWidth(node.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const layout = buildDynastyStrip(periods, { width, eraGroups });
  const height = ERA_HEAD + layout.laneCount * LANE_HEIGHT + AXIS_HEIGHT;

  return (
    <div className="history-dynasty-strip">
      <div className="history-dynasty-strip-head">
        <span>朝代长卷</span>
        <small>横轴为真实年代（等比），同一纵轨为并立政权 · 点条切换时期</small>
      </div>
      <div
        ref={hostRef}
        className="history-dynasty-strip-canvas"
        style={{ height }}
        aria-label="朝代时空总览"
      >
        {layout.eras.map((era) => (
          <span
            key={`era-${era.label}`}
            className="history-dynasty-strip-era"
            style={{ left: `${era.left}%`, width: `${era.width}%` }}
          >
            {era.showLabel ? <i>{era.label}</i> : null}
          </span>
        ))}
        {layout.ticks.map((tick) => (
          <span
            key={`grid-${tick.year}`}
            className="history-dynasty-strip-grid"
            style={{ left: `${tick.left}%`, top: ERA_HEAD - 4, bottom: AXIS_HEIGHT }}
            aria-hidden="true"
          />
        ))}
        {layout.bars.map((bar) => {
          const current = bar.period.id === currentId;
          const stories = storyCounts[bar.period.id] ?? 0;
          const top = ERA_HEAD + bar.lane * LANE_HEIGHT + 20;
          const labelClass =
            bar.placement === "inside"
              ? "history-dynasty-strip-name"
              : `history-dynasty-strip-name is-outside is-${bar.placement}`;
          return (
            <button
              key={bar.period.id}
              type="button"
              className={`history-dynasty-strip-bar${current ? " is-current" : ""}${bar.openStart ? " is-open-start" : ""}`}
              style={{
                left: `${bar.left}%`,
                width: `${bar.width}%`,
                top,
                height: BAR_HEIGHT,
              }}
              onClick={() => onSelect(bar.period)}
              title={`${bar.period.name_zh_cn} ${rangeText(bar.period.start_year, bar.period.end_year)}${
                bar.period.event_count ? ` · ${bar.period.event_count} 事件` : ""
              }${stories ? ` · ${stories} 典故` : ""}`}
              aria-current={current ? "true" : undefined}
            >
              <span className={labelClass}>{bar.period.name_zh_cn}</span>
              {stories ? (
                <i className="history-dynasty-strip-dot" aria-hidden="true" />
              ) : null}
            </button>
          );
        })}
        {layout.ticks.map((tick) => (
          <span
            key={`label-${tick.year}`}
            className="history-dynasty-strip-tick"
            style={{
              left: `${tick.left}%`,
              top: ERA_HEAD + layout.laneCount * LANE_HEIGHT + 7,
            }}
          >
            {tick.label}
          </span>
        ))}
      </div>
    </div>
  );
}
