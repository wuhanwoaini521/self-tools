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
import { KnowledgeReviewCardEditor } from "./KnowledgeReviewCardEditor";
import { CollectionItemNoteEditor } from "./CollectionItemNoteEditor";
import { sourceRoute } from "./sourceNavigation";
import "./collections.css";
import type { Collection, CollectionItem } from "../../types";

interface CollectionsPageProps {
  active: boolean;
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

export function CollectionsPage({ active, onNavigate, onAskAi }: CollectionsPageProps) {
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
      setSelectedCollection((current) =>
        current && list.some((collection) => collection.id === current.id)
          ? list.find((collection) => collection.id === current.id) ?? null
          : list[0] ?? null,
      );
    } catch (err) {
      console.error("Failed to load collections:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (active) void loadCollections();
  }, [active, loadCollections]);

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
          item_count: Math.max(0, selectedCollection.item_count - 1),
        });
      }
    } catch (err) {
      console.error("Failed to remove item:", err);
    }
  };

  const handleOpenItem = (item: CollectionItem) => {
    const route = sourceRoute(item.module, item.entity_type, item.entity_id);
    if (route) {
      onNavigate?.(route);
      return;
    }
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
    <div className="collections-shell page-shell">
      <aside className="collections-sidebar" aria-label="专题合集列表">
        <div className="collections-sidebar-head">
          <span className="collections-sidebar-title">
            专题合集 <b>{collections.length}</b>
          </span>
          <button type="button" className="collections-create-btn" onClick={() => setShowCreateModal(true)}>
            <Plus size={15} /> 新建
          </button>
        </div>
        <div className="collections-list">
          {loading ? <div className="collections-list-empty">正在读取合集…</div> : null}
          {!loading && collections.length === 0 ? (
            <div className="collections-list-empty">
              还没有合集
              <br />
              点右上角「新建」创建第一个专题
            </div>
          ) : null}
          {collections.map((col) => {
            const isSelected = selectedCollection?.id === col.id;
            return (
              <button
                key={col.id}
                type="button"
                className={"collections-item" + (isSelected ? " is-active" : "")}
                onClick={() => setSelectedCollection(col)}
                aria-current={isSelected ? "true" : undefined}
              >
                <span className="collections-item-top">
                  <span className="collections-item-title">{col.title}</span>
                  <span className="collections-item-count">{col.item_count} 项</span>
                </span>
                {col.description ? <span className="collections-item-desc">{col.description}</span> : null}
                {col.tags.length > 0 ? (
                  <span className="collections-tags">
                    {col.tags.map((tag) => (
                      <span key={tag} className="collections-tag">#{tag}</span>
                    ))}
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>
      </aside>

      <section className="collections-content">
        {selectedCollection ? (
          <div className="collections-detail">
            <header className="collections-detail-head">
              <div className="collections-detail-main">
                <h1 className="collections-detail-title">{selectedCollection.title}</h1>
                {selectedCollection.description ? (
                  <p className="collections-detail-desc">{selectedCollection.description}</p>
                ) : null}
                <div className="collections-detail-meta">
                  <span>共收录 {items.length} 个跨模块知识条目</span>
                  {onAskAi ? (
                    <button
                      type="button"
                      className="collections-ai-btn"
                      onClick={() =>
                        onAskAi(
                          `请针对专题合集《${selectedCollection.title}》（包含 ${items.map((i) => i.title).join("、")}）进行综合知识串讲与脉络梳理。`
                        )
                      }
                    >
                      <Sparkle size={14} /> AI 综合串讲
                    </button>
                  ) : null}
                </div>
              </div>
              <button
                type="button"
                className="collections-delete-btn"
                onClick={() => handleDeleteCollection(selectedCollection.id)}
              >
                <Trash size={14} /> 删除
              </button>
            </header>

            {items.length === 0 ? (
              <div className="collections-empty">
                <Folder size={30} />
                <strong>合集中暂无内容</strong>
                <p>在知识图谱或今日主页点击「加入专题合集」，把历史、地理、语言、新闻条目归档到这里。</p>
              </div>
            ) : (
              <div className="collections-grid">
                {items.map((item) => (
                  <article className="collections-card" key={item.id}>
                    <div className="collections-card-head">
                      <span className="collections-card-type">
                        {MODULE_ICONS[item.module] ?? <BookOpen size={15} />}
                        {item.module} · {item.entity_type}
                      </span>
                      <button
                        type="button"
                        className="collections-card-remove"
                        onClick={() => handleRemoveItem(item.id)}
                        aria-label={`移除 ${item.title}`}
                      >
                        <X size={14} />
                      </button>
                    </div>
                    <h3 className="collections-card-title">{item.title}</h3>
                    <CollectionItemNoteEditor
                      item={item}
                      onSaved={(updated) =>
                        setItems((current) =>
                          current.map((entry) => (entry.id === updated.id ? updated : entry)),
                        )
                      }
                    />
                    <div className="collections-card-foot">
                      <button type="button" className="collections-open-btn" onClick={() => handleOpenItem(item)}>
                        <ArrowSquareOut size={14} /> 查看详情
                      </button>
                      <KnowledgeReviewCardEditor item={item} onNavigate={onNavigate} />
                    </div>
                  </article>
                ))}
              </div>
            )}
          </div>
        ) : (
          <div className="collections-empty collections-empty-page">
            <span className="collections-empty-icon" aria-hidden="true">
              <FolderSimple size={22} />
            </span>
            <strong>还没有打开专题合集</strong>
            <p>
              专题合集把跨模块内容放到一起：一个历史事件、相关地点、词典条目收进同一主题，随时一起复习。
            </p>
            <button type="button" className="collections-create-btn" onClick={() => setShowCreateModal(true)}>
              <Plus size={14} /> 新建第一个专题合集
            </button>
          </div>
        )}
      </section>

      {showCreateModal ? (
        <div className="collections-modal-backdrop" onClick={() => setShowCreateModal(false)}>
          <div
            className="collections-modal"
            role="dialog"
            aria-modal="true"
            aria-label="新建专题合集"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="collections-modal-head">
              <strong>新建专题合集</strong>
              <button type="button" onClick={() => setShowCreateModal(false)} aria-label="关闭">
                <X size={18} />
              </button>
            </div>
            <form className="collections-form" onSubmit={handleCreate}>
              <label>
                <span>合集名称</span>
                <input
                  type="text"
                  required
                  placeholder="例如：丝绸之路历史与地理脉络"
                  value={newTitle}
                  onChange={(event) => setNewTitle(event.target.value)}
                />
              </label>
              <label>
                <span>合集简介（可选）</span>
                <textarea
                  rows={3}
                  placeholder="简要说明此合集的学习目标或主题"
                  value={newDescription}
                  onChange={(event) => setNewDescription(event.target.value)}
                />
              </label>
              <label>
                <span>标签（逗号分隔，可选）</span>
                <input
                  type="text"
                  placeholder="历史, 丝绸之路, 中亚"
                  value={newTags}
                  onChange={(event) => setNewTags(event.target.value)}
                />
              </label>
              <div className="collections-form-actions">
                <button type="button" className="collections-cancel-btn" onClick={() => setShowCreateModal(false)}>
                  取消
                </button>
                <button type="submit" className="collections-create-btn">
                  创建合集
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}
    </div>
  );
}
