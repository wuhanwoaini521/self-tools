import { useId, useMemo } from "react";

export interface ProfileData {
  heights: number[];
  min: number;
  max: number;
  /** vertical distance A->B in world units (for the horizontal axis label) */
  length: number;
  aLabel: string;
  bLabel: string;
}

const W = 560;
const H = 150;
const PADX = 8;
const PADY = 18;

function pretty(v: number): string {
  return v >= 100 ? Math.round(v).toLocaleString() : v.toFixed(1);
}

/** Elevation profile rendered below the 3D viewer in section mode. */
export function ElevationProfile({ data }: { data: ProfileData }) {
  const gradId = useId();
  const path = useMemo(() => {
    const n = data.heights.length;
    if (n < 2) return "";
    const span = Math.max(data.max - data.min, 1e-6);
    let d = `M ${PADX} ${PADY + (1 - (data.heights[0] - data.min) / span) * (H - 2 * PADY)}`;
    for (let i = 1; i < n; i += 1) {
      const x = PADX + (i / (n - 1)) * (W - 2 * PADX);
      const y =
        PADY + (1 - (data.heights[i] - data.min) / span) * (H - 2 * PADY);
      d += ` L ${x.toFixed(1)} ${y.toFixed(1)}`;
    }
    return d;
  }, [data]);

  const areaPath = useMemo(
    () =>
      path ? `${path} L ${W - PADX} ${H - PADY} L ${PADX} ${H - PADY} Z` : "",
    [path],
  );

  const minY = PADY + (1 - 0) * (H - 2 * PADY);
  return (
    <div className="landform-profile">
      <div className="landform-profile-head">
        <span>Elevation Profile</span>
        <small>
          A ─ {data.length.toFixed(0)} 单位 → B · 相对高差{" "}
          {pretty(data.max - data.min)}
        </small>
      </div>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="地形剖面示意图">
        <defs>
          <linearGradient id={gradId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#c9a063" stopOpacity="0.55" />
            <stop offset="100%" stopColor="#c9a063" stopOpacity="0.08" />
          </linearGradient>
        </defs>
        {[0.25, 0.5, 0.75].map((t) => (
          <line
            key={t}
            x1={PADX}
            x2={W - PADX}
            y1={PADY + t * (H - 2 * PADY)}
            y2={PADY + t * (H - 2 * PADY)}
            className="landform-profile-grid"
          />
        ))}
        <text x={PADX - 4} y={PADY + 4} className="landform-profile-axis">
          {pretty(data.max)}
        </text>
        <text x={PADX - 4} y={H - PADY + 4} className="landform-profile-axis">
          {pretty(data.min)}
        </text>
        <path d={areaPath} fill={`url(#${gradId})`} />
        <path d={path} className="landform-profile-line" fill="none" />
        <text x={PADX} y={minY - 6} className="landform-profile-mark">
          A
        </text>
        <text x={W - PADX - 8} y={minY - 6} className="landform-profile-mark">
          B
        </text>
      </svg>
    </div>
  );
}
