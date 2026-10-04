import React, { useEffect, useState, useCallback } from "react";
import {
  Folder,
  FolderSimple,
  Plus,
  Trash,
  Sparkle,
  ArrowSquareOut,
  Tag,
  ArrowsCounterClockwise,
  BookOpen,
  Globe,
  Clock,
  Translate,
  Lightbulb,
  X,
} from "@phosphor-icons/react";
import { learningClient } from "./learningClient";
import type { Collection, CollectionItem } from "../../types";

interface CollectionsPageProps {
  onNavigate?: (route: string) => void;
  onAskAi?: (prompt: string) => void;
}

const MODULE_ICONS: Record<string, React.ReactNode> = {
  history: <Clock size={16} color="#f59e0b" />,
  geography: <Globe size={16} color="#10b981" />,
  language: <Translate size={16} color="#3b82f6" />,
  study: <BookOpen size={16} color="#8b5cf6" />,
  news: <Lightbulb size={16} color="#f43f5e" />,
};

export function CollectionsPage({ onNavigate, onAskAi }: CollectionsPageProps) {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [selectedCollection, setSelectedCollection] = useState<Collection | null>(null);
  const [items, setItems] = useState<CollectionItem[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [showCreateModal, setShowCreateModal] = useState<boolean>(false);
  const [newTitle, setNewTitle] = useState<string>("");
  const [newDescription, setNewDescription] = useState<string>("");
  const [newTags, setNewTags] = useState<string>("");

  const loadCollections = useCallback(async () => {
    setLoading(true);
    try {
      const list = await learningClient.listCollections();
      setCollections(list);
      if (list.length > 0 && !selectedCollection) {
        setSelectedCollection(list[0]);
      }
    } catch (err) {
      console.error("Failed to load collections:", err);
    } finally {
      setLoading(false);
    }
  }, [selectedCollection]);

  useEffect(() => {
    loadCollections();
  }, [loadCollections]);

  useEffect(() => {
    if (!selectedCollection) {
      setItems([]);
      return;
    }
    learningClient
      .listCollectionItems(selectedCollection.id)
      .then((data) => setItems(data))
      .catch((err) => console.error("Failed to list collection items:", err));
  }, [selectedCollection]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newTitle.trim()) return;
    try {
      const tags = newTags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      const created = await learningClient.createCollection(
        newTitle.trim(),
        newDescription.trim() || undefined,
        tags
      );
      setCollections((prev) => [created, ...prev]);
      setSelectedCollection(created);
      setShowCreateModal(false);
      setNewTitle("");
      setNewDescription("");
      setNewTags("");
    } catch (err) {
      console.error("Failed to create collection:", err);
    }
  };

  const handleDeleteCollection = async (collectionId: string) => {
    if (!confirm("确定要删除此合集吗？")) return;
    try {
      await learningClient.deleteCollection(collectionId);
      const remaining = collections.filter((c) => c.id !== collectionId);
      setCollections(remaining);
      setSelectedCollection(remaining[0] ?? null);
    } catch (err) {
      console.error("Failed to delete collection:", err);
    }
  };

  const handleRemoveItem = async (itemId: string) => {
    try {
      await learningClient.removeCollectionItem(itemId);
      setItems((prev) => prev.filter((it) => it.id !== itemId));
      if (selectedCollection) {
        setSelectedCollection({
          ...selectedCollection,
          items_count: Math.max(0, selectedCollection.items_count - 1),
        });
      }
    } catch (err) {
      console.error("Failed to remove item:", err);
    }
  };

  const handleOpenItem = (item: CollectionItem) => {
    if (item.module === "history") {
      if (item.entity_type === "person") onNavigate?.(`#history?person=${item.entity_id}`);
      else if (item.entity_type === "event") onNavigate?.(`#history?event=${item.entity_id}`);
      else onNavigate?.(`#history?story=${item.entity_id}`);
    }
    else if (item.module === "geography") onNavigate?.(`#geography?id=${item.entity_id}`);
    else if (item.module === "language") onNavigate?.(`#language?id=${item.entity_id}`);
    else if (item.module === "news") onNavigate?.(`#news`);
    else if (item.module === "study") onNavigate?.(`#study-board`);
    else onNavigate?.(`#${item.module}`);
  };

  return (
    <div className="collections-shell page-shell" style={{ display: "flex", height: "100%", overflow: "hidden", padding: 0 }}>
      {/* Left Sidebar: Collection List */}
      <div
        style={{
          width: 320,
          borderRight: "1px solid var(--border-color, #e5e7eb)",
          background: "var(--surface-secondary, #f9fafb)",
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div
          style={{
            padding: "20px 20px 14px",
            borderBottom: "1px solid var(--border-subtle, #e5e7eb)",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <div style={{ fontSize: 16, fontWeight: 700, color: "var(--text-primary, #111827)" }}>
            专题合集 ({collections.length})
          </div>
          <button
            onClick={() => setShowCreateModal(true)}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              padding: "6px 12px",
              borderRadius: 6,
              border: "none",
              background: "var(--accent-primary, #2563eb)",
              color: "#ffffff",
              fontSize: 12,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            <Plus size={14} /> 新建合集
          </button>
        </div>

        <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
          {collections.length === 0 ? (
            /* 列表为空时不要留一块空白：给出与右侧一致的引导文案。 */
            <div
              style={{
                padding: "28px 16px",
                textAlign: "center",
                color: "var(--text-tertiary, #9ca3af)",
                fontSize: 12,
                lineHeight: 1.7,
              }}
            >
              列表为空
              <br />
              从右上角新建第一个合集
            </div>
          ) : null}
          {collections.map((col) => {
            const isSelected = selectedCollection?.id === col.id;
            return (
              <div
                key={col.id}
                onClick={() => setSelectedCollection(col)}
                style={{
                  padding: "12px 16px",
                  borderRadius: 8,
                  marginBottom: 8,
                  cursor: "pointer",
                  background: isSelected ? "var(--surface-primary, #ffffff)" : "transparent",
                  border: isSelected
                    ? "1px solid var(--accent-primary, #2563eb)"
                    : "1px solid transparent",
                  boxShadow: isSelected ? "0 2px 8px rgba(0,0,0,0.04)" : "none",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <div style={{ fontWeight: 600, fontSize: 14, color: "var(--text-primary, #111827)" }}>
                    {col.title}
                  </div>
                  <span style={{ fontSize: 11, color: "var(--text-tertiary, #9ca3af)", fontWeight: 500 }}>
                    {col.items_count} 项
                  </span>
                </div>
                {col.description && (
                  <p style={{ fontSize: 12, color: "var(--text-secondary, #6b7280)", margin: "4px 0 0", lineClamp: 2 }}>
                    {col.description}
                  </p>
                )}
                {col.tags.length > 0 && (
                  <div style={{ display: "flex", gap: 4, marginTop: 6, flexWrap: "wrap" }}>
                    {col.tags.map((t) => (
                      <span
                        key={t}
                        style={{
                          fontSize: 10,
                          padding: "1px 6px",
                          borderRadius: 4,
                          background: "rgba(59, 130, 246, 0.1)",
                          color: "#2563eb",
                        }}
                      >
                        #{t}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Right Content Area */}
      <div style={{ flex: 1, background: "var(--surface-primary, #ffffff)", overflowY: "auto", padding: "32px 40px" }}>
        {selectedCollection ? (
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 24 }}>
              <div>
                <h1 style={{ fontSize: 24, fontWeight: 700, color: "var(--text-primary, #111827)", margin: "0 0 8px" }}>
                  {selectedCollection.title}
                </h1>
                {selectedCollection.description && (
                  <p style={{ color: "var(--text-secondary, #6b7280)", fontSize: 14, margin: "0 0 12px", maxWidth: 640 }}>
                    {selectedCollection.description}
                  </p>
                )}
                <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <span style={{ fontSize: 12, color: "var(--text-tertiary, #9ca3af)" }}>
                    共收录 {items.length} 个跨模块知识条目
                  </span>
                  {onAskAi && (
                    <button
                      onClick={() =>
                        onAskAi(
                          `请针对专题合集《${selectedCollection.title}》（包含 ${items.map((i) => i.title).join("、")}）进行综合知识串讲与脉络梳理。`
                        )
                      }
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 4,
                        padding: "4px 10px",
                        borderRadius: 6,
                        border: "1px solid rgba(139, 92, 246, 0.3)",
                        background: "rgba(139, 92, 246, 0.08)",
                        color: "#7c3aed",
                        fontSize: 12,
                        fontWeight: 600,
                        cursor: "pointer",
                      }}
                    >
                      <Sparkle size={14} /> AI 综合串讲
                    </button>
                  )}
                </div>
              </div>

              <button
                onClick={() => handleDeleteCollection(selectedCollection.id)}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  padding: "6px 12px",
                  borderRadius: 6,
                  border: "1px solid var(--border-color, #e5e7eb)",
                  background: "transparent",
                  color: "#ef4444",
                  fontSize: 12,
                  cursor: "pointer",
                }}
              >
                <Trash size={14} /> 删除合集
              </button>
            </div>

            {/* Items Grid */}
            {items.length === 0 ? (
              <div
                style={{
                  textAlign: "center",
                  padding: "80px 0",
                  color: "var(--text-tertiary, #9ca3af)",
                  background: "var(--surface-secondary, #f9fafb)",
                  borderRadius: 12,
                }}
              >
                <Folder size={40} style={{ marginBottom: 12 }} />
                <div>合集中暂无内容</div>
                <div style={{ fontSize: 13, marginTop: 4 }}>
                  可以在知识图谱或今日主页中点击“加入专题合集”将历史、地理、语言、新闻条目归档至此。
                </div>
              </div>
            ) : (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))", gap: 16 }}>
                {items.map((item) => (
                  <div
                    key={item.id}
                    style={{
                      border: "1px solid var(--border-color, #e5e7eb)",
                      borderRadius: 12,
                      padding: 16,
                      background: "var(--surface-primary, #ffffff)",
                      display: "flex",
                      flexDirection: "column",
                      justifyContent: "space-between",
                    }}
                  >
                    <div>
                      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                          {MODULE_ICONS[item.module] ?? <BookOpen size={16} />}
                          <span style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", color: "var(--text-tertiary, #9ca3af)" }}>
                            {item.module} · {item.entity_type}
                          </span>
                        </div>
                        <button
                          onClick={() => handleRemoveItem(item.id)}
                          style={{
                            border: "none",
                            background: "transparent",
                            color: "var(--text-tertiary, #9ca3af)",
                            cursor: "pointer",
                            padding: 2,
                          }}
                        >
                          <X size={14} />
                        </button>
                      </div>

                      <h3 style={{ fontSize: 16, fontWeight: 600, color: "var(--text-primary, #111827)", margin: "0 0 6px" }}>
                        {item.title}
                      </h3>

                      {item.note && (
                        <p style={{ fontSize: 13, color: "var(--text-secondary, #4b5563)", margin: 0 }}>
                          {item.note}
                        </p>
                      )}
                    </div>

                    <div style={{ marginTop: 14, paddingTop: 10, borderTop: "1px solid var(--border-subtle, #f3f4f6)" }}>
                      <button
                        onClick={() => handleOpenItem(item)}
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 4,
                          fontSize: 12,
                          fontWeight: 600,
                          color: "var(--accent-primary, #2563eb)",
                          background: "none",
                          border: "none",
                          cursor: "pointer",
                          padding: 0,
                        }}
                      >
                        <ArrowSquareOut size={14} /> 查看详情
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        ) : (
          /* 未选中合集时，右侧不能是一整片空白——那看起来像坏了，
             而不是「还没有内容」。这里给出占满内容区的空态与下一步。 */
          <div
            style={{
              height: "100%",
              minHeight: 420,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              gap: 12,
              textAlign: "center",
            }}
          >
            <div
              aria-hidden="true"
              style={{
                width: 56,
                height: 56,
                borderRadius: "var(--radius-lg, 16px)",
                display: "grid",
                placeItems: "center",
                background: "var(--surface-raised, #f7f7f8)",
                border: "1px solid var(--border-subtle, #ececef)",
                color: "var(--text-tertiary, #9ca3af)",
              }}
            >
              <FolderSimple size={24} />
            </div>
            <p
              style={{
                margin: 0,
                fontSize: 15,
                fontWeight: 600,
                color: "var(--text-primary, #111827)",
              }}
            >
              还没有打开专题合集
            </p>
            <p
              style={{
                margin: 0,
                maxWidth: 380,
                fontSize: 13,
                lineHeight: 1.7,
                color: "var(--text-tertiary, #9ca3af)",
              }}
            >
              专题合集把跨模块的内容放在一起——比如把一个历史事件、相关地点、
              词典条目收进同一个主题，随时一起复习。
            </p>
            <button
              type="button"
              onClick={() => setShowCreateModal(true)}
              style={{
                marginTop: 4,
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                padding: "8px 16px",
                borderRadius: "var(--radius-control, 10px)",
                border: "1px solid var(--accent-primary, #1688ff)",
                background: "var(--accent-primary, #1688ff)",
                color: "#fff",
                fontSize: 13,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              <Plus size={14} /> 新建第一个专题合集
            </button>
          </div>
        )}
      </div>

      {/* Create Modal */}
      {showCreateModal && (
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: "rgba(0, 0, 0, 0.4)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1100,
          }}
        >
          <div
            style={{
              background: "var(--surface-primary, #ffffff)",
              borderRadius: 12,
              padding: 24,
              width: 420,
              boxShadow: "0 10px 25px rgba(0, 0, 0, 0.15)",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
              <div style={{ fontSize: 16, fontWeight: 700 }}>新建专题合集</div>
              <button
                onClick={() => setShowCreateModal(false)}
                style={{ border: "none", background: "none", cursor: "pointer", color: "#9ca3af" }}
              >
                <X size={18} />
              </button>
            </div>

            <form onSubmit={handleCreate}>
              <div style={{ marginBottom: 12 }}>
                <label style={{ display: "block", fontSize: 12, fontWeight: 600, marginBottom: 4 }}>
                  合集名称
                </label>
                <input
                  type="text"
                  required
                  placeholder="例如：丝绸之路历史与地理脉络"
                  value={newTitle}
                  onChange={(e) => setNewTitle(e.target.value)}
                  style={{
                    width: "100%",
                    padding: "8px 12px",
                    borderRadius: 6,
                    border: "1px solid var(--border-color, #d1d5db)",
                    fontSize: 13,
                    boxSizing: "border-box",
                  }}
                />
              </div>

              <div style={{ marginBottom: 12 }}>
                <label style={{ display: "block", fontSize: 12, fontWeight: 600, marginBottom: 4 }}>
                  合集简介 (可选)
                </label>
                <textarea
                  placeholder="简要说明此合集的学习目标或主题"
                  value={newDescription}
                  onChange={(e) => setNewDescription(e.target.value)}
                  rows={3}
                  style={{
                    width: "100%",
                    padding: "8px 12px",
                    borderRadius: 6,
                    border: "1px solid var(--border-color, #d1d5db)",
                    fontSize: 13,
                    boxSizing: "border-box",
                    resize: "none",
                  }}
                />
              </div>

              <div style={{ marginBottom: 20 }}>
                <label style={{ display: "block", fontSize: 12, fontWeight: 600, marginBottom: 4 }}>
                  标签 (逗号分隔，可选)
                </label>
                <input
                  type="text"
                  placeholder="历史, 丝绸之路, 中亚"
                  value={newTags}
                  onChange={(e) => setNewTags(e.target.value)}
                  style={{
                    width: "100%",
                    padding: "8px 12px",
                    borderRadius: 6,
                    border: "1px solid var(--border-color, #d1d5db)",
                    fontSize: 13,
                    boxSizing: "border-box",
                  }}
                />
              </div>

              <div style={{ display: "flex", justifyContent: "flex-end", gap: 10 }}>
                <button
                  type="button"
                  onClick={() => setShowCreateModal(false)}
                  style={{
                    padding: "8px 14px",
                    borderRadius: 6,
                    border: "none",
                    background: "var(--surface-secondary, #f3f4f6)",
                    fontSize: 13,
                    cursor: "pointer",
                  }}
                >
                  取消
                </button>
                <button
                  type="submit"
                  style={{
                    padding: "8px 16px",
                    borderRadius: 6,
                    border: "none",
                    background: "var(--accent-primary, #2563eb)",
                    color: "#ffffff",
                    fontSize: 13,
                    fontWeight: 600,
                    cursor: "pointer",
                  }}
                >
                  创建合集
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
