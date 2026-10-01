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

  // Force simulation & render loop
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

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
          ctx.strokeStyle = isHighlighted ? "#3b82f6" : "rgba(156, 163, 175, 0.35)";
          ctx.lineWidth = isHighlighted ? 2.5 : 1.2;
          ctx.stroke();

          // Label
          if (edge.label) {
            const midX = (source.x + target.x) / 2;
            const midY = (source.y + target.y) / 2;
            ctx.font = "10px sans-serif";
            ctx.fillStyle = isHighlighted ? "#2563eb" : "rgba(156, 163, 175, 0.7)";
            ctx.textAlign = "center";
            ctx.fillText(edge.label, midX, midY - 3);
          }
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

        // Node label
        ctx.font = isCenter ? "bold 13px sans-serif" : "11px sans-serif";
        ctx.fillStyle = isSelected ? "#1d4ed8" : "#1f2937";
        ctx.textAlign = "center";
        ctx.fillText(node.name, node.x, node.y + node.radius + 14);
      }

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
    <div style={{ position: "relative", width: "100%", height: "100vh", overflow: "hidden", background: "var(--surface-secondary, #f8fafc)" }}>
      {/* Top Toolbar */}
      <div
        style={{
          position: "absolute",
          top: 16,
          left: 20,
          right: 20,
          zIndex: 10,
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          background: "rgba(255, 255, 255, 0.9)",
          backdropFilter: "blur(12px)",
          borderRadius: 12,
          padding: "10px 18px",
          border: "1px solid var(--border-color, #e2e8f0)",
          boxShadow: "0 4px 12px rgba(0,0,0,0.04)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 700, fontSize: 16, color: "var(--text-primary, #1e293b)" }}>
            <Brain size={20} color="#2563eb" /> 跨模块知识图谱
          </div>

          {/* Hops selector */}
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
            <span style={{ color: "var(--text-secondary, #64748b)" }}>探索深度:</span>
            <select
              value={hops}
              onChange={(e) => setHops(Number(e.target.value))}
              style={{
                padding: "4px 8px",
                borderRadius: 6,
                border: "1px solid var(--border-color, #cbd5e1)",
                fontSize: 12,
                background: "#ffffff",
              }}
            >
              <option value={1}>1 跳 (直接关联)</option>
              <option value={2}>2 跳 (二级脉络)</option>
            </select>
          </div>

          {/* Type filter */}
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
            <span style={{ color: "var(--text-secondary, #64748b)" }}>实体类别:</span>
            <select
              value={selectedType}
              onChange={(e) => setSelectedType(e.target.value)}
              style={{
                padding: "4px 8px",
                borderRadius: 6,
                border: "1px solid var(--border-color, #cbd5e1)",
                fontSize: 12,
                background: "#ffffff",
              }}
            >
              <option value="all">全部类型</option>
              {Object.keys(TYPE_COLORS).map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* Legend */}
        <div style={{ display: "flex", alignItems: "center", gap: 10, fontSize: 11, color: "#64748b" }}>
          {Object.entries(TYPE_COLORS).slice(0, 5).map(([type, color]) => (
            <div key={type} style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <span style={{ width: 8, height: 8, borderRadius: "50%", background: color }} />
              <span style={{ textTransform: "capitalize" }}>{type}</span>
            </div>
          ))}
          <button
            onClick={loadGraph}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              padding: "6px 12px",
              borderRadius: 6,
              border: "1px solid var(--border-color, #cbd5e1)",
              background: "#ffffff",
              cursor: "pointer",
              fontSize: 12,
              fontWeight: 600,
            }}
          >
            <ArrowsCounterClockwise size={14} className={loading ? "spin" : ""} /> 重置视图
          </button>
        </div>
      </div>

      {/* Canvas */}
      <canvas
        ref={canvasRef}
        width={window.innerWidth}
        height={window.innerHeight}
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
