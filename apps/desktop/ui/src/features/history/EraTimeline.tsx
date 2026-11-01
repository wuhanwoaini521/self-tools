/**
 * EraTimeline —— 按年代跨度等比例分段的「时代带」。
 *
 * 设计参考：
 * - ybogdanov/history-timeline：横向比例时间带 + 年份刻度 + 当前项高亮；
 * - Chronas：时代（period）是浏览的主轴，可随时切换；
 * - TimelineJS：时间线本身是可操作的对象，而不是一张静态图。
 *
 * 全部数据来自 `history_semantic_home` 的 periods（真实 start_year / end_year /
 * date_precision），不做任何推断或补值：
 * - 公元前用负数年份表示，直接参与比例计算，刻度显示为「前2070」；
 * - `date_precision = approximate` 的时期用虚线边框标注（约略年代）；
 * - 时间范围重叠的时期（如宋辽金并存）自动分轨，绝不重叠成一团；
 * - 开端年代缺失（如「上古」）用「年代待考」标签 + 虚线左边框表示，不伪造起点。
 *
 * 比例与可点性的取舍：先按真实跨度计算左偏移，再给过窄的时期补到最小宽度；
 * 补宽后可能与相邻时期重叠，交给分轨算法处理，因此年份位置始终是准确的。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { ArrowLeft, ArrowRight } from "@phosphor-icons/react";
import type { SemanticPeriod } from "./semanticTypes";
import { yearText } from "./periodDetailHelpers";
import "./era-timeline.css";

const TICK_STEP = 500;
const DESKTOP_MIN_SEGMENT = 32;
const COMPACT_MIN_SEGMENT = 46;
const SEGMENT_GAP = 0;
const DESKTOP_LANE_HEIGHT = 36;
const COMPACT_LANE_HEIGHT = 40;
const DESKTOP_AXIS_HEIGHT = 54;
const COMPACT_AXIS_HEIGHT = 46;
const DESKTOP_DENSITY_HEIGHT = 30;
const COMPACT_DENSITY_HEIGHT = 34;
const DESKTOP_MIN_TRACK = 640;
const COMPACT_MIN_TRACK = 880;
const COMPACT_QUERY = "(max-width: 1100px)";
const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

export interface EraSegment {
  period: SemanticPeriod;
  /** 距轨道左侧的像素偏移（按真实年份比例）。 */
  left: number;
  /** 渲染宽度：真实比例宽度与最小可点宽度取大。 */
  width: number;
  /** 分轨结果（0 起），用于展示时间重叠的时期。 */
  lane: number;
  /** 起始年份缺失，宽度只用于可点，不代表真实跨度。 */
  openStart: boolean;
  /** 边界为约略年代（date_precision = approximate）。 */
  approximate: boolean;
}

export interface EraTick {
  year: number;
  left: number;
  label: string;
}

export interface EraTimelineLayout {
  trackWidth: number;
  bandHeight: number;
  axisHeight: number;
  laneCount: number;
  laneHeight: number;
  segments: EraSegment[];
  ticks: EraTick[];
  /** 0 年（公元元年）在轨道上的位置；域内没有 0 时为 null。 */
  epochLeft: number | null;
  /** 时间轴域（含两端留白），用于刻度与零线。 */
  domain: { from: number; to: number } | null;
}

function isYear(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** 密度条高度：按事件数归一化到可用高度；无数据返回 0（不画柱子）。 */
export function eraDensityHeight(
  count: number | null | undefined,
  max: number,
  height: number,
): number {
  if (!count || count <= 0 || max <= 0 || height <= 0) return 0;
  return Math.max(6, Math.round((count / max) * height));
}

/** 约略年代：区间边界不是精确断代。 */
export function isApproximatePeriod(period: SemanticPeriod): boolean {
  return period.date_precision === "approximate";
}

/** 时期年份文案：缺失端显示「年代待考」，绝不猜测。 */
export function periodRangeLabel(period: SemanticPeriod): string {
  const start = isYear(period.start_year) ? period.start_year : null;
  const end = isYear(period.end_year) ? period.end_year : null;
  if (start === null && end === null) return "年代待考";
  if (start === null) return `年代待考 — ${yearText(end)}`;
  if (end === null) return `${yearText(start)} — 年代待考`;
  if (start === end) return yearText(start);
  return `${yearText(start)} — ${yearText(end)}`;
}

/**
 * 纯布局计算：输入真实时期与轨道宽度，输出分段/分轨/刻度。
 * 无 DOM 依赖，便于在单元测试里固定公元前、最小宽度与并发时期的行为。
 */
export function buildEraTimelineLayout(
  periods: SemanticPeriod[],
  trackWidth: number,
  compact: boolean,
): EraTimelineLayout {
  const laneHeight = compact ? COMPACT_LANE_HEIGHT : DESKTOP_LANE_HEIGHT;
  const axisHeight = compact ? COMPACT_AXIS_HEIGHT : DESKTOP_AXIS_HEIGHT;
  const minSegment = compact
    ? COMPACT_MIN_SEGMENT
    : DESKTOP_MIN_SEGMENT;
  const safeTrack = Math.max(120, Math.round(trackWidth));
  // 预留一个最小段宽，保证「最右侧时期 + 最小宽度」不会顶出轨道。
  const layoutWidth = Math.max(1, safeTrack - minSegment);

  const ranged = periods.filter(
    (period) => isYear(period.start_year) && isYear(period.end_year),
  );
  if (!ranged.length) {
    return {
      trackWidth: safeTrack,
      bandHeight: 0,
      axisHeight,
      laneCount: 0,
      laneHeight,
      segments: [],
      ticks: [],
      epochLeft: null,
      domain: null,
    };
  }

  let minYear = Infinity;
  let maxYear = -Infinity;
  for (const period of ranged) {
    minYear = Math.min(minYear, period.start_year as number);
    maxYear = Math.max(maxYear, period.end_year as number);
  }
  const rawSpan = Math.max(1, maxYear - minYear);
  const pad = rawSpan * 0.015;
  const from = minYear - pad;
  const to = maxYear + pad;
  const span = to - from;
  const position = (year: number) =>
    Math.min(layoutWidth, Math.max(0, ((year - from) / span) * layoutWidth));

  const segments: EraSegment[] = [];
  for (const period of periods) {
    const approximate = isApproximatePeriod(period);
    if (isYear(period.start_year) && isYear(period.end_year)) {
      const left = position(period.start_year);
      const width = Math.max(
        position(period.end_year) - left,
        minSegment,
      );
      segments.push({
        period,
        left,
        width,
        lane: 0,
        openStart: false,
        approximate,
      });
      continue;
    }
    // 只有结束年份：以结束点对齐，向左占一个最小段宽，并标为开放起点。
    if (isYear(period.end_year)) {
      const left = Math.max(0, position(period.end_year) - minSegment);
      segments.push({
        period,
        left,
        width: minSegment,
        lane: 0,
        openStart: true,
        approximate,
      });
      continue;
    }
    // 只有起始年份：向右占一个最小段宽。
    if (isYear(period.start_year)) {
      const left = position(period.start_year);
      segments.push({
        period,
        left,
        width: minSegment,
        lane: 0,
        openStart: true,
        approximate,
      });
    }
    // 两端都缺失的时期不进入比例带，由调用方以列表形式兜底展示。
  }

  segments.sort(
    (left, right) => left.left - right.left || left.width - right.width,
  );

  // 首次适配分轨：补宽后的片段不允许互相压住，因此需要足够多的轨道。
  const laneEnds: number[] = [];
  for (const segment of segments) {
    let lane = laneEnds.findIndex(
      (end) => segment.left >= end + SEGMENT_GAP,
    );
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(0);
    }
    laneEnds[lane] = segment.left + segment.width;
    segment.lane = lane;
  }

  const ticks: EraTick[] = [];
  const firstTick = Math.ceil(from / TICK_STEP) * TICK_STEP;
  for (let year = firstTick; year <= to; year += TICK_STEP) {
    const label =
      year === 0 ? "公元" : year < 0 ? `前${Math.abs(year)}` : `${year}`;
    ticks.push({ year, left: position(year), label });
  }

  return {
    trackWidth: safeTrack,
    bandHeight: laneEnds.length * laneHeight + 4,
    axisHeight,
    laneCount: laneEnds.length,
    laneHeight,
    segments,
    ticks,
    epochLeft: from <= 0 && to >= 0 ? position(0) : null,
    domain: { from, to },
  };
}

interface EraTimelineProps {
  periods: SemanticPeriod[];
  currentId: string;
  onSelect: (period: SemanticPeriod) => void;
}

export function EraTimeline({
  periods,
  currentId,
  onSelect,
}: EraTimelineProps) {
  const compact = useMediaQuery(COMPACT_QUERY);
  const reduceMotion = useMediaQuery(REDUCED_MOTION_QUERY);
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const segmentRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
  const [measuredWidth, setMeasuredWidth] = useState(0);

  useEffect(() => {
    const node = scrollerRef.current;
    if (!node) return;
    const update = () => setMeasuredWidth(node.clientWidth);
    update();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", update);
      return () => window.removeEventListener("resize", update);
    }
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const trackWidth = Math.max(
    measuredWidth,
    compact ? COMPACT_MIN_TRACK : DESKTOP_MIN_TRACK,
  );
  const layout = useMemo(
    () => buildEraTimelineLayout(periods, trackWidth, compact),
    [periods, trackWidth, compact],
  );
  const scrollable = measuredWidth > 0 && layout.trackWidth > measuredWidth + 1;
  const densityHeight = compact ? COMPACT_DENSITY_HEIGHT : DESKTOP_DENSITY_HEIGHT;
  const maxEventCount = useMemo(
    () => periods.reduce((max, period) => Math.max(max, period.event_count ?? 0), 0),
    [periods],
  );
  const hasDensity = maxEventCount > 0;

  // 手机上默认只显示前 2 条并行轨道，避免空轨把时间轴撑得过高；当前时期在隐藏
  // 轨道里时自动展开，切回可见时期后仍保持用户的选择。
  const [showAllLanes, setShowAllLanes] = useState(false);
  const currentLane =
    layout.segments.find((segment) => segment.period.id === currentId)?.lane ?? 0;
  const effectiveShowAll = showAllLanes || (compact && currentLane >= 2);
  const visibleLaneCount =
    compact && !effectiveShowAll
      ? Math.min(layout.laneCount, 2)
      : layout.laneCount;
  const bandHeight = visibleLaneCount * layout.laneHeight + 4;
  const visibleSegments = layout.segments.filter(
    (segment) => segment.lane < visibleLaneCount,
  );

  // 当前时期自动进入视野；只滚动横向容器，不劫持页面纵向滚动。
  useEffect(() => {
    const scroller = scrollerRef.current;
    const node = segmentRefs.current.get(currentId);
    if (!scroller || !node) return;
    if (scroller.scrollWidth <= scroller.clientWidth + 1) return;
    const scrollerRect = scroller.getBoundingClientRect();
    const nodeRect = node.getBoundingClientRect();
    const fullyVisible =
      nodeRect.left >= scrollerRect.left + 8 &&
      nodeRect.right <= scrollerRect.right - 8;
    if (fullyVisible) return;
    const target =
      nodeRect.left - scrollerRect.left + scroller.scrollLeft -
      scroller.clientWidth / 2 + nodeRect.width / 2;
    const maxLeft = scroller.scrollWidth - scroller.clientWidth;
    scroller.scrollTo({
      left: Math.max(0, Math.min(target, maxLeft)),
      behavior: reduceMotion ? "auto" : "smooth",
    });
  }, [currentId, layout.trackWidth, measuredWidth, reduceMotion]);

  if (!periods.length) return null;

  const hasGeometry = layout.segments.length > 0;
  const approximateCount = layout.segments.filter(
    (segment) => segment.approximate,
  ).length;
  const current =
    periods.find((period) => period.id === currentId) ?? null;
  const currentApproximate = current ? isApproximatePeriod(current) : false;

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const keys = ["ArrowLeft", "ArrowRight", "Home", "End"];
    if (!keys.includes(event.key)) return;
    const buttons = Array.from(
      scrollerRef.current?.querySelectorAll<HTMLButtonElement>(
        ".history-era-segment",
      ) ?? [],
    );
    if (!buttons.length) return;
    const active = document.activeElement;
    const index = buttons.findIndex((button) => button === active);
    let nextIndex = index;
    if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = buttons.length - 1;
    else if (index === -1) nextIndex = 0;
    else
      nextIndex =
        (index + (event.key === "ArrowLeft" ? -1 : 1) + buttons.length) %
        buttons.length;
    buttons[nextIndex]?.focus();
    event.preventDefault();
  };

  return (
    <section
      className="history-era-timeline"
      aria-labelledby="history-era-timeline-title"
    >
      <div className="history-v2-section-label history-era-timeline-label">
        <span
          id="history-era-timeline-title"
          className="history-v2-section-title"
        >
          时代时间轴
        </span>
        <small>
          {hasGeometry
            ? `${periods.length} 个时期 · 按年代跨度等比例 · 重叠时期自动分轨${
                approximateCount ? " · 虚线为约略年代" : ""
              }${hasDensity ? " · 顶部条高=事件数量" : ""}`
            : `${periods.length} 个时期 · 年代数据待补充，仍可按时序浏览`}
        </small>
      </div>
      <div
        className={`history-era-card${scrollable ? " is-scrollable" : ""}`}
      >
        {hasGeometry ? (
          <div className="history-era-viewport">
          <div className="history-era-scroll" ref={scrollerRef}>
            <div
              className="history-era-track"
              style={{
                width: `${layout.trackWidth}px`,
                height: `${
                  (hasDensity ? densityHeight : 0) +
                  bandHeight +
                  layout.axisHeight
                }px`,
              }}
              onKeyDown={handleKeyDown}
            >
              {hasDensity ? (
                <div
                  className="history-era-density"
                  style={{ height: `${densityHeight}px` }}
                  aria-hidden="true"
                >
                  {visibleSegments.map((segment) => {
                    const barHeight = eraDensityHeight(
                      segment.period.event_count,
                      maxEventCount,
                      densityHeight - 6,
                    );
                    if (!barHeight) return null;
                    const currentBar = segment.period.id === currentId;
                    return (
                      <button
                        key={`density-${segment.period.id}`}
                        type="button"
                        tabIndex={-1}
                        className={`history-era-density-bar${
                          currentBar ? " is-current" : ""
                        }`}
                        style={{
                          left: `${segment.left}px`,
                          width: `${Math.max(2, segment.width)}px`,
                          height: `${barHeight}px`,
                        }}
                        onClick={() => onSelect(segment.period)}
                        title={`${segment.period.name_zh_cn} · ${segment.period.event_count ?? 0} 个事件`}
                      />
                    );
                  })}
                </div>
              ) : null}
              {layout.epochLeft !== null ? (
                <span
                  className="history-era-epoch-line"
                  style={{
                    left: `${layout.epochLeft}px`,
                    top: `${hasDensity ? densityHeight : 0}px`,
                    height: `${bandHeight}px`,
                  }}
                  aria-hidden="true"
                />
              ) : null}
              <div
                className="history-era-band"
                style={{ height: `${bandHeight}px` }}
              >
                <div className="history-era-lane-guides" aria-hidden="true">
                  {Array.from({ length: visibleLaneCount }, (_, lane) => (
                    <span
                      key={lane}
                      style={{ height: `${layout.laneHeight}px` }}
                    />
                  ))}
                </div>
                {visibleSegments.map((segment) => {
                  const currentSegment = segment.period.id === currentId;
                  const showYears = segment.width >= 78;
                  const rangeLabel = segment.openStart
                    ? `年代待考 — ${yearText(segment.period.end_year)}`
                    : `${yearText(segment.period.start_year)} — ${yearText(
                        segment.period.end_year,
                      )}`;
                  const label = `${segment.period.name_zh_cn}，${rangeLabel}${
                    segment.approximate ? "，约略年代" : ""
                  }${segment.openStart ? "，开端年代待考" : ""}`;
                  return (
                    <button
                      key={segment.period.id}
                      type="button"
                      ref={(node) => {
                        if (node) segmentRefs.current.set(segment.period.id, node);
                        else segmentRefs.current.delete(segment.period.id);
                      }}
                      className={`history-era-segment${
                        currentSegment ? " is-current" : ""
                      }${segment.approximate ? " is-approximate" : ""}${
                        segment.openStart ? " is-open-start" : ""
                      }${segment.width < 64 ? " is-tight" : ""}`}
                      style={{
                        left: `${segment.left}px`,
                        width: `${segment.width}px`,
                        top: `${segment.lane * layout.laneHeight + 4}px`,
                        height: `${layout.laneHeight - 6}px`,
                      }}
                      aria-current={currentSegment ? "true" : undefined}
                      aria-label={label}
                      title={label}
                      onClick={() => onSelect(segment.period)}
                    >
                      <span className="history-era-segment-name">
                        {segment.period.name_zh_cn}
                      </span>
                      {showYears ? (
                        <span className="history-era-segment-years">
                          {segment.openStart ? (
                            <>? — {yearText(segment.period.end_year)}</>
                          ) : (
                            <>
                              {yearText(segment.period.start_year)} —{" "}
                              {yearText(segment.period.end_year)}
                            </>
                          )}
                        </span>
                      ) : null}
                    </button>
                  );
                })}
              </div>
              <div
                className="history-era-axis"
                style={{
                  top: `${(hasDensity ? densityHeight : 0) + bandHeight}px`,
                  height: `${layout.axisHeight}px`,
                }}
              >
                {layout.epochLeft !== null ? (
                  <span
                    className="history-era-zone is-bce"
                    style={{ width: `${layout.epochLeft}px` }}
                    aria-hidden="true"
                  >
                    公元前
                  </span>
                ) : null}
                {layout.epochLeft !== null ? (
                  <span
                    className="history-era-zone is-ce"
                    style={{ left: `${layout.epochLeft}px` }}
                    aria-hidden="true"
                  >
                    公元
                  </span>
                ) : null}
                {layout.ticks.map((tick) => (
                  <span
                    key={tick.year}
                    className={`history-era-tick${
                      tick.year === 0 ? " is-epoch" : ""
                    }`}
                    style={{ left: `${tick.left}px` }}
                    aria-hidden="true"
                  >
                    <i />
                    <b>{tick.label}</b>
                  </span>
                ))}
              </div>
            </div>
          </div>
        </div>
        ) : (
          <div
            className="history-era-fallback"
            role="group"
            aria-label="全部历史时期"
          >
            {periods.map((period) => (
              <button
                key={period.id}
                type="button"
                className={`history-era-chip${
                  period.id === currentId ? " is-current" : ""
                }`}
                aria-current={period.id === currentId ? "true" : undefined}
                onClick={() => onSelect(period)}
              >
                <strong>{period.name_zh_cn}</strong>
                <small>{periodRangeLabel(period)}</small>
              </button>
            ))}
          </div>
        )}
        {compact && layout.laneCount > 2 ? (
          <button
            type="button"
            className="history-era-lanes-toggle"
            aria-expanded={effectiveShowAll}
            onClick={() => setShowAllLanes((value) => !value)}
          >
            {effectiveShowAll
              ? "收起并发轨道"
              : `显示全部 ${layout.laneCount} 条并发轨道`}
          </button>
        ) : null}
        <div className="history-era-summary" aria-live="polite">
          <span className="history-era-summary-current">
            <i aria-hidden="true" />
            当前 · <strong>{current?.name_zh_cn ?? "—"}</strong>
          </span>
          <span className="history-era-summary-range">
            {current ? periodRangeLabel(current) : "年代待考"}
            {currentApproximate ? " · 约略年代" : ""}
          </span>
          {hasGeometry ? (
            scrollable ? (
              <span className="history-era-summary-hint">
                <ArrowLeft size={11} /> 横向滑动了解更多{" "}
                <ArrowRight size={11} />
              </span>
            ) : (
              <span className="history-era-summary-hint">
                点击或使用方向键切换时期
              </span>
            )
          ) : (
            <span className="history-era-summary-hint">点击任一时期切换</span>
          )}
        </div>
      </div>
    </section>
  );
}

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    typeof window === "undefined" ? false : window.matchMedia(query).matches,
  );
  useEffect(() => {
    const media = window.matchMedia(query);
    setMatches(media.matches);
    const listener = (event: MediaQueryListEvent) =>
      setMatches(event.matches);
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, [query]);
  return matches;
}
