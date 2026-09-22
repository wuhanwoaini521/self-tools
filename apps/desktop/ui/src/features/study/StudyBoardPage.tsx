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
}

const STROKE_COLORS = ["#1688ff", "#f5f5f5", "#ffb020", "#22c55e"] as const;
const CANVAS_BACKGROUND = "#0d1315";

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
  const [drawing, setDrawing] = useState(false);
  const [status, setStatus] = useState("");
  const [boards, setBoards] = useState<StudyBoardSummary[]>([]);

  // --- 绘制 ---------------------------------------------------------------

  const redraw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = CANVAS_BACKGROUND;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    for (const stroke of strokes) {
      if (stroke.points.length < 2) continue;
      ctx.strokeStyle = stroke.color;
      ctx.lineWidth = stroke.width;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      ctx.moveTo(stroke.points[0], stroke.points[1]);
      for (let index = 2; index < stroke.points.length; index += 2) {
        ctx.lineTo(stroke.points[index], stroke.points[index + 1]);
      }
      ctx.stroke();
    }
  }, [strokes]);

  useEffect(() => {
    redraw();
  }, [redraw, active]);

  // 画布尺寸跟随容器（DPR 适配，笔画不糊）。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const resize = () => {
      const parent = canvas.parentElement;
      if (!parent) return;
      const dpr = window.devicePixelRatio || 1;
      const width = parent.clientWidth;
      const height = parent.clientHeight;
      canvas.width = Math.max(1, Math.floor(width * dpr));
      canvas.height = Math.max(1, Math.floor(height * dpr));
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      const ctx = canvas.getContext("2d");
      if (ctx) ctx.scale(dpr, dpr);
      // 重设尺寸后坐标系复位 → 以 CSS px 重放已有笔画。
      ctx?.setTransform(1, 0, 0, 1, 0, 0);
      if (ctx) ctx.scale(dpr, dpr);
      redraw();
    };
    resize();
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, [redraw]);

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
    setStrokes((current) => [
      ...current,
      {
        color: tool === "eraser" ? CANVAS_BACKGROUND : color,
        width: tool === "eraser" ? 24 : 3,
        points: [position[0], position[1]],
      },
    ]);
    setRedoStack([]);
  };

  const onPointerMove = (event: React.PointerEvent<HTMLCanvasElement>) => {
    if (!drawing) return;
    const position = pointerPosition(event);
    if (!position) return;
    setStrokes((current) => {
      if (current.length === 0) return current;
      const last = current[current.length - 1];
      const updated: Stroke = {
        ...last,
        points: [...last.points, position[0], position[1]],
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
      return canvas.toDataURL("image/png").split(",")[1] ?? null;
    } catch {
      return null;
    }
  }, []);

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
        <div className="study-toolbar-group" role="group" aria-label="颜色">
          {STROKE_COLORS.map((value) => (
            <button
              key={value}
              type="button"
              className={"study-swatch" + (color === value ? " active" : "")}
              style={{ background: value }}
              onClick={() => {
                setColor(value);
                setTool("pen");
              }}
              title={`颜色 ${value}`}
            />
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
        <span className="study-meta">
          {strokes.length} 笔 · {boardId.slice(0, 12)}
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
