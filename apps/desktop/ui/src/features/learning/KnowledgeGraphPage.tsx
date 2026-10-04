import React, { useEffect, useRef, useState, useCallback } from "react";
import {
  Brain,
  ArrowsCounterClockwise,
  MagnifyingGlass,
  Funnel,
  SlidersHorizontal,
  Sparkle,
  Plus,
  Minus,
  ArrowsIn,
  Clock,
  Globe,
  Translate,
} from "@phosphor-icons/react";
import { learningClient } from "./learningClient";
import { EntityDrawer } from "./EntityDrawer";
import type {
  GraphEdge,
  GraphNeighborhood,
  GraphNode,
} from "../../types";

interface KnowledgeGraphPageProps {
  active?: boolean;
  onNavigate?: (route: string) => void;
  onAskAi?: (prompt: string) => void;
  initialRootId?: string;
}

const TYPE_COLORS: Record<string, string> = {
  person: "#ec4899",
  place: "#10b981",
  event: "#f59e0b",
  time: "#8b5cf6",
  concept: "#06b6d4",
  article: "#3b82f6",
  language: "#6366f1",
  language_item: "#6366f1",
  word: "#6366f1",
  topic: "#f97316",
  destination: "#14b8a6",
};

/** 图例与筛选项的中文标签；后端未登记的类型回落到原始 key。 */
const ENTITY_TYPE_LABELS: Record<string, string> = {
  person: "人物",
  place: "地点",
  event: "事件",
  time: "年代",
  concept: "概念",
  article: "文章",
  language: "语言",
  language_item: "语言条目",
  word: "词条",
  topic: "专题",
  destination: "目的地",
};

/** 节点标签绘制在圆下方（node.y + radius + 14），适配视口时要预留出来。 */
const LABEL_ALLOWANCE = 22;

interface SimulationNode extends GraphNode {
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
}

export function KnowledgeGraphPage({
  active = true,
  onNavigate,
  onAskAi,
  initialRootId,
}: KnowledgeGraphPageProps) {
  const [neighborhood, setNeighborhood] = useState<GraphNeighborhood | null>(null);
  const [rootId, setRootId] = useState<string>(initialRootId ?? "");
  const [hops, setHops] = useState<number>(1);
  const [selectedType, setSelectedType] = useState<string>("all");
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [hoveredNode, setHoveredNode] = useState<GraphNode | null>(null);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const simNodesRef = useRef<SimulationNode[]>([]);
  const simEdgesRef = useRef<GraphEdge[]>([]);
  const animFrameRef = useRef<number | null>(null);
  const transformRef = useRef<{ x: number; y: number; k: number }>({ x: 0, y: 0, k: 1 });
  const draggingNodeRef = useRef<SimulationNode | null>(null);
  const isPanningRef = useRef<boolean>(false);
  const panStartRef = useRef<{ x: number; y: number }>({ x: 0, y: 0 });

  const loadGraph = useCallback(async () => {
    setLoading(true);
    try {
      const data = await learningClient.getGraph(rootId || undefined, hops);
      setNeighborhood(data);

      // Initialize simulation nodes with parent container dimensions
      const canvas = canvasRef.current;
      const rect = canvas?.parentElement?.getBoundingClientRect();
      const width = (rect && rect.width > 0) ? rect.width : (canvas?.width || 900);
      const height = (rect && rect.height > 0) ? rect.height : (canvas?.height || 600);
      if (canvas) {
        canvas.width = width;
        canvas.height = height;
      }
      const cx = width / 2;
      const cy = height / 2;

      const centerId = data.root_id || data.center?.id || data.nodes[0]?.id;

      const nodes: SimulationNode[] = data.nodes.map((n, i) => {
        const isCenter = n.id === centerId;
        const angle = (i / Math.max(1, data.nodes.length - 1)) * Math.PI * 2;
        const dist = isCenter ? 0 : 160 + (i % 3) * 60;
        return {
          ...n,
          x: cx + Math.cos(angle) * dist + (Math.random() - 0.5) * 20,
          y: cy + Math.sin(angle) * dist + (Math.random() - 0.5) * 20,
          vx: 0,
          vy: 0,
          radius: isCenter ? 26 : Math.max(16, Math.min(24, 14 + (n.degree ?? 1) * 2)),
        };
      });

      simNodesRef.current = nodes;
      simEdgesRef.current = data.edges;
      transformRef.current = { x: 0, y: 0, k: 1 };
    } catch (err) {
      console.error("Failed to load knowledge graph:", err);
    } finally {
      setLoading(false);
    }
  }, [rootId, hops]);

  useEffect(() => {
    if (active) {
      loadGraph();
    }
  }, [active, loadGraph]);

  useEffect(() => {
    const handleResize = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const rect = canvas.parentElement?.getBoundingClientRect();
      if (rect && rect.width > 0 && rect.height > 0) {
        canvas.width = rect.width;
        canvas.height = rect.height;
      }
    };
    handleResize();
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [active]);

  /**
   * 把力导向布局收敛后的节点缩放平移到画布可视区。
   * 布局只关心相对距离（弹簧 140px / 斥力 ~100px），在大画布上会缩成中心一小团、
   * 标签互相压盖；这里按节点包围盒求 scale + offset，让它铺满可视区。
   */
  const fitViewToNodes = useCallback(() => {
    const canvas = canvasRef.current;
    const nodes = simNodesRef.current;
    if (!canvas || nodes.length === 0) return;

    const width = canvas.width;
    const height = canvas.height;
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const node of nodes) {
      // 标签画在节点下方（y + radius + ~16px），下边界要把它一起框进来，
      // 否则最下方的节点会被画布底边裁掉。
      minX = Math.min(minX, node.x - node.radius);
      minY = Math.min(minY, node.y - node.radius);
      maxX = Math.max(maxX, node.x + node.radius);
      maxY = Math.max(maxY, node.y + node.radius + LABEL_ALLOWANCE);
    }
    if (!Number.isFinite(minX) || !Number.isFinite(minY)) return;

    // 预留：顶部工具条高度、节点标签行高与外边距。
    const padding = 64;
    const topInset = 76;
    const bottomInset = 44;
    const graphW = Math.max(1, maxX - minX);
    const graphH = Math.max(1, maxY - minY);
    const usableW = Math.max(1, width - padding * 2);
    const usableH = Math.max(1, height - topInset - bottomInset);

    const k = Math.min(2.2, Math.max(0.35, Math.min(usableW / graphW, usableH / graphH)));
    const graphCX = (minX + maxX) / 2;
    const graphCY = (minY + maxY) / 2;
    const viewCX = width / 2;
    const viewCY = topInset + usableH / 2;

    transformRef.current = { x: viewCX - graphCX * k, y: viewCY - graphCY * k, k };
  }, []);

  // 力导向稳定后再适配一次，保证首次进入就是铺满的视图。
  useEffect(() => {
    if (!active) return;
    let fitTimer = 0;
    const settle = window.setTimeout(() => {
      fitTimer = window.setTimeout(() => fitViewToNodes(), 900);
    }, 250);
    return () => {
      window.clearTimeout(settle);
      window.clearTimeout(fitTimer);
    };
  }, [active, neighborhood, fitViewToNodes]);

  // Force simulation & render loop
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Canvas 绘制不吃 CSS 变量，这里按当前主题取前景色，保证暗色模式可读。
    const styles = getComputedStyle(document.documentElement);
    const textColor = styles.getPropertyValue("--text").trim() || "#1f2937";
    const mutedColor = styles.getPropertyValue("--muted").trim() || "#9ca3af";
    const accentColor = styles.getPropertyValue("--accent").trim() || "#3b82f6";

    let running = true;

    const render = () => {
      if (!running) return;

      const width = canvas.width || 900;
      const height = canvas.height || 600;
      const cx = width / 2;
      const cy = height / 2;

      const nodes = simNodesRef.current;
      const edges = simEdgesRef.current;
      const nodeMap = new Map<string, SimulationNode>(nodes.map((n) => [n.id, n]));

      // Physics step
      for (let i = 0; i < nodes.length; i++) {
        const n1 = nodes[i];
        if (draggingNodeRef.current === n1) continue;

        // Centering force
        n1.vx += (cx - n1.x) * 0.001;
        n1.vy += (cy - n1.y) * 0.001;

        // Repulsion between nodes
        for (let j = i + 1; j < nodes.length; j++) {
          const n2 = nodes[j];
          const dx = n2.x - n1.x;
          const dy = n2.y - n1.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          const minDist = n1.radius + n2.radius + 60;
          if (dist < minDist * 2.5) {
            const force = (minDist - dist) / dist * 0.05;
            n1.vx -= dx * force;
            n1.vy -= dy * force;
            if (draggingNodeRef.current !== n2) {
              n2.vx += dx * force;
              n2.vy += dy * force;
            }
          }
        }
      }

      // Spring force on edges
      for (const edge of edges) {
        const sId = edge.source_id || edge.source;
        const tId = edge.target_id || edge.target;
        const source = sId ? nodeMap.get(sId) : undefined;
        const target = tId ? nodeMap.get(tId) : undefined;
        if (source && target) {
          const dx = target.x - source.x;
          const dy = target.y - source.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          const desiredDist = 140;
          const force = (dist - desiredDist) * 0.005;
          if (draggingNodeRef.current !== source) {
            source.vx += (dx / dist) * force;
            source.vy += (dy / dist) * force;
          }
          if (draggingNodeRef.current !== target) {
            target.vx -= (dx / dist) * force;
            target.vy -= (dy / dist) * force;
          }
        }
      }

      // Apply velocity and damping
      for (const n of nodes) {
        if (draggingNodeRef.current === n) continue;
        n.vx *= 0.88;
        n.vy *= 0.88;
        n.x += n.vx;
        n.y += n.vy;
      }

      // Draw canvas
      ctx.clearRect(0, 0, width, height);
      ctx.save();

      const { x: tx, y: ty, k } = transformRef.current;
      ctx.translate(tx, ty);
      ctx.scale(k, k);

      // Draw Edges
      for (const edge of edges) {
        const sId = edge.source_id || edge.source;
        const tId = edge.target_id || edge.target;
        const source = sId ? nodeMap.get(sId) : undefined;
        const target = tId ? nodeMap.get(tId) : undefined;
        if (source && target) {
          const isHighlighted =
            (hoveredNode && (source.id === hoveredNode.id || target.id === hoveredNode.id)) ||
            (selectedNode && (source.id === selectedNode.id || target.id === selectedNode.id));

          ctx.beginPath();
          ctx.moveTo(source.x, source.y);
          ctx.lineTo(target.x, target.y);
          ctx.strokeStyle = isHighlighted ? accentColor : mutedColor;
          ctx.lineWidth = isHighlighted ? 2.5 : 1.2;
          ctx.stroke();

          // 边标签在线中间的**屏幕坐标**下画（见下方 screen-space pass），
          // 这里只记录，避免跟随 scale 放大后堆叠成一团。
        }
      }

      // Draw Nodes
      const centerId = neighborhood?.root_id || neighborhood?.center?.id || nodes[0]?.id;
      for (const node of nodes) {
        if (selectedType !== "all" && node.entity_type !== selectedType) {
          continue;
        }

        const isCenter = node.id === centerId;
        const isSelected = selectedNode?.id === node.id;
        const isHovered = hoveredNode?.id === node.id;
        const color = TYPE_COLORS[node.entity_type] ?? "#6b7280";

        // Outer glow on hover or selected
        if (isSelected || isHovered) {
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius + 6, 0, Math.PI * 2);
          ctx.fillStyle = isSelected ? "rgba(37, 99, 235, 0.25)" : "rgba(59, 130, 246, 0.15)";
          ctx.fill();
        }

        // Node circle
        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();

        if (isCenter) {
          ctx.lineWidth = 3;
          ctx.strokeStyle = "#ffffff";
          ctx.stroke();
        }

        // Mastery badge outline
        if (node.mastery_score && node.mastery_score > 0) {
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius + 2, 0, (Math.PI * 2 * (node.mastery_score / 100)));
          ctx.strokeStyle = "#10b981";
          ctx.lineWidth = 2.5;
          ctx.stroke();
        }

      }

      // 节点标签在**屏幕坐标**下绘制（不随 canvas scale 放大），并做贪心避让：
      // 1) 标签若跟随缩放变换，k>1.5 时 11px 会变成 17px+ 并互相压盖；
      // 2) 相邻节点的标签框常常重叠，中心 / 选中 / 悬停的节点优先显示，其余跳过。

      const placedLabels: Array<{ x1: number; y1: number; x2: number; y2: number }> = [];
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      // 中心 / 选中 / 悬停的节点先画，保证它们的标签不被避让逻辑丢掉。
      const ordered = [...nodes].sort(
        (a, b) => Number(b.id === centerId) - Number(a.id === centerId),
      );
      for (const node of ordered) {
        if (selectedType !== "all" && node.entity_type !== selectedType) continue;
        const sx = node.x * k + tx;
        const sy = node.y * k + ty;
        const screenR = node.radius * k;
        const isCenter = node.id === centerId;
        const isActive = isCenter || node.id === selectedNode?.id || node.id === hoveredNode?.id;
        ctx.font = isCenter ? "600 13px sans-serif" : "11px sans-serif";
        const text = node.name.length > 14 ? node.name.slice(0, 13) + "…" : node.name;
        const tw = ctx.measureText(text).width;
        const x1 = sx - tw / 2 - 2;
        const x2 = sx + tw / 2 + 2;
        const y1 = sy + screenR + 4;
        const y2 = y1 + 14;
        const collides = placedLabels.some((r) => !(x2 < r.x1 || x1 > r.x2 || y2 < r.y1 || y1 > r.y2));
        if (collides && !isActive) continue;
        placedLabels.push({ x1, y1, x2, y2 });
        if (y2 > height) continue;
        ctx.fillStyle = isActive ? accentColor : textColor;
        ctx.fillText(text, sx, y1);
      }

      // 边标签：只在悬停/选中节点的关联边上显示。全量显示时十几条「脉络关联」
      // 会叠在图中央，既不可读也没有信息量。
      const focusId = hoveredNode?.id ?? selectedNode?.id;
      if (focusId) {
        ctx.font = "10px sans-serif";
        for (const edge of edges) {
          if (!edge.label) continue;
          const sId = edge.source_id || edge.source;
          const tId = edge.target_id || edge.target;
          if (sId !== focusId && tId !== focusId) continue;
          const source = sId ? nodeMap.get(sId) : undefined;
          const target = tId ? nodeMap.get(tId) : undefined;
          if (!source || !target) continue;
          const mx = ((source.x + target.x) / 2) * k + tx;
          const my = ((source.y + target.y) / 2) * k + ty;
          const tw = ctx.measureText(edge.label).width;
          const x1 = mx - tw / 2 - 3;
          const x2 = mx + tw / 2 + 3;
          const y1 = my - 12;
          const y2 = my + 2;
          if (placedLabels.some((r) => !(x2 < r.x1 || x1 > r.x2 || y2 < r.y1 || y1 > r.y2))) continue;
          placedLabels.push({ x1, y1, x2, y2 });
          ctx.fillStyle = accentColor;
          ctx.fillText(edge.label, mx, y1);
        }
      }
      ctx.restore();
      ctx.restore();
      animFrameRef.current = requestAnimationFrame(render);
    };

    render();

    return () => {
      running = false;
      if (animFrameRef.current) cancelAnimationFrame(animFrameRef.current);
    };
  }, [neighborhood, selectedType, selectedNode, hoveredNode]);

  // Handle Canvas mouse interaction
  const getCanvasCoords = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const x = (e.clientX - rect.left - transformRef.current.x) / transformRef.current.k;
    const y = (e.clientY - rect.top - transformRef.current.y) / transformRef.current.k;
    return { x, y };
  };

  const findNodeAt = (x: number, y: number): SimulationNode | null => {
    for (const n of simNodesRef.current) {
      const dx = n.x - x;
      const dy = n.y - y;
      if (Math.sqrt(dx * dx + dy * dy) <= n.radius + 4) {
        return n;
      }
    }
    return null;
  };

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const coords = getCanvasCoords(e);
    const clickedNode = findNodeAt(coords.x, coords.y);
    if (clickedNode) {
      draggingNodeRef.current = clickedNode;
      setSelectedNode(clickedNode);
    } else {
      isPanningRef.current = true;
      panStartRef.current = { x: e.clientX - transformRef.current.x, y: e.clientY - transformRef.current.y };
    }
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (draggingNodeRef.current) {
      const coords = getCanvasCoords(e);
      draggingNodeRef.current.x = coords.x;
      draggingNodeRef.current.y = coords.y;
      draggingNodeRef.current.vx = 0;
      draggingNodeRef.current.vy = 0;
    } else if (isPanningRef.current) {
      transformRef.current.x = e.clientX - panStartRef.current.x;
      transformRef.current.y = e.clientY - panStartRef.current.y;
    } else {
      const coords = getCanvasCoords(e);
      const hovered = findNodeAt(coords.x, coords.y);
      setHoveredNode(hovered);
    }
  };

  const handleMouseUp = () => {
    draggingNodeRef.current = null;
    isPanningRef.current = false;
  };

  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const zoomFactor = e.deltaY < 0 ? 1.1 : 0.9;
    const newK = Math.max(0.3, Math.min(3, transformRef.current.k * zoomFactor));
    transformRef.current.k = newK;
  };

  return (
    <div style={{ position: "relative", width: "100%", height: "100%", minHeight: 0, overflow: "hidden", background: "var(--bg)" }}>

      {/* Top Toolbar */}
      <div
        style={{
          position: "absolute",
          top: 12,
          left: 16,
          right: 16,
          zIndex: 10,
          display: "flex",
          flexWrap: "wrap",
          rowGap: 8,
          justifyContent: "space-between",
          alignItems: "center",
          background: "var(--panel)",
          borderRadius: 12,
          padding: "10px 16px",
          border: "1px solid var(--line)",
          boxShadow: "0 4px 12px rgba(0,0,0,0.04)",
        }}
      >
        <div style={{ display: "flex", flexWrap: "wrap", rowGap: 8, alignItems: "center", gap: 12 }}>
          {/* 统一页面骨架的「标题 + 操作位」：此前本页没有任何页面级标题 */}
          <div className="page-shell-title" style={{ marginRight: "auto" }}>
            <span className="page-shell-eyebrow">graph</span>
            <h1 style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <Brain size={18} color="var(--accent)" /> 跨模块知识图谱
            </h1>
          </div>

          {/* Hops selector */}
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
            <span style={{ color: "var(--muted)", whiteSpace: "nowrap" }}>探索深度</span>
            <select
              value={hops}
              onChange={(e) => setHops(Number(e.target.value))}
              style={{
                padding: "4px 8px",
                borderRadius: 6,
                border: "1px solid var(--line)",
                fontSize: 12,
                color: "var(--text)",
                background: "var(--panel-raised)",
              }}
            >
              <option value={1}>1 跳（直接关联）</option>
              <option value={2}>2 跳（二级脉络）</option>
            </select>
          </div>

          {/* Type filter */}
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
            <span style={{ color: "var(--muted)", whiteSpace: "nowrap" }}>实体类别</span>
            <select
              value={selectedType}
              onChange={(e) => setSelectedType(e.target.value)}
              style={{
                padding: "4px 8px",
                borderRadius: 6,
                border: "1px solid var(--line)",
                fontSize: 12,
                color: "var(--text)",
                background: "var(--panel-raised)",
              }}
            >
              <option value="all">全部类型</option>
              {Object.keys(TYPE_COLORS).map((t) => (
                <option key={t} value={t}>
                  {ENTITY_TYPE_LABELS[t] ?? t}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* Legend */}
        <div style={{ display: "flex", flexWrap: "wrap", rowGap: 6, alignItems: "center", gap: 10, fontSize: 11, color: "var(--muted)" }}>
          {Object.entries(TYPE_COLORS).slice(0, 5).map(([type, color]) => (
            <div key={type} style={{ display: "flex", alignItems: "center", gap: 4, whiteSpace: "nowrap" }}>
              <span style={{ width: 8, height: 8, borderRadius: "50%", background: color }} />
              <span>{ENTITY_TYPE_LABELS[type] ?? type}</span>
            </div>
          ))}
          <button
            onClick={() => {
              void loadGraph();
              fitViewToNodes();
            }}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              padding: "6px 12px",
              borderRadius: 6,
              border: "1px solid var(--line)",
              background: "var(--panel-raised)",
              color: "var(--text)",
              cursor: "pointer",
              fontSize: 12,
              fontWeight: 600,
              whiteSpace: "nowrap",
            }}
          >
            <ArrowsCounterClockwise size={14} className={loading ? "spin" : ""} /> 重置视图
          </button>
        </div>
      </div>

      {/* Canvas */}
      <canvas
        ref={canvasRef}
        width={900}
        height={600}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onWheel={handleWheel}
        style={{ width: "100%", height: "100%", cursor: isPanningRef.current ? "grabbing" : "grab" }}
      />

      {/* Entity Drawer Inspector */}
      {selectedNode && (
        <EntityDrawer
          node={selectedNode}
          onClose={() => setSelectedNode(null)}
          onNavigate={onNavigate}
          onSelectNode={(id) => {
            setRootId(id);
            setSelectedNode(null);
          }}
          onAskAi={onAskAi}
        />
      )}
    </div>
  );
}
