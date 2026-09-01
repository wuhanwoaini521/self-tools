import { useCallback, useEffect, useRef } from "react";
import { clampEraIndex, historyEras } from "../historyEras";

/**
 * History Chronology Navigator：页面顶部的中国历史总时间轴。
 *
 * - 节点宽度按时代实际时长比例（duration * scale），叠加 min-width 与上限，
 *   直观体现「秦短汉长、唐很长的认知」；
 * - 交互：点击 / hover / 键盘 ← → / 滚轮 / trackpad / 拖拽，均为横向 scrub；
 * - 当前时代自动保持在视口中央附近（scroll-snap + scrollIntoView）。
 */

const MIN_NODE_WIDTH = 64;
const WIDTH_SCALE = 0.55; // px per year
const MAX_NODE_WIDTH = 300;

function eraWidth(eraIndex: number): number {
  const era = historyEras[eraIndex];
  const duration = Math.max(1, era.endYear - era.startYear);
  const raw = duration * WIDTH_SCALE;
  return Math.min(MAX_NODE_WIDTH, Math.max(MIN_NODE_WIDTH, raw));
}

export const CHRONOLOGY_ESTIMATED_WIDTH = historyEras.reduce(
  (sum, _era, index) => sum + eraWidth(index),
  0,
);

export function HistoryChronologyNavigator({
  currentIndex,
  onSelect,
  active = true,
}: {
  currentIndex: number;
  onSelect: (index: number) => void;
  /** 是否启用全局 ← → 键盘切换（首页无输入框时） */
  active?: boolean;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const nodeRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const dragRef = useRef<{
    startX: number;
    scrollLeft: number;
    moved: boolean;
    pointerId: number;
  } | null>(null);
  const currentRef = useRef(currentIndex);
  const explicitRef = useRef(false);
  const scrollTickingRef = useRef(false);

  currentRef.current = currentIndex;

  const select = useCallback(
    (index: number) => {
      const clamped = clampEraIndex(index);
      if (clamped === currentRef.current) return;
      explicitRef.current = true;
      onSelect(clamped);
    },
    [onSelect],
  );

  // 显式切换（点击 / 键盘）后把当前时代滚到视口中央。
  useEffect(() => {
    if (!explicitRef.current) return;
    const node = nodeRefs.current[currentIndex];
    if (node) {
      node.scrollIntoView({ inline: "center", block: "nearest", behavior: "smooth" });
    }
    explicitRef.current = false;
  }, [currentIndex]);

  // 全局 ← → 键切换时代（首页无输入框时）。
  useEffect(() => {
    if (!active) return;
    const onKey = (event: KeyboardEvent) => {
      const el = document.activeElement;
      if (
        el &&
        (el.tagName === "INPUT" ||
          el.tagName === "TEXTAREA" ||
          el.tagName === "SELECT")
      )
        return;
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        select(currentRef.current - 1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        select(currentRef.current + 1);
      } else if (event.key === "Home") {
        event.preventDefault();
        select(0);
      } else if (event.key === "End") {
        event.preventDefault();
        select(historyEras.length - 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, select]);

  // 从滚动位置推导最接近视口中央的时代。
  const syncFromScroll = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    if (scrollTickingRef.current) return;
    scrollTickingRef.current = true;
    requestAnimationFrame(() => {
      scrollTickingRef.current = false;
      const centerX =
        viewport.getBoundingClientRect().left + viewport.clientWidth / 2;
      let best = currentRef.current;
      let bestDist = Infinity;
      nodeRefs.current.forEach((node, index) => {
        if (!node) return;
        const rect = node.getBoundingClientRect();
        const dist = Math.abs(rect.left + rect.width / 2 - centerX);
        if (dist < bestDist) {
          bestDist = dist;
          best = index;
        }
      });
      if (best !== currentRef.current) {
        currentRef.current = best;
        onSelect(best);
      }
    });
  }, [onSelect]);

  const onWheel = useCallback((event: WheelEvent) => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    event.preventDefault();
    const delta =
      Math.abs(event.deltaX) > Math.abs(event.deltaY)
        ? event.deltaX
        : event.deltaY;
    viewport.scrollBy({ left: delta, behavior: "auto" });
  }, []);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [onWheel]);

  const onPointerDown = useCallback((event: React.PointerEvent) => {
    if (event.button !== 0) return;
    const viewport = viewportRef.current;
    if (!viewport) return;
    event.preventDefault();
    viewport.setPointerCapture(event.pointerId);
    dragRef.current = {
      startX: event.clientX,
      scrollLeft: viewport.scrollLeft,
      moved: false,
      pointerId: event.pointerId,
    };
  }, []);

  const onPointerMove = useCallback((event: React.PointerEvent) => {
    const drag = dragRef.current;
    const viewport = viewportRef.current;
    if (!drag || !viewport || drag.pointerId !== event.pointerId) return;
    const dx = event.clientX - drag.startX;
    if (Math.abs(dx) > 6) drag.moved = true;
    if (drag.moved) {
      viewport.scrollLeft = drag.scrollLeft - dx;
      viewport.classList.add("is-dragging");
    }
  }, []);

  const endDrag = useCallback(
    (event: React.PointerEvent) => {
      const drag = dragRef.current;
      const viewport = viewportRef.current;
      if (!drag || !viewport || drag.pointerId !== event.pointerId) return;
      dragRef.current = null;
      viewport.classList.remove("is-dragging");
      try {
        viewport.releasePointerCapture(event.pointerId);
      } catch {
        /* capture already released */
      }
      if (drag.moved) {
        syncFromScroll();
      } else {
        // 轻点：选中指针所在位置最近的时代。
        const centerX = event.clientX;
        let best = currentRef.current;
        let bestDist = Infinity;
        nodeRefs.current.forEach((node, index) => {
          if (!node) return;
          const rect = node.getBoundingClientRect();
          const dist = Math.abs(rect.left + rect.width / 2 - centerX);
          if (dist < bestDist) {
            bestDist = dist;
            best = index;
          }
        });
        if (best !== currentRef.current) select(best);
      }
    },
    [select, syncFromScroll],
  );

  const onScroll = useCallback(() => {
    if (!dragRef.current?.moved) syncFromScroll();
  }, [syncFromScroll]);

  return (
    <section className="history-chronology" aria-label="中国历史总时间轴">
      <header className="history-chronology-head">
        <div>
          <span>CHRONOLOGY</span>
          <h2>中国历史总时间轴</h2>
        </div>
        <p>节点宽度按各时代实际时长比例显示</p>
      </header>
      <div
        className="history-chronology-viewport"
        ref={viewportRef}
        onScroll={onScroll}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        tabIndex={0}
        role="group"
        aria-label="拖动、滚动或使用方向键浏览时代"
      >
        <div className="history-chronology-track">
          {historyEras.map((era, index) => (
            <button
              type="button"
              key={era.id}
              ref={(node) => {
                nodeRefs.current[index] = node;
              }}
              className={`history-chronology-node${index === currentIndex ? " is-current" : ""}`}
              style={{ width: eraWidth(index) }}
              onClick={() => select(index)}
              aria-current={index === currentIndex ? "true" : undefined}
              aria-label={`${era.name}，${era.yearLabel}`}
              title={`${era.name} · ${era.yearLabel}`}
            >
              <i aria-hidden="true" />
              <b>{era.name}</b>
              <small>{era.yearLabel}</small>
            </button>
          ))}
        </div>
        <div className="history-chronology-axis" aria-hidden="true" />
      </div>
      <footer className="history-chronology-hint">
        滚动、拖拽或使用 ← → 键浏览 · 当前时代自动保持在中央附近
      </footer>
    </section>
  );
}