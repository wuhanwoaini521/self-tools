import type { CSSProperties } from "react";
import tangPalaceImage from "../../../assets/history-tang-palace.png";
import { ERA_DEFAULT_ACCENT, type HistoryEra } from "../historyEras";
import type { EraVisual } from "../types/history";

/**
 * Era Visual Plate：每个时代的「视觉身份」。
 * 没有 asset 图片的时代，用数据驱动的 SVG 图版（经纬网格 / 线路 / 青铜纹 / 几何纹）区分，
 * 避免所有时代都是同一张卡片。纯装饰层，低计算量，无动画。
 */

function MapMotif({ accent }: { accent: string }) {
  return (
    <svg
      viewBox="0 0 480 340"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      {[0, 1, 2, 3, 4].map((i) => (
        <path
          key={`v${i}`}
          d={`M ${40 + i * 100} 20 C ${50 + i * 100} 120, ${30 + i * 100} 220, ${60 + i * 100} 320`}
          stroke={accent}
          strokeOpacity={0.16}
          strokeWidth={1}
          fill="none"
        />
      ))}
      {[0, 1, 2, 3].map((i) => (
        <path
          key={`h${i}`}
          d={`M 20 ${40 + i * 80} C 140 ${30 + i * 80}, 300 ${50 + i * 80}, 460 ${40 + i * 80}`}
          stroke={accent}
          strokeOpacity={0.16}
          strokeWidth={1}
          fill="none"
        />
      ))}
      <path
        d="M 240 70 L 380 175 L 240 280 L 100 175 Z"
        stroke={accent}
        strokeOpacity={0.5}
        strokeWidth={1.5}
        fill={accent}
        fillOpacity={0.05}
      />
      <circle cx={240} cy={175} r={4} fill={accent} fillOpacity={0.7} />
    </svg>
  );
}

function RouteMotif({ accent }: { accent: string }) {
  return (
    <svg
      viewBox="0 0 480 340"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      <path
        d="M 70 270 C 160 220, 200 120, 300 95 S 420 120, 430 70"
        stroke={accent}
        strokeOpacity={0.45}
        strokeWidth={2}
        strokeDasharray="6 6"
        fill="none"
      />
      <circle cx={70} cy={270} r={6} fill={accent} fillOpacity={0.55} />
      <circle cx={160} cy={225} r={4} fill={accent} fillOpacity={0.5} />
      <circle cx={300} cy={95} r={4} fill={accent} fillOpacity={0.5} />
      <circle cx={430} cy={70} r={7} fill={accent} fillOpacity={0.7} />
      {[0, 1, 2].map((i) => (
        <path
          key={`w${i}`}
          d={`M ${70 + i * 30} 300 C ${90 + i * 30} 260, ${110 + i * 30} 260, ${130 + i * 30} 300`}
          stroke={accent}
          strokeOpacity={0.12}
          strokeWidth={1}
          fill="none"
        />
      ))}
    </svg>
  );
}

function ArtifactMotif({ accent }: { accent: string }) {
  return (
    <svg
      viewBox="0 0 480 340"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      {[40, 78, 118, 156].map((r, i) => (
        <circle
          key={r}
          cx={240}
          cy={170}
          r={r}
          stroke={accent}
          strokeOpacity={0.16 + i * 0.09}
          strokeWidth={1}
          fill="none"
        />
      ))}
      {Array.from({ length: 24 }, (_, i) => {
        const angle = (i / 24) * Math.PI * 2;
        const x1 = 240 + Math.cos(angle) * 40;
        const y1 = 170 + Math.sin(angle) * 40;
        const x3 = 240 + Math.cos(angle) * 150;
        const y3 = 170 + Math.sin(angle) * 158;
        return (
          <line
            key={i}
            x1={x1}
            y1={y1}
            x2={x3}
            y2={y3}
            stroke={accent}
            strokeOpacity={0.22}
            strokeWidth={1}
          />
        );
      })}
      <path
        d="M 190 170 a 50 50 0 0 1 100 0 a 50 50 0 0 1 -100 0"
        stroke={accent}
        strokeOpacity={0.55}
        strokeWidth={1.5}
        fill={accent}
        fillOpacity={0.06}
      />
    </svg>
  );
}

function PatternMotif({ accent }: { accent: string }) {
  const diamond = [0, 0, 14, -22, 28, 0, 14, 22].join(",");
  return (
    <svg
      viewBox="0 0 480 340"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      <defs>
        <pattern
          id="hist-diamond"
          width={56}
          height={56}
          patternUnits="userSpaceOnUse"
        >
          <path
            d={`M ${diamond}`}
            fill={accent}
            fillOpacity={0.09}
            stroke={accent}
            strokeOpacity={0.18}
            strokeWidth={0.8}
          />
        </pattern>
      </defs>
      <rect width={480} height={340} fill="url(#hist-diamond)" />
      <path
        d="M 240 40 C 330 80, 400 140, 400 200 C 400 260, 320 300, 240 300 C 160 300, 80 260, 80 200 C 80 140, 150 80, 240 40"
        stroke={accent}
        strokeOpacity={0.4}
        strokeWidth={1.5}
        fill="none"
      />
    </svg>
  );
}

function VisualMotif({
  visual,
  accent,
}: {
  visual: EraVisual;
  accent: string;
}) {
  switch (visual.type) {
    case "map":
      return <MapMotif accent={accent} />;
    case "route":
      return <RouteMotif accent={accent} />;
    case "artifact":
      return <ArtifactMotif accent={accent} />;
    case "pattern":
      return <PatternMotif accent={accent} />;
    default:
      return null;
  }
}

export function EraVisualPlate({
  era,
  className = "",
}: {
  era: HistoryEra;
  className?: string;
}) {
  const visual: EraVisual = era.visual ?? {
    type: "map",
    focalPoint: "历史疆域",
  };
  const accent = visual.accent ?? ERA_DEFAULT_ACCENT;
  const motif = visual.motif ?? era.shortName;
  const useImage = visual.type === "illustration" && visual.asset === "tang";

  return (
    <div
      className={`history-visual${className ? ` ${className}` : ""}`}
      style={{ "--era-accent": accent } as CSSProperties}
    >
      <div
        className="history-visual-frame"
        role="img"
        aria-label={`${era.name} 时代视觉 · ${visual.focalPoint ?? era.yearLabel}`}
      >
        {useImage ? (
          <img
            className="history-visual-img"
            src={tangPalaceImage}
            alt=""
            aria-hidden="true"
          />
        ) : (
          <div className="history-visual-motif-layer" aria-hidden="true">
            <VisualMotif visual={visual} accent={accent} />
          </div>
        )}
        <span className="history-visual-motif" aria-hidden="true">
          {motif}
        </span>
        <div className="history-visual-caption">
          <b>{era.name}</b>
          <span>{era.yearLabel}</span>
        </div>
        {visual.focalPoint ? (
          <small className="history-visual-focus">{visual.focalPoint}</small>
        ) : null}
      </div>
    </div>
  );
}
