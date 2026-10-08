/**
 * SketchIllustration — 统一手绘线稿插画。
 *
 * 每个模块拥有自己的「插画语言」，但都出自同一支铅笔：
 * 同一 strokeWidth、同一圆头线帽、同一 currentColor 墨色。
 * 全部为内联 SVG（无外部图片、不增加 bundle 体积）。
 */
export type SketchIllustrationName =
  | "history"
  | "geography"
  | "language"
  | "study"
  | "news"
  | "rss"
  | "travel"
  | "knowledge"
  | "graph"
  | "search"
  | "server"
  | "applications"
  | "markdown"
  | "collections"
  | "review"
  | "home"
  | "ai";

interface SketchIllustrationProps {
  name: SketchIllustrationName;
  size?: number;
  className?: string;
  title?: string;
}

const stroke = {
  fill: "none" as const,
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

function Paths({ name }: { name: SketchIllustrationName }) {
  switch (name) {
    case "history":
      return (
        <g {...stroke}>
          <path d="M6 8c4-2 8-2 12 0v30c-4-2-8-2-12 0z" />
          <path d="M30 8c-4-2-8-2-12 0v30c4-2 8-2 12 0z" />
          <path d="M9 14h6M9 19h6M27 14h-6M27 19h-6" />
          <path d="M6 38h24" opacity="0.5" />
        </g>
      );
    case "geography":
      return (
        <g {...stroke}>
          <circle cx="19" cy="20" r="13" />
          <path d="M6 20h26M19 7c4 4 6 8 6 13s-2 9-6 13c-4-4-6-8-6-13s2-9 6-13z" />
          <path d="M11 12c3 1 5 1 8 0M11 28c3-1 5-1 8 0" opacity="0.6" />
          <path d="M32 30l3-3 3 3-3 3z" />
          <path d="M35 30v-8" />
        </g>
      );
    case "language":
      return (
        <g {...stroke}>
          <rect x="6" y="8" width="22" height="26" rx="3" />
          <path d="M12 8v26" opacity="0.6" />
          <path d="M17 16h7M17 21h5" />
          <path d="M24 26c4 0 6 3 4 6l-2 4 5 2" />
          <circle cx="31" cy="14" r="6" />
          <path d="M31 11v6M28 14h6" />
        </g>
      );
    case "study":
      return (
        <g {...stroke}>
          <path d="M7 10h18a3 3 0 0 1 3 3v22H10a3 3 0 0 1-3-3z" />
          <path d="M28 13l6 2-6 20" opacity="0.7" />
          <path d="M12 16h10M12 21h10M12 26h6" />
          <path d="M31 8l4 1" opacity="0.6" />
        </g>
      );
    case "news":
      return (
        <g {...stroke}>
          <path d="M6 10h20v24H9a3 3 0 0 1-3-3z" />
          <path d="M26 15h6v16a3 3 0 0 1-6 3" />
          <path d="M10 15h12v6H10zM10 25h12M10 29h8" />
        </g>
      );
    case "rss":
      return (
        <g {...stroke}>
          <path d="M8 30h20V12a2 2 0 0 0-2-2H10a2 2 0 0 0-2 2z" />
          <circle cx="14" cy="24" r="2.4" />
          <path d="M14 17a7 7 0 0 1 7 7M14 12a12 12 0 0 1 12 12" />
        </g>
      );
    case "travel":
      return (
        <g {...stroke}>
          <path d="M8 12l24-5-6 26-6-8-8 4 2-7z" />
          <path d="M17 22l6 3" opacity="0.6" />
          <path d="M6 32c2 2 4 2 6 0" />
        </g>
      );
    case "knowledge":
      return (
        <g {...stroke}>
          <circle cx="12" cy="14" r="4" />
          <circle cx="28" cy="11" r="4" />
          <circle cx="24" cy="28" r="4" />
          <circle cx="9" cy="28" r="3" />
          <path d="M16 14l8-2M14 17l7 8M12 25l-1 2M27 15l-2 9" opacity="0.7" />
          <path d="M22 6h10v10" opacity="0.4" />
        </g>
      );
    case "graph":
      return (
        <g {...stroke}>
          <circle cx="19" cy="9" r="3.5" />
          <circle cx="9" cy="28" r="3.5" />
          <circle cx="29" cy="28" r="3.5" />
          <circle cx="19" cy="20" r="3" opacity="0.7" />
          <path d="M17 12l-6 13M21 12l6 13M12 27h14M17 22l-6 4M21 22l6 4" opacity="0.7" />
        </g>
      );
    case "search":
      return (
        <g {...stroke}>
          <circle cx="16" cy="16" r="9" />
          <path d="M23 23l9 9" />
          <path d="M11 13h10M11 17h7" opacity="0.6" />
        </g>
      );
    case "server":
      return (
        <g {...stroke}>
          <rect x="8" y="7" width="24" height="9" rx="2" />
          <rect x="8" y="18" width="24" height="9" rx="2" />
          <rect x="8" y="29" width="24" height="6" rx="2" opacity="0.7" />
          <circle cx="12" cy="11.5" r="1" fill="currentColor" />
          <circle cx="12" cy="22.5" r="1" fill="currentColor" />
          <path d="M17 11.5h10M17 22.5h10M17 32h10" opacity="0.6" />
        </g>
      );
    case "applications":
      return (
        <g {...stroke}>
          <path d="M6 15l13-8 13 8-13 8z" />
          <path d="M6 15v10l13 8 13-8V15" />
          <path d="M19 23v10" opacity="0.6" />
        </g>
      );
    case "markdown":
      return (
        <g {...stroke}>
          <rect x="6" y="8" width="26" height="26" rx="3" />
          <path d="M12 16h14M12 21h14M12 26h9" />
          <path d="M27 24l3 3 3-4" opacity="0.7" />
        </g>
      );
    case "collections":
      return (
        <g {...stroke}>
          <path d="M7 12a2 2 0 0 1 2-2h8l3 4h12a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2z" />
          <path d="M7 20h26" opacity="0.5" />
        </g>
      );
    case "review":
      return (
        <g {...stroke}>
          <rect x="9" y="9" width="18" height="22" rx="2" transform="rotate(-6 18 20)" />
          <rect x="13" y="10" width="18" height="22" rx="2" />
          <path d="M18 16h8M18 21h8M18 26h5" />
        </g>
      );
    case "home":
      return (
        <g {...stroke}>
          <path d="M7 18l12-10 12 10" />
          <path d="M10 16v16h18V16" />
          <path d="M16 32V23h6v9" opacity="0.7" />
        </g>
      );
    case "ai":
      return (
        <g {...stroke}>
          <rect x="9" y="10" width="20" height="17" rx="4" />
          <path d="M15 20h2M21 20h2" />
          <path d="M19 10V5M16 5h6" />
          <path d="M4 17v4M34 17v4" opacity="0.6" />
          <path d="M13 30h12" opacity="0.6" />
        </g>
      );
    default:
      return null;
  }
}

export function SketchIllustration({
  name,
  size = 40,
  className,
  title,
}: SketchIllustrationProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 40 40"
      role={title ? "img" : "presentation"}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    >
      <Paths name={name} />
    </svg>
  );
}

/** 一个更小的、用于按钮/列表的手绘图标（比 Phosphor 更有铅笔味）。 */
export function SketchDoodle({
  name,
  size = 18,
}: {
  name: SketchIllustrationName;
  size?: number;
}) {
  return <SketchIllustration name={name} size={size} />;
}
