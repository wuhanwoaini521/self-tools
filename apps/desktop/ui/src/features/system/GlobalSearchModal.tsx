import React, { useEffect, useState, useRef } from "react";
import {
  MagnifyingGlass,
  Clock,
  Globe,
  Translate,
  BookOpen,
  Folder,
  ArrowRight,
  X,
  Sparkle,
} from "@phosphor-icons/react";
import { learningClient } from "../learning/learningClient";
import type { GlobalSearchItem, GlobalSearchResultGroup } from "../../types";

interface GlobalSearchModalProps {
  isOpen: boolean;
  onClose: () => void;
  onNavigate?: (route: string) => void;
  onAskAi?: (prompt: string) => void;
}

const MODULE_ICONS: Record<string, React.ReactNode> = {
  history: <Clock size={16} color="#f59e0b" />,
  geography: <Globe size={16} color="#10b981" />,
  language: <Translate size={16} color="#3b82f6" />,
  collection: <Folder size={16} color="#8b5cf6" />,
  study: <BookOpen size={16} color="#6366f1" />,
};

export function GlobalSearchModal({
  isOpen,
  onClose,
  onNavigate,
  onAskAi,
}: GlobalSearchModalProps) {
  const [query, setQuery] = useState("");
  const [groups, setGroups] = useState<GlobalSearchResultGroup[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Focus on open
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 50);
      setQuery("");
      setGroups([]);
      setSelectedIndex(0);
    }
  }, [isOpen]);

  // Debounced search
  useEffect(() => {
    if (!query.trim()) {
      setGroups([]);
      return;
    }
    const timer = setTimeout(async () => {
      setLoading(true);
      try {
        const res = await learningClient.globalSearch(query);
        setGroups(res);
        setSelectedIndex(0);
      } catch (err) {
        console.error("Global search failed:", err);
      } finally {
        setLoading(false);
      }
    }, 150);
    return () => clearTimeout(timer);
  }, [query]);

  // Flattened items for keyboard navigation
  const allItems: GlobalSearchItem[] = groups.flatMap((g) => g.items);

  const handleSelect = (item: GlobalSearchItem) => {
    onClose();
    if (item.action_target.startsWith("#")) {
      onNavigate?.(item.action_target);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % Math.max(1, allItems.length));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + allItems.length) % Math.max(1, allItems.length));
    } else if (e.key === "Enter" && allItems[selectedIndex]) {
      e.preventDefault();
      handleSelect(allItems[selectedIndex]);
    }
  };

  if (!isOpen) return null;

  let flatIndexCounter = 0;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        background: "rgba(15, 23, 42, 0.45)",
        backdropFilter: "blur(6px)",
        zIndex: 2000,
        display: "flex",
        justifyContent: "center",
        alignItems: "flex-start",
        paddingTop: "12vh",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: "100%",
          maxWidth: 620,
          background: "var(--surface-primary, #ffffff)",
          borderRadius: 14,
          boxShadow: "0 20px 40px rgba(0, 0, 0, 0.2)",
          border: "1px solid var(--border-color, #e2e8f0)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Search Input Bar */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            padding: "16px 20px",
            borderBottom: "1px solid var(--border-subtle, #e2e8f0)",
            gap: 12,
          }}
        >
          <MagnifyingGlass size={20} color="var(--text-tertiary, #94a3b8)" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="搜索全站历史、地理、词汇、合集或直接提问 AI..."
            style={{
              flex: 1,
              border: "none",
              outline: "none",
              fontSize: 16,
              color: "var(--text-primary, #0f172a)",
              background: "transparent",
            }}
          />
          {query && (
            <button
              onClick={() => setQuery("")}
              style={{ border: "none", background: "none", cursor: "pointer", color: "#94a3b8" }}
            >
              <X size={16} />
            </button>
          )}
          <span
            style={{
              fontSize: 11,
              fontWeight: 600,
              padding: "2px 6px",
              borderRadius: 4,
              background: "var(--surface-secondary, #f1f5f9)",
              color: "var(--text-tertiary, #64748b)",
            }}
          >
            ESC 退出
          </span>
        </div>

        {/* Results List */}
        <div style={{ maxHeight: 420, overflowY: "auto", padding: "12px 0" }}>
          {loading ? (
            <div style={{ padding: "32px 0", textAlign: "center", color: "#94a3b8", fontSize: 13 }}>
              搜索中...
            </div>
          ) : groups.length > 0 ? (
            groups.map((group) => (
              <div key={group.group_key} style={{ marginBottom: 12 }}>
                <div
                  style={{
                    padding: "6px 20px",
                    fontSize: 11,
                    fontWeight: 700,
                    textTransform: "uppercase",
                    color: "var(--text-tertiary, #94a3b8)",
                    letterSpacing: "0.05em",
                  }}
                >
                  {group.group_title}
                </div>
                {group.items.map((item) => {
                  const currentIndex = flatIndexCounter++;
                  const isSelected = currentIndex === selectedIndex;
                  return (
                    <div
                      key={item.id}
                      onClick={() => handleSelect(item)}
                      onMouseEnter={() => setSelectedIndex(currentIndex)}
                      style={{
                        padding: "10px 20px",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        cursor: "pointer",
                        background: isSelected ? "var(--surface-secondary, #f1f5f9)" : "transparent",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                        {MODULE_ICONS[item.module] ?? <BookOpen size={16} />}
                        <div>
                          <div style={{ fontSize: 14, fontWeight: 600, color: "var(--text-primary, #0f172a)" }}>
                            {item.title}
                          </div>
                          {item.subtitle && (
                            <div style={{ fontSize: 12, color: "var(--text-tertiary, #64748b)" }}>
                              {item.subtitle}
                            </div>
                          )}
                        </div>
                      </div>
                      <ArrowRight size={14} color={isSelected ? "#2563eb" : "#94a3b8"} />
                    </div>
                  );
                })}
              </div>
            ))
          ) : query.trim() ? (
            <div style={{ padding: "32px 20px", textAlign: "center" }}>
              <div style={{ fontSize: 14, color: "#64748b", marginBottom: 12 }}>未找到匹配结果</div>
              {onAskAi && (
                <button
                  onClick={() => {
                    onClose();
                    onAskAi(`请为我解答：${query}`);
                  }}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    padding: "8px 16px",
                    borderRadius: 8,
                    border: "none",
                    background: "rgba(139, 92, 246, 0.1)",
                    color: "#7c3aed",
                    fontSize: 13,
                    fontWeight: 600,
                    cursor: "pointer",
                  }}
                >
                  <Sparkle size={16} /> 向 Personal AI 提问 “{query}”
                </button>
              )}
            </div>
          ) : (
            <div style={{ padding: "24px 20px", color: "var(--text-tertiary, #94a3b8)", fontSize: 13, textAlign: "center" }}>
              输入关键词，跨模块快速检索历史事件、地理城市、语言词汇与合集
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
