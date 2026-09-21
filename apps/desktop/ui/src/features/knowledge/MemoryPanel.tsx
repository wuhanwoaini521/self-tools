/**
 * Memory 面板（V6 §24 / §75）：记忆的查看、确认、编辑、归档。
 *
 * 分组（已生效 / 待确认 / 已归档）+ 关键词 + 分类筛选；候选行只能确认或拒绝
 * （模型路径永不产生「已生效」），已生效行可内联编辑或归档。
 * 数据全部来自 `knowledgeClient`；浏览器预览下 client 抛错 → 本面板展示空态。
 */
import {
  Archive,
  ArrowClockwise,
  Check,
  MagnifyingGlass,
  PencilSimple,
  Trash,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import { errorMessage, formatRelativeTime } from "../../utils";
import { knowledgeClient } from "./knowledgeClient";
import {
  MEMORY_CATEGORY_LABELS,
  MEMORY_CATEGORY_OPTIONS,
  MEMORY_SENSITIVITY_LABELS,
  MEMORY_SENSITIVITY_OPTIONS,
  MEMORY_SOURCE_LABELS,
  MEMORY_STATUS_LABELS,
  type MemoryCategory,
  type MemoryItemDto,
  type MemorySensitivity,
  type MemoryStatsDto,
  type MemoryStatus,
} from "./knowledgeTypes";

type MemoryGroup = Extract<MemoryStatus, "active" | "candidate" | "archived">;

const GROUPS: { id: MemoryGroup; label: string }[] = [
  { id: "active", label: "已生效" },
  { id: "candidate", label: "待确认" },
  { id: "archived", label: "已归档" },
];

interface MemoryPanelProps {
  active: boolean;
  setNotice: (message: string) => void;
}

export function MemoryPanel({ active, setNotice }: MemoryPanelProps) {
  const [stats, setStats] = useState<MemoryStatsDto | null>(null);
  const [items, setItems] = useState<MemoryItemDto[]>([]);
  const [group, setGroup] = useState<MemoryGroup>("active");
  const [queryInput, setQueryInput] = useState("");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<MemoryCategory | "">("");
  const [loading, setLoading] = useState(false);
  const [errorText, setErrorText] = useState("");
  const [busyId, setBusyId] = useState("");
  const [editingId, setEditingId] = useState("");
  const [draftContent, setDraftContent] = useState("");
  const [draftCategory, setDraftCategory] =
    useState<MemoryCategory>("preference");
  const [draftSensitivity, setDraftSensitivity] =
    useState<MemorySensitivity>("normal");

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const [nextStats, nextItems] = await Promise.all([
        knowledgeClient.memoryStatus(),
        knowledgeClient.memoryList({
          query: query.trim() || undefined,
          category: category || undefined,
          status: group,
          limit: 100,
        }),
      ]);
      setStats(nextStats);
      setItems(nextItems);
      setErrorText("");
    } catch (error) {
      setStats(null);
      setItems([]);
      setErrorText(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [group, query, category]);

  useEffect(() => {
    if (!active) return;
    void reload();
  }, [active, reload]);

  const apply = useCallback(
    async (id: string, run: () => Promise<MemoryItemDto>) => {
      setBusyId(id);
      try {
        await run();
        await reload();
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setBusyId("");
      }
    },
    [reload, setNotice],
  );

  const startEdit = (item: MemoryItemDto) => {
    setEditingId(item.id);
    setDraftContent(item.content);
    setDraftCategory(item.category);
    setDraftSensitivity(item.sensitivity);
  };

  const saveEdit = (item: MemoryItemDto) => {
    const content = draftContent.trim();
    if (!content) {
      setNotice("记忆内容不能为空。");
      return;
    }
    setEditingId("");
    void apply(item.id, () =>
      knowledgeClient.memoryUpdate({
        id: item.id,
        content,
        category: draftCategory,
        sensitivity: draftSensitivity,
      }),
    );
  };

  const summary = [
    { key: "active", label: "已生效", value: stats?.active },
    { key: "candidates", label: "待确认", value: stats?.candidates },
    { key: "archived", label: "已归档", value: stats?.archived },
    { key: "rejected", label: "已拒绝", value: stats?.rejected },
    { key: "expired", label: "已过期", value: stats?.expired },
  ];

  return (
    <div className="knowledge-panel memory-panel">
      <div className="knowledge-statusbar">
        {summary.map((entry) => (
          <span className="knowledge-stat" key={entry.key}>
            <b>{entry.value ?? "—"}</b>
            {entry.label}
          </span>
        ))}
      </div>

      <div className="knowledge-toolbar">
        <div className="knowledge-tabs">
          {GROUPS.map((entry) => (
            <button
              key={entry.id}
              className={group === entry.id ? "active" : ""}
              onClick={() => setGroup(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </div>
        <div className="knowledge-filters">
          <div className="knowledge-search">
            <MagnifyingGlass size={14} />
            <input
              type="search"
              placeholder="搜索记忆内容…"
              value={queryInput}
              onChange={(event) => setQueryInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") setQuery(queryInput);
              }}
            />
            <button onClick={() => setQuery(queryInput)}>搜索</button>
          </div>
          <select
            value={category}
            onChange={(event) =>
              setCategory(event.target.value as MemoryCategory | "")
            }
            title="按分类筛选"
          >
            <option value="">全部分类</option>
            {MEMORY_CATEGORY_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {MEMORY_CATEGORY_LABELS[option]}
              </option>
            ))}
          </select>
          <button
            className="knowledge-refresh"
            onClick={() => void reload()}
            disabled={loading}
            title="刷新"
          >
            <ArrowClockwise size={14} />
            刷新
          </button>
        </div>
      </div>

      {errorText ? <p className="knowledge-muted">{errorText}</p> : null}

      {items.length === 0 && !loading ? (
        <div className="knowledge-empty">
          {errorText
            ? "暂无数据。"
            : "还没有记忆。提示：对 AI 说『记住：……』，然后在这里确认。"}
        </div>
      ) : (
        <ul className="memory-list">
          {items.map((item) => (
            <li
              key={item.id}
              className={
                "memory-card" +
                (item.status === "candidate" ? " memory-card--candidate" : "")
              }
            >
              <div className="memory-card-head">
                <span className="memory-badge">
                  {item.category_label ??
                    MEMORY_CATEGORY_LABELS[item.category] ??
                    item.category}
                </span>
                <span className="memory-badge muted">
                  {MEMORY_SOURCE_LABELS[item.source_type] ?? item.source_type}
                </span>
                <span className="memory-badge status">
                  {MEMORY_STATUS_LABELS[item.status] ?? item.status}
                </span>
                {item.sensitivity !== "normal" ? (
                  <span
                    className={
                      "memory-badge sensitivity " + item.sensitivity
                    }
                  >
                    {MEMORY_SENSITIVITY_LABELS[item.sensitivity]}
                  </span>
                ) : null}
                <span className="memory-time">
                  {formatRelativeTime(item.updated_at)}
                </span>
              </div>

              {editingId === item.id ? (
                <div className="memory-edit">
                  <textarea
                    value={draftContent}
                    rows={3}
                    onChange={(event) => setDraftContent(event.target.value)}
                  />
                  <div className="memory-edit-controls">
                    <select
                      value={draftCategory}
                      onChange={(event) =>
                        setDraftCategory(event.target.value as MemoryCategory)
                      }
                    >
                      {MEMORY_CATEGORY_OPTIONS.map((option) => (
                        <option key={option} value={option}>
                          {MEMORY_CATEGORY_LABELS[option]}
                        </option>
                      ))}
                    </select>
                    <select
                      value={draftSensitivity}
                      onChange={(event) =>
                        setDraftSensitivity(
                          event.target.value as MemorySensitivity,
                        )
                      }
                    >
                      {MEMORY_SENSITIVITY_OPTIONS.map((option) => (
                        <option key={option} value={option}>
                          {MEMORY_SENSITIVITY_LABELS[option]}
                        </option>
                      ))}
                    </select>
                    <button
                      className="memory-action primary"
                      disabled={busyId === item.id}
                      onClick={() => saveEdit(item)}
                    >
                      保存
                    </button>
                    <button
                      className="memory-action"
                      onClick={() => setEditingId("")}
                    >
                      取消
                    </button>
                  </div>
                </div>
              ) : (
                <p className="memory-content">{item.content}</p>
              )}

              <div className="memory-card-actions">
                {item.status === "candidate" ? (
                  <>
                    <button
                      className="memory-action primary"
                      disabled={busyId === item.id}
                      onClick={() =>
                        void apply(item.id, () =>
                          knowledgeClient.memoryConfirm(item.id),
                        )
                      }
                    >
                      <Check size={13} />
                      确认
                    </button>
                    <button
                      className="memory-action"
                      disabled={busyId === item.id}
                      onClick={() =>
                        void apply(item.id, () =>
                          knowledgeClient.memoryReject(item.id),
                        )
                      }
                    >
                      <Trash size={13} />
                      拒绝
                    </button>
                  </>
                ) : null}
                {item.status === "active" ? (
                  <>
                    <button
                      className="memory-action"
                      disabled={busyId === item.id}
                      onClick={() => startEdit(item)}
                    >
                      <PencilSimple size={13} />
                      编辑
                    </button>
                    <button
                      className="memory-action"
                      disabled={busyId === item.id}
                      onClick={() =>
                        void apply(item.id, () =>
                          knowledgeClient.memoryArchive(item.id),
                        )
                      }
                    >
                      <Archive size={13} />
                      归档
                    </button>
                  </>
                ) : null}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
