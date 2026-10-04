/**
 * 画板笔刷引擎（纯前端，无后端依赖）。
 *
 * 目标：接近 GoodNotes / Notability 的手写手感——
 * - **压感**：优先用 `PointerEvent.pressure`（触控笔真实压感）；
 *   没有压感的设备（鼠标 / 触摸）用**速度反推**：写得慢 → 粗，写得快 → 细，
 *   这是市面笔记软件在无压感设备上的通用做法，手感自然且不需要额外硬件。
 * - **多种笔型**：圆珠笔 / 马克笔 / 荧光笔 / 铅笔，各有不同的混合、透明与纹理。
 * - **可变线宽**：按每个采样点的压力逐段绘制，而不是整笔一个宽度。
 *
 * ## 数据兼容
 *
 * 旧学习板里只有 `{color, width, points:[x,y,...], eraser?}`。新增字段全部可选：
 * 没有 `brush` / `pressures` 的旧笔画按「圆珠笔 + 恒定宽度」渲染，视觉不变。
 */

export type BrushKind = "ballpoint" | "marker" | "highlighter" | "pencil";

export const BRUSHES: ReadonlyArray<{
  id: BrushKind;
  label: string;
  hint: string;
}> = [
  { id: "ballpoint", label: "圆珠笔", hint: "日常书写，压感明显" },
  { id: "marker", label: "马克笔", hint: "粗、圆润、略透" },
  { id: "highlighter", label: "荧光笔", hint: "扁平半透明，可叠加" },
  { id: "pencil", label: "铅笔", hint: "颗粒感、淡" },
];

/** 笔刷渲染参数（与 id 分离，便于以后调手感而不改数据）。 */
export interface BrushProfile {
  /** 基础线宽（px），用户滑杆控制。 */
  baseWidth: number;
  /** 压感对线宽的影响系数（0 = 无压感，1 = 全力最粗）。 */
  pressureGain: number;
  /** 整体不透明度。 */
  opacity: number;
  /** 荧光笔用 multiply 混合，普通笔用 source-over。 */
  composite: "source-over" | "multiply";
  /** 铅笔的颗粒强度（0 = 平滑，1 = 最毛糙）。 */
  grain: number;
  /** 起笔/收笔是否收细（马克笔不收，荧光笔不收）。 */
  taper: boolean;
}

export const BRUSH_PROFILES: Record<BrushKind, BrushProfile> = {
  ballpoint: {
    baseWidth: 3,
    pressureGain: 0.75,
    opacity: 1,
    composite: "source-over",
    grain: 0,
    taper: true,
  },
  marker: {
    baseWidth: 9,
    pressureGain: 0.45,
    opacity: 0.85,
    composite: "source-over",
    grain: 0,
    taper: false,
  },
  highlighter: {
    baseWidth: 18,
    pressureGain: 0.1,
    opacity: 0.35,
    composite: "multiply",
    grain: 0,
    taper: false,
  },
  pencil: {
    baseWidth: 2.5,
    pressureGain: 0.6,
    opacity: 0.75,
    composite: "source-over",
    grain: 0.55,
    taper: true,
  },
};

/** 笔刷预设：宽度 + 颜色 + 笔型，三者一组。 */
export interface BrushPreset {
  id: string;
  label: string;
  brush: BrushKind;
  color: string;
  width: number;
}

/** 调色板：按语义分组（写实一点，接近纸质笔记本的笔袋）。 */
export const PALETTE_GROUPS: ReadonlyArray<{
  label: string;
  colors: ReadonlyArray<{ value: string; label: string }>;
}> = [
  {
    label: "常用",
    colors: [
      { value: "#151b1f", label: "墨黑" },
      { value: "#2d6cdf", label: "蓝" },
      { value: "#c0392b", label: "红" },
    ],
  },
  {
    label: "笔记",
    colors: [
      { value: "#1f9d55", label: "绿" },
      { value: "#b8860b", label: "棕" },
      { value: "#7d3c98", label: "紫" },
    ],
  },
  {
    label: "标记",
    colors: [
      { value: "#f5a623", label: "橙" },
      { value: "#e8c547", label: "黄" },
      { value: "#00a5b5", label: "青" },
    ],
  },
];

export const ALL_COLORS: ReadonlyArray<string> = PALETTE_GROUPS.flatMap((group) =>
  group.colors.map((color) => color.value),
);

/** 笔刷滑杆范围（px）。 */
export const WIDTH_RANGE = { min: 1, max: 32, step: 0.5, defaultBallpoint: 3 } as const;

/** 单点采样。 */
export interface BrushPoint {
  x: number;
  y: number;
  /** 归一化压力 0..1。 */
  pressure: number;
}

/**
 * 从指针事件取压力。
 *
 * - 触控笔：`pressure` 真实（0..1），并且 `pointerType === "pen"` 时最可信。
 * - 鼠标：按下恒为 0.5（浏览器规范如此），所以对鼠标改用速度反推。
 * - 触摸：多数设备 pressure 恒为 1 或 0，同样走速度反推。
 */
export function readPressure(event: {
  pointerType: string;
  pressure: number;
  velocity?: number;
}): number {
  const isPen = event.pointerType === "pen";
  const raw = event.pressure;
  const usable = isPen && Number.isFinite(raw) && raw > 0 && raw < 1;
  if (usable) return clamp01(raw);
  // 速度反推：慢 → 粗。velocity 为 px/ms。
  const velocity = event.velocity ?? 0;
  // 1.6 px/ms 已经很慢；0 视为最慢。
  const slow = 1 - Math.min(1, velocity / 1.6);
  // 映射到 0.35..1，让鼠标/触摸也有可见的粗细变化。
  return 0.35 + slow * 0.65;
}

export function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0.5;
  return Math.min(1, Math.max(0, value));
}

/** 某采样点处的实际线宽。 */
export function widthAt(
  profile: BrushProfile,
  pressure: number,
  /** 起笔/收笔的收细系数 0..1（1 = 不收细）。 */
  taperFactor = 1,
): number {
  const p = clamp01(pressure);
  const scaled = profile.baseWidth * (1 - profile.pressureGain + profile.pressureGain * p);
  // 收笔至少保留 35% 宽度，避免尖端消失。
  return Math.max(profile.baseWidth * 0.35, scaled * taperFactor);
}

/** 首尾收细系数：起笔 0 → 1，收笔 1 → 0。 */
export function taperFactorAt(
  profile: BrushProfile,
  index: number,
  total: number,
): number {
  if (!profile.taper || total < 3) return 1;
  const head = Math.min(1, index / 2);
  const tail = Math.min(1, (total - 1 - index) / 2);
  return 0.55 + 0.45 * Math.min(head, tail);
}

/**
 * 把一笔画到 canvas 上（可变线宽 + 可选颗粒）。
 *
 * 逐段绘制而不是整笔 `beginPath`，否则线宽无法变化。
 */
export function renderStroke(
  ctx: CanvasRenderingContext2D,
  points: BrushPoint[],
  options: {
    color: string;
    brush: BrushKind;
    width: number;
    eraser?: boolean;
    /** 稳定随机源（按笔迹 id 取），保证重绘时颗粒一致不闪烁。 */
    seed?: number;
  },
): void {
  const { color, brush, eraser, seed = 1 } = options;
  if (points.length === 0) return;

  const profile: BrushProfile = {
    ...BRUSH_PROFILES[brush],
    baseWidth: options.width,
  };

  ctx.save();
  if (eraser) {
    ctx.globalCompositeOperation = "destination-out";
    ctx.strokeStyle = color;
  } else {
    ctx.globalCompositeOperation = profile.composite;
    ctx.strokeStyle = color;
    ctx.globalAlpha = profile.opacity;
  }
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  if (points.length === 1) {
    // 单点：画一个点，否则轻点屏幕不会留下痕迹。
    const only = points[0];
    ctx.beginPath();
    ctx.arc(only.x, only.y, widthAt(profile, only.pressure) / 2, 0, Math.PI * 2);
    ctx.fillStyle = ctx.strokeStyle;
    ctx.fill();
    ctx.restore();
    return;
  }

  // 压感增益很低（马克笔 / 荧光笔）时用**单条路径**画：
  // 逐段 + 圆头 + 半透明会在接缝处叠加加深，荧光笔就会变成一串「珠子」。
  // 这类笔本来就是平的，没必要逐段。
  if (profile.pressureGain <= 0.2) {
    ctx.lineWidth = widthAt(profile, points[0].pressure);
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    for (const point of points.slice(1)) ctx.lineTo(point.x, point.y);
    ctx.stroke();
    ctx.restore();
    return;
  }

  for (let i = 0; i < points.length - 1; i++) {
    const from = points[i];
    const to = points[i + 1];
    const w = widthAt(profile, to.pressure, taperFactorAt(profile, i, points.length - 1));
    ctx.lineWidth = Math.max(0.6, w);
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    if (profile.grain > 0) {
      ctx.globalAlpha = profile.opacity * (1 - profile.grain * 0.5 * noise(i, seed));
    }
    ctx.stroke();
  }

  ctx.restore();
}

/** 确定性伪随机（同一笔重绘结果一致，不闪烁）。 */
function noise(index: number, seed: number): number {
  const x = Math.sin(index * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

/** 预设笔刷：四支常用笔，颜色取调色板。 */
export function defaultPresets(): BrushPreset[] {
  return [
    { id: "pen-blue", label: "蓝 · 圆珠", brush: "ballpoint", color: "#2d6cdf", width: 3 },
    { id: "pen-black", label: "黑 · 圆珠", brush: "ballpoint", color: "#151b1f", width: 3 },
    { id: "marker-orange", label: "橙 · 马克", brush: "marker", color: "#f5a623", width: 9 },
    { id: "highlight-yellow", label: "黄 · 荧光", brush: "highlighter", color: "#e8c547", width: 18 },
    { id: "pencil-grey", label: "灰 · 铅笔", brush: "pencil", color: "#6b7280", width: 2.5 },
    { id: "pen-red", label: "红 · 圆珠", brush: "ballpoint", color: "#c0392b", width: 3 },
  ];
}
