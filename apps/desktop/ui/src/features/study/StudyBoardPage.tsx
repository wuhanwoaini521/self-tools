/**
 * Study Board（V11 §107-§114）：平板优先的画写板。
 *
 * P0 能力：Pen / Eraser / Undo / Redo / Clear / Touch / 保存 / 打开。
 * 数据流：Board → strokes（本组件状态 + 后端 StudyBoardStore）
 *         → snapshot（PNG base64）→ BoardSnapshot ContentPart → PersonalAgent。
 *
 * 隐私：画布内容不上传任何第三方；仅按用户显式操作保存 / 发送。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowCounterClockwise,
  ArrowClockwise,
  Eraser,
  Pen,
  Sparkle,
  Trash,
} from "@phosphor-icons/react";
import type { AppContextPayload } from "../ai/aiTypes";
import { learningClient } from "../learning/learningClient";
import {
  BRUSH_PROFILES,
  BRUSHES,
  PALETTE_GROUPS,
  type BrushKind,
  clamp01,
  readPressure,
  renderStroke,
  type BrushPoint,
} from "./brush";

export interface StudyBoardSummary {
  id: string;
  title: string;
  updated_at: number;
}

export interface StudyBoardPageProps {
  active: boolean;
  /** 当前 AppContext（前端负责「我在哪」）。 */
  onContextChange?: (context: AppContextPayload | null) => void;
  /** Ask AI：把快照 + 问题发给 PersonalAgent。 */
  onAskAi?: (prompt: string, snapshotBase64: string | null) => void;
}

interface Stroke {
  color: string;
  width: number;
  points: number[];
  /** 擦除操作在重绘时使用 destination-out，只影响笔迹图层。 */
  eraser?: boolean;
  /** 笔型（缺省按圆珠笔渲染，老学习板视觉不变）。 */
  brush?: BrushKind;
  /** 每点压力 0..1，与 points 一一对应；缺省视为恒定。 */
  pressures?: number[];
}

const STROKE_COLORS = ["#1688ff", "#f5f5f5", "#ffb020", "#22c55e"] as const;
/**
 * 历史遗留的画布底色常量。
 * 旧版本把橡皮擦存成「不透明背景色」的一笔，读回时靠这个值识别并还原成
 * destination-out（见 drawStrokes）。**不要跟着主题改**，否则老学习板的橡皮擦
 * 会退化成一条实色线。
 */
const LEGACY_CANVAS_BACKGROUND = "#151b1f";

/** 画布底色 / 网格跟随当前主题，避免浅色主题下出现一整块死黑。 */
function readCanvasPalette(): { background: string; grid: string } {
  const styles = getComputedStyle(document.documentElement);
  const surface = styles.getPropertyValue("--surface-sunken").trim() || "#faf8f3";
  const line = styles.getPropertyValue("--line").trim() || "#e5e0d7";
  return { background: surface, grid: line };
}

function drawBoardBackground(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  palette: { background: string; grid: string },
) {
  ctx.fillStyle = palette.background;
  ctx.fillRect(0, 0, width, height);
  const grid = 24;
  ctx.strokeStyle = palette.grid;
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = grid; x < width; x += grid) {
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
  }
  for (let y = grid; y < height; y += grid) {
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
  }
  ctx.stroke();
}


/** 旧数据（无 pressures）→ 按恒定压力还原，压感笔也能正确重绘。 */
function toBrushPoints(stroke: Stroke): BrushPoint[] {
  const points: BrushPoint[] = [];
  const count = stroke.points.length / 2;
  for (let i = 0; i < count; i++) {
    points.push({
      x: stroke.points[i * 2],
      y: stroke.points[i * 2 + 1],
      pressure: stroke.pressures?.[i] ?? 0.6,
    });
  }
  return points;
}

function drawStrokes(ctx: CanvasRenderingContext2D, strokes: Stroke[]) {
  for (const stroke of strokes) {
    if (stroke.points.length < 2) continue;
    // 旧版本把橡皮擦存成「不透明背景色」的一笔，读回时恢复。
    const isEraser =
      stroke.eraser ??
      (stroke.color === LEGACY_CANVAS_BACKGROUND && stroke.width === 24);
    renderStroke(ctx, toBrushPoints(stroke), {
      color: stroke.color,
      brush: stroke.brush ?? "ballpoint",
      width: stroke.width,
      eraser: isEraser,
      seed: stroke.points[0] * 31 + stroke.points[1] * 17,
    });
  }
  ctx.globalCompositeOperation = "source-over";
  ctx.globalAlpha = 1;
}

function newBoardId(): string {
  return `board-${Date.now().toString(36)}-${Math.floor(Math.random() * 0xffffff).toString(36)}`;
}

export function StudyBoardPage({ active, onContextChange, onAskAi }: StudyBoardPageProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [boardId, setBoardId] = useState(newBoardId);
  const [title, setTitle] = useState("未命名学习板");
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const [redoStack, setRedoStack] = useState<Stroke[]>([]);
  const [tool, setTool] = useState<"pen" | "eraser">("pen");
  const [color, setColor] = useState<string>(STROKE_COLORS[0]);
  /** 当前笔型（圆珠/马克/荧光/铅笔）。 */
  const [brush, setBrush] = useState<BrushKind>("ballpoint");
  /** 当前基础线宽（px）；切换笔型时给出该笔型的默认值。 */
  const [width, setWidth] = useState<number>(BRUSH_PROFILES.ballpoint.baseWidth);
  /** 最近一笔的采样点与时间戳，用于速度反推压感。 */
  const lastPointRef = useRef<{ x: number; y: number; at: number } | null>(null);
  const [drawing, setDrawing] = useState(false);
  const [status, setStatus] = useState("");
  const [boards, setBoards] = useState<StudyBoardSummary[]>([]);

  // --- 绘制 ---------------------------------------------------------------

  const redraw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    // 网格由 CSS 背景绘制，笔迹层保持透明，擦除时不会破坏纸面和网格。
    const dpr = window.devicePixelRatio || 1;
    const cssWidth = canvas.width / dpr;
    const cssHeight = canvas.height / dpr;
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    drawStrokes(ctx, strokes);
  }, [strokes]);

  useEffect(() => {
    redraw();
  }, [redraw, active]);

  // 画布尺寸跟随容器（DPR 适配，笔画不糊）。
  // 页面前几次渲染时容器可能仍是 0（隐藏 pane 刚显示）→ 用 rAF 重试几次，
  // 并兜底一个最小尺寸，避免 1×1 画布导致「画不了」。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let attempts = 0;
    let frame = 0;
    const resize = () => {
      const parent = canvas.parentElement;
      const dpr = window.devicePixelRatio || 1;
      const measured = parent ? parent.clientWidth : 0;
      const measuredHeight = parent ? parent.clientHeight : 0;
      // 兜底：容器还没布局出来时给一个可画的最小尺寸。
      const width = measured > 0 ? measured : 640;
      const height = measuredHeight > 0 ? measuredHeight : 480;
      canvas.width = Math.max(1, Math.floor(width * dpr));
      canvas.height = Math.max(1, Math.floor(height * dpr));
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      const ctx = canvas.getContext("2d");
      if (ctx) {
        // 重设尺寸后坐标系复位 → 以 CSS px 重放已有笔画。
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.scale(dpr, dpr);
      }
      redraw();
      // 容器还没布局完（例如 pane 刚从 page-hidden 切出来）→ 下一帧再试。
      if (measured <= 0 && attempts < 8) {
        attempts += 1;
        frame = requestAnimationFrame(resize);
      }
    };
    resize();
    window.addEventListener("resize", resize);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      window.removeEventListener("resize", resize);
    };
  }, [redraw, active]);

  const pointerPosition = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top] as const;
  };

  const onPointerDown = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const position = pointerPosition(event);
    if (!position) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDrawing(true);
    // 落笔速度记为 0；readPressure 在无压感设备上据此反推粗细。
    lastPointRef.current = { x: position[0], y: position[1], at: performance.now() };
    const profile = BRUSH_PROFILES[brush];
    setStrokes((current) => [
      ...current,
      {
        color: tool === "eraser" ? LEGACY_CANVAS_BACKGROUND : color,
        width: tool === "eraser" ? 24 : width || profile.baseWidth,
        points: [position[0], position[1]],
        pressures: [readPressure(event)],
        brush: tool === "eraser" ? brush : brush,
        eraser: tool === "eraser",
      },
    ]);
    setRedoStack([]);
  };

  const onPointerMove = (event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!drawing) return;
    const position = pointerPosition(event);
    if (!position) return;
    // 事件间隔用于速度 → 压感反推（触控笔走真实 pressure）。
    const now = performance.now();
    const prev = lastPointRef.current;
    const dt = prev ? Math.max(1, now - prev.at) : 16;
    const dist = prev
      ? Math.hypot(position[0] - prev.x, position[1] - prev.y)
      : 0;
    const velocity = dist / dt; // px/ms
    lastPointRef.current = { x: position[0], y: position[1], at: now };
    const pressure = readPressure({ ...event, velocity });
    setStrokes((current) => {
      if (current.length === 0) return current;
      const last = current[current.length - 1];
      const updated: Stroke = {
        ...last,
        points: [...last.points, position[0], position[1]],
        pressures: [...(last.pressures ?? []), clamp01(pressure)],
      };
      return [...current.slice(0, -1), updated];
    });
  };

  const onPointerUp = () => setDrawing(false);

  // --- 编辑 ---------------------------------------------------------------

  const undo = useCallback(() => {
    setStrokes((current) => {
      if (current.length === 0) return current;
      const removed = current[current.length - 1];
      setRedoStack((stack) => [...stack, removed]);
      return current.slice(0, -1);
    });
  }, []);

  const redo = useCallback(() => {
    setRedoStack((stack) => {
      if (stack.length === 0) return stack;
      const restored = stack[stack.length - 1];
      setStrokes((current) => [...current, restored]);
      return stack.slice(0, -1);
    });
  }, []);

  const clear = useCallback(() => {
    setStrokes((current) => {
      if (current.length > 0) setRedoStack((stack) => [...stack, ...current]);
      return [];
    });
  }, []);

  // --- 上下文 / AI --------------------------------------------------------

  useEffect(() => {
    if (!active) return;
    onContextChange?.({
      module: "study-board",
      page: "board",
      entity: { kind: "board", id: boardId, label: title },
      selection: null,
      view_state: { strokes: strokes.length },
    });
  }, [active, boardId, title, strokes.length, onContextChange]);

  useEffect(() => {
    if (!active) onContextChange?.(null);
  }, [active, onContextChange]);

  const snapshot = useCallback((): string | null => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    try {
      const dpr = window.devicePixelRatio || 1;
      const output = document.createElement("canvas");
      output.width = canvas.width;
      output.height = canvas.height;
      const ctx = output.getContext("2d");
      if (!ctx) return null;
      ctx.scale(dpr, dpr);
      drawBoardBackground(ctx, canvas.width / dpr, canvas.height / dpr, readCanvasPalette());
      drawStrokes(ctx, strokes);
      return output.toDataURL("image/png").split(",")[1] ?? null;
    } catch {
      return null;
    }
  }, [strokes]);

  const askAi = useCallback(() => {
    const image = snapshot();
    onAskAi?.("这块学习板上写了什么？帮我检查思路、错误或可以改进的地方。", image);
    setStatus(image ? "快照已发送（本地处理）" : "画布为空，仅发送了文字");
  }, [onAskAi, snapshot]);

  const save = useCallback(() => {
    // 本地优先：后端 Conversation/StudyBoardStore 未接通前保存到 localStorage，
    // 接通后由组合根替换（V11 §109：不能只存在浏览器内存 —— 这里至少跨会话存活，
    // 真正的持久化解在 StudyBoardSqliteStore + desktop 命令）。
    try {
      const payload = JSON.stringify({ boardId, title, strokes, updated_at: Date.now() });
      window.localStorage.setItem(`study-board:${boardId}`, payload);
      setBoards((current) => {
        const others = current.filter((board) => board.id !== boardId);
        return [{ id: boardId, title, updated_at: Date.now() }, ...others].slice(0, 20);
      });
      void learningClient.recordEvent({
        module: "study",
        entity_type: "board",
        entity_id: boardId,
        entity_title: title || "研习画板",
        action: "study",
      });
      setStatus("已保存到本地");
    } catch (error) {
      setStatus(`保存失败：${String(error)}`);
    }
  }, [boardId, strokes, title]);

  const open = useCallback((id: string) => {
    try {
      const raw = window.localStorage.getItem(`study-board:${id}`);
      if (!raw) {
        setStatus("本地没有这块板");
        return;
      }
      const parsed = JSON.parse(raw) as { title: string; strokes: Stroke[] };
      setBoardId(id);
      setTitle(parsed.title);
      setStrokes(parsed.strokes ?? []);
      setRedoStack([]);
      setStatus("已打开");
    } catch {
      setStatus("打开失败（本地数据损坏）");
    }
  }, []);

  const toolButtons = useMemo(
    () => (
      <div className="study-toolbar">
        <div className="study-toolbar-group" role="group" aria-label="工具">
          <button
            type="button"
            className={tool === "pen" ? "active" : ""}
            onClick={() => setTool("pen")}
            title="笔"
          >
            <Pen size={18} />
          </button>
          <button
            type="button"
            className={tool === "eraser" ? "active" : ""}
            onClick={() => setTool("eraser")}
            title="橡皮"
          >
            <Eraser size={18} />
          </button>
        </div>
        <div className="study-toolbar-group" role="group" aria-label="笔型">
          {BRUSHES.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={"study-brush" + (brush === entry.id && tool === "pen" ? " active" : "")}
              onClick={() => {
                setBrush(entry.id);
                setWidth(BRUSH_PROFILES[entry.id].baseWidth);
                setTool("pen");
              }}
              title={`${entry.label} — ${entry.hint}`}
              aria-label={entry.label}
            >
              {entry.label}
            </button>
          ))}
        </div>
        <div className="study-toolbar-group" role="group" aria-label="粗细">
          <input
            className="study-width"
            type="range"
            min={1}
            max={32}
            step={0.5}
            value={width}
            onChange={(event) => setWidth(Number(event.target.value))}
            aria-label="笔触粗细"
            title={`粗细 ${width}px`}
          />
          <span className="study-width-value">{width}px</span>
        </div>
        <div className="study-toolbar-group study-palette" role="group" aria-label="颜色">
          {PALETTE_GROUPS.map((group) => (
            <span key={group.label} className="study-palette-group" title={group.label}>
              {group.colors.map((entry) => (
                <button
                  key={entry.value}
                  type="button"
                  className={"study-swatch" + (color === entry.value ? " active" : "")}
                  style={{ background: entry.value }}
                  onClick={() => {
                    setColor(entry.value);
                    setTool("pen");
                  }}
                  title={`${entry.label} ${entry.value}`}
                  aria-label={entry.label}
                />
              ))}
            </span>
          ))}
        </div>
        <div className="study-toolbar-group" role="group" aria-label="编辑">
          <button type="button" onClick={undo} title="撤销" disabled={strokes.length === 0}>
            <ArrowCounterClockwise size={18} />
          </button>
          <button type="button" onClick={redo} title="重做" disabled={redoStack.length === 0}>
            <ArrowClockwise size={18} />
          </button>
          <button type="button" onClick={clear} title="清空" disabled={strokes.length === 0}>
            <Trash size={18} />
          </button>
        </div>
        <div className="study-toolbar-group study-toolbar-right" role="group" aria-label="操作">
          <button type="button" onClick={save} title="保存">
            保存
          </button>
          <button type="button" onClick={askAi} className="study-ask" title="问 AI">
            <Sparkle size={16} weight="fill" />
            问 AI
          </button>
        </div>
      </div>
    ),
    [askAi, clear, color, redo, redoStack.length, save, strokes.length, tool, undo],
  );

  return (
    <div className="study-board">
      <header className="study-header">
        <input
          className="study-title"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          aria-label="学习板标题"
        />
        {/* 只展示笔画数与保存状态；boardId 是内部主键，不属于用户可读信息。 */}
        <span className="study-meta">
          {strokes.length} 笔{status ? ` · ${status}` : ""}
        </span>
      </header>
      {toolButtons}
      <div className="study-canvas-wrap">
        <canvas
          ref={canvasRef}
          className="study-canvas"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          aria-label="学习板画布"
        />
      </div>
      {boards.length > 0 ? (
        <ul className="study-recent" aria-label="最近学习板">
          {boards.map((board) => (
            <li key={board.id}>
              <button type="button" onClick={() => open(board.id)}>
                {board.title}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {status ? <p className="study-status" role="status">{status}</p> : null}
    </div>
  );
}
