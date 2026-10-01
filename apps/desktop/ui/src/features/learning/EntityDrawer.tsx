import React, { useState, useEffect } from "react";
import {
  X,
  Sparkle,
  ArrowSquareOut,
  FolderPlus,
  Brain,
  CheckCircle,
  Clock,
  Globe,
  Translate,
  BookOpen,
  Lightbulb,
  Tag,
  ShareNetwork,
} from "@phosphor-icons/react";
import { learningClient } from "./learningClient";
import type {
  Collection,
  GraphNode,
  LearningProgress,
} from "../../types";

interface EntityDrawerProps {
  node: GraphNode | null;
  onClose: () => void;
  onNavigate?: (route: string) => void;
  onSelectNode?: (nodeId: string) => void;
  onAskAi?: (prompt: string) => void;
}

const TYPE_COLORS: Record<string, string> = {
  person: "#ec4899",
  place: "#10b981",
  event: "#f59e0b",
  time: "#8b5cf6",
  concept: "#06b6d4",
  article: "#3b82f6",
  language: "#6366f1",
  topic: "#f97316",
};

export function EntityDrawer({
  node,
  onClose,
  onNavigate,
  onSelectNode,
  onAskAi,
}: EntityDrawerProps) {
  const [progress, setProgress] = useState<LearningProgress | null>(null);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [showAddToCollection, setShowAddToCollection] = useState(false);
  const [selectedCollectionId, setSelectedCollectionId] = useState<string>("");
  const [addingToCollection, setAddingToCollection] = useState(false);
  const [addedMessage, setAddedMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!node) return;
    const entityKey = `${node.module}:${node.entity_type}:${node.id}`;
    learningClient.getProgress(entityKey).then((p) => setProgress(p)).catch(() => {});
    learningClient.listCollections().then((c) => {
      setCollections(c);
      if (c.length > 0) setSelectedCollectionId(c[0].id);
    }).catch(() => {});
  }, [node]);

  if (!node) return null;

  const handleOpenOriginal = () => {
    onClose();
    if (node.module === "history") {
      if (node.entity_type === "person") {
        onNavigate?.(`#history?person=${node.id}`);
      } else if (node.entity_type === "event") {
        onNavigate?.(`#history?event=${node.id}`);
      } else {
        onNavigate?.(`#history?story=${node.id}`);
      }
    } else if (node.module === "geography") {
      onNavigate?.(`#geography?id=${node.id}`);
    } else if (node.module === "language") {
      onNavigate?.(`#language?id=${node.id}`);
    } else if (node.module === "news") {
      onNavigate?.(`#news`);
    } else if (node.module === "study") {
      onNavigate?.(`#study-board`);
    } else {
      onNavigate?.(`#${node.module}`);
    }
  };

  const handleAddToCollection = async () => {
    if (!selectedCollectionId || addingToCollection) return;
    setAddingToCollection(true);
    try {
      await learningClient.addCollectionItem(
        selectedCollectionId,
        node.module,
        node.entity_type,
        node.id,
        node.name,
        node.summary ?? undefined
      );
      setAddedMessage("已成功加入专题合集！");
      setTimeout(() => {
        setAddedMessage(null);
        setShowAddToCollection(false);
      }, 1500);
    } catch (err) {
      console.error("Failed to add to collection:", err);
    } finally {
      setAddingToCollection(false);
    }
  };

  const mastery = progress?.mastery_score ?? node.mastery_score ?? 0;
  const status = progress?.status ?? node.learning_status ?? "new";

  return (
    <div
      style={{
        position: "fixed",
        top: 0,
        right: 0,
        bottom: 0,
        width: 380,
        background: "var(--surface-primary, #ffffff)",
        boxShadow: "-4px 0 24px rgba(0, 0, 0, 0.12)",
        zIndex: 1000,
        display: "flex",
        flexDirection: "column",
        borderLeft: "1px solid var(--border-color, #e5e7eb)",
        animation: "slideInRight 0.25s ease-out",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "20px 24px",
          borderBottom: "1px solid var(--border-subtle, #f3f4f6)",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
        }}
      >
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
            <span
              style={{
                fontSize: 11,
                fontWeight: 700,
                textTransform: "uppercase",
                padding: "2px 8px",
                borderRadius: 4,
                background: TYPE_COLORS[node.entity_type] ?? "#6b7280",
                color: "#ffffff",
              }}
            >
              {node.entity_type}
            </span>
            <span
              style={{
                fontSize: 12,
                color: "var(--text-tertiary, #9ca3af)",
                textTransform: "capitalize",
              }}
            >
              {node.module}
            </span>
          </div>
          <h2 style={{ fontSize: 20, fontWeight: 700, color: "var(--text-primary, #111827)", margin: 0 }}>
            {node.name}
          </h2>
        </div>

        <button
          onClick={onClose}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            color: "var(--text-tertiary, #9ca3af)",
            padding: 4,
          }}
        >
          <X size={20} />
        </button>
      </div>

      {/* Body */}
      <div style={{ flex: 1, overflowY: "auto", padding: "20px 24px" }}>
        {/* Mastery progress */}
        <div
          style={{
            background: "var(--surface-secondary, #f9fafb)",
            borderRadius: 12,
            padding: "16px",
            marginBottom: 20,
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 8, fontSize: 13 }}>
            <span style={{ color: "var(--text-secondary, #6b7280)", fontWeight: 500 }}>掌握度等级</span>
            <span style={{ fontWeight: 700, color: mastery >= 80 ? "#10b981" : mastery >= 50 ? "#3b82f6" : "#f59e0b" }}>
              {mastery}% · {status}
            </span>
          </div>
          <div
            style={{
              height: 8,
              borderRadius: 4,
              background: "var(--border-subtle, #e5e7eb)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                height: "100%",
                width: `${mastery}%`,
                background: mastery >= 80 ? "#10b981" : mastery >= 50 ? "#3b82f6" : "#f59e0b",
                borderRadius: 4,
                transition: "width 0.3s ease",
              }}
            />
          </div>
        </div>

        {/* Summary */}
        {node.summary && (
          <div style={{ marginBottom: 20 }}>
            <div style={{ fontSize: 12, fontWeight: 600, color: "var(--text-tertiary, #9ca3af)", marginBottom: 6 }}>
              知识摘要
            </div>
            <p style={{ fontSize: 14, color: "var(--text-secondary, #374151)", lineHeight: 1.6, margin: 0 }}>
              {node.summary}
            </p>
          </div>
        )}

        {/* Add to collection section */}
        {showAddToCollection ? (
          <div
            style={{
              background: "rgba(59, 130, 246, 0.05)",
              border: "1px solid rgba(59, 130, 246, 0.2)",
              borderRadius: 10,
              padding: 14,
              marginBottom: 20,
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 600, color: "#2563eb", marginBottom: 8 }}>
              选择归档合集
            </div>
            {collections.length > 0 ? (
              <div style={{ display: "flex", gap: 8, flexDirection: "column" }}>
                <select
                  value={selectedCollectionId}
                  onChange={(e) => setSelectedCollectionId(e.target.value)}
                  style={{
                    padding: "8px 12px",
                    borderRadius: 6,
                    border: "1px solid var(--border-color, #d1d5db)",
                    fontSize: 13,
                    background: "var(--surface-primary, #ffffff)",
                  }}
                >
                  {collections.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.title} ({c.items_count} 项)
                    </option>
                  ))}
                </select>
                <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
                  <button
                    onClick={() => setShowAddToCollection(false)}
                    style={{
                      padding: "6px 12px",
                      borderRadius: 6,
                      border: "none",
                      background: "transparent",
                      color: "var(--text-secondary, #6b7280)",
                      fontSize: 12,
                      cursor: "pointer",
                    }}
                  >
                    取消
                  </button>
                  <button
                    disabled={addingToCollection}
                    onClick={handleAddToCollection}
                    style={{
                      padding: "6px 14px",
                      borderRadius: 6,
                      border: "none",
                      background: "#2563eb",
                      color: "#ffffff",
                      fontSize: 12,
                      fontWeight: 600,
                      cursor: "pointer",
                    }}
                  >
                    {addingToCollection ? "添加中..." : "确认加入"}
                  </button>
                </div>
                {addedMessage && (
                  <div style={{ fontSize: 12, color: "#10b981", textAlign: "center", marginTop: 4 }}>
                    {addedMessage}
                  </div>
                )}
              </div>
            ) : (
              <div style={{ fontSize: 12, color: "var(--text-tertiary, #9ca3af)" }}>
                暂无合集，请在“合集”页先创建合集。
              </div>
            )}
          </div>
        ) : null}
      </div>

      {/* Footer action buttons */}
      <div
        style={{
          padding: "16px 24px",
          borderTop: "1px solid var(--border-subtle, #f3f4f6)",
          display: "flex",
          flexDirection: "column",
          gap: 10,
        }}
      >
        <button
          onClick={handleOpenOriginal}
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 8,
            padding: "10px 16px",
            borderRadius: 8,
            border: "none",
            background: "var(--accent-primary, #2563eb)",
            color: "#ffffff",
            fontSize: 13,
            fontWeight: 600,
            cursor: "pointer",
          }}
        >
          <ArrowSquareOut size={16} /> 在原模块中查看
        </button>

        {onAskAi && (
          <button
            onClick={() => {
              onAskAi(`请结合上下文为我详细讲解“${node.name}”（属于 ${node.module} 模块中的 ${node.entity_type}）。`);
              onClose();
            }}
            style={{
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              gap: 8,
              padding: "10px 16px",
              borderRadius: 8,
              border: "1px solid rgba(139, 92, 246, 0.3)",
              background: "rgba(139, 92, 246, 0.08)",
              color: "#7c3aed",
              fontSize: 13,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            <Sparkle size={16} /> 向 AI 深入提问
          </button>
        )}

        <button
          onClick={() => setShowAddToCollection(true)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 8,
            padding: "8px 16px",
            borderRadius: 8,
            border: "1px solid var(--border-color, #e5e7eb)",
            background: "var(--surface-primary, #ffffff)",
            color: "var(--text-secondary, #4b5563)",
            fontSize: 13,
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          <FolderPlus size={16} /> 加入专题合集
        </button>
      </div>
    </div>
  );
}
