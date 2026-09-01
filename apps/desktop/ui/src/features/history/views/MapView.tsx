import { useId, useState } from "react";
import { eraAtYear, historyEras } from "../historyEras";
import { compactYear } from "../labels";
import { ERA_DEFAULT_ACCENT } from "../historyEras";

/**
 * Map View（结构版）：先提供「年份 → 时代」的历史年份定位器与时代分段色带
 * （真实数据），疆域地图、历史地点与路线将在后续版本接入。
 */
export function MapView() {
  const titleId = useId();
  const [year, setYear] = useState(1368);

  const minYear = Math.min(...historyEras.map((era) => era.startYear));
  const maxYear = Math.max(...historyEras.map((era) => era.endYear));
  const era = eraAtYear(year);

  return (
    <section className="history-map-view" aria-labelledby={titleId}>
      <header className="history-section-head">
        <div>
          <span>MAP</span>
          <h2 id={titleId}>历史地图</h2>
        </div>
        <p>拖动年份，查看它属于哪个时代</p>
      </header>

      <div className="history-map-controls">
        <output className="history-map-year" aria-live="polite">
          <b>{compactYear(year)}</b>
          <span>年</span>
        </output>
        <input
          type="range"
          min={minYear}
          max={maxYear}
          step={1}
          value={year}
          onChange={(event) => setYear(Number(event.target.value))}
          aria-label="选择年份"
          className="history-map-year-range"
        />
        {era ? (
          <div className="history-map-era-hit">
            <span>所属时代</span>
            <b>{era.name}</b>
            <small>{era.yearLabel}</small>
            <p>{era.tagline ?? era.description}</p>
          </div>
        ) : null}
      </div>

      <div className="history-map-strip" aria-hidden="true">
        {historyEras.map((item) => {
          const left =
            ((item.startYear - minYear) / (maxYear - minYear)) * 100;
          const width =
            ((item.endYear - item.startYear) / (maxYear - minYear)) * 100;
          const accent = item.visual?.accent ?? ERA_DEFAULT_ACCENT;
          return (
            <i
              key={item.id}
              className={year >= item.startYear && year <= item.endYear ? "is-active" : ""}
              style={
                {
                  left: `${left}%`,
                  width: `${Math.max(width, 0.4)}%`,
                  "--era-accent": accent,
                } as React.CSSProperties
              }
              title={`${item.name} · ${item.yearLabel}`}
            />
          );
        })}
        <b className="history-map-marker" style={{ left: `${((year - minYear) / (maxYear - minYear)) * 100}%` }} />
      </div>

      <div className="history-map-coming">
        <p>
          疆域变化地图、历史地点、城市、战争与路线的可视化将在此接入。
          当前使用的年份定位与时代分段均为真实数据。
        </p>
      </div>
    </section>
  );
}