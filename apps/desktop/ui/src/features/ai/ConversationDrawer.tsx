/**
 * 会话抽屉（V11-K）：New / Recent / Resume / Rename / Archive / Delete。
 *
 * 只做「会话历史」管理，不触碰 Personal Memory（Conversation != Memory）。
 * 后端命令不可用时显示受控提示，不影响 AI 面板其它功能。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowClockwise,
  Archive,
  ChatCircleDots,
  NotePencil,
  Plus,
  Trash,
  X,
} from "@phosphor-icons/react";
import { isTauriRuntime } from "../../utils";
import {
  conversationClient,
  conversationRelativeTime,
  type ConversationMessageDto,
  type ConversationSummaryDto,
} from "./conversationClient";
import type { AgentMessage } from "./aiTypes";

export interface ConversationDrawerProps {
  open: boolean;
  onClose: () => void;
  /**
   * 恢复某会话：父级把 session id 换成它，并把消息渲染进面板。
   */
  onResume: (sessionId: string, messages: AgentMessage[]) => void;
  /** 新建会话（父级清空消息 + 换 session id 后回填新 id）。 */
  onCreated: (sessionId: string) => void;
  /** 当前面板正在使用的会话 id（高亮用）。 */
  activeId?: string | null;
}

/** 后端消息 → 面板消息（tool 也按纯文本行展示）。 */
function toPanelMessages(messages: ConversationMessageDto[]): AgentMessage[] {
  return messages
    .filter((message) => message.role !== "tool" || message.content.trim().length > 0)
    .map((message) => ({
      role: message.role === "assistant" ? "assistant" : "user",
      content: message.content,
    }));
}

export function ConversationDrawer({
  open,
  onClose,
  onResume,
  onCreated,
  activeId,
}: ConversationDrawerProps) {
  const [items, setItems] = useState<ConversationSummaryDto[]>([]);
  const [loading, setLoading] = useState(false);
  const [notice, setNotice] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftTitle, setDraftTitle] = useState("");
  const supported = isTauriRuntime();

  const reload = useCallback(async () => {
    if (!supported) return;
    setLoading(true);
    setNotice("");
    try {
      setItems(await conversationClient.list(20, showArchived));
    } catch (error) {
      setNotice(`会话列表不可用：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setLoading(false);
    }
  }, [showArchived, supported]);

  useEffect(() => {
    if (open) void reload();
  }, [open, reload]);

  const visible = useMemo(
    () => (showArchived ? items : items.filter((item) => !item.archived)),
    [items, showArchived],
  );

  const createNew = useCallback(async () => {
    if (!supported) return;
    try {
      const created = await conversationClient.create();
      onCreated(created.conversation_id);
      await reload();
      setNotice("");
    } catch (error) {
      setNotice(`新建失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }, [onCreated, reload, supported]);

  const resume = useCallback(
    async (id: string) => {
      if (!supported) return;
      try {
        const conversation = await conversationClient.load(id);
        if (!conversation) {
          setNotice("会话不存在或已删除");
          return;
        }
        onResume(conversation.conversation_id, toPanelMessages(conversation.messages));
        setNotice("");
      } catch (error) {
        setNotice(`读取失败：${error instanceof Error ? error.message : String(error)}`);
      }
    },
    [onResume, supported],
  );

  const commitRename = useCallback(
    async (id: string) => {
      const title = draftTitle.trim();
      setEditingId(null);
      if (!title) return;
      try {
        await conversationClient.update(id, { title });
        await reload();
      } catch (error) {
        setNotice(`重命名失败：${error instanceof Error ? error.message : String(error)}`);
      }
    },
    [draftTitle, reload],
  );

  const toggleArchive = useCallback(
    async (item: ConversationSummaryDto) => {
      try {
        await conversationClient.update(item.conversation_id, { archived: !item.archived });
        await reload();
      } catch (error) {
        setNotice(`操作失败：${error instanceof Error ? error.message : String(error)}`);
      }
    },
    [reload],
  );

  const remove = useCallback(
    async (item: ConversationSummaryDto) => {
      const confirmed = window.confirm(`删除会话「${item.title}」？该操作不可恢复。`);
      if (!confirmed) return;
      try {
        await conversationClient.remove(item.conversation_id);
        await reload();
      } catch (error) {
        setNotice(`删除失败：${error instanceof Error ? error.message : String(error)}`);
      }
    },
    [reload],
  );

  if (!open) return null;

  return (
    <aside className="ai-conversation-drawer" aria-label="会话历史">
      <header className="ai-conversation-head">
        <ChatCircleDots size={16} />
        <strong>会话</strong>
        <button
          type="button"
          className="ai-conversation-icon"
          title="刷新"
          onClick={() => void reload()}
        >
          <ArrowClockwise size={15} />
        </button>
        <button
          type="button"
          className="ai-conversation-primary"
          onClick={() => void createNew()}
        >
          <Plus size={14} />
          新对话
        </button>
        <button
          type="button"
          className="ai-conversation-icon"
          title="关闭"
          onClick={onClose}
        >
          <X size={15} />
        </button>
      </header>

      <label className="ai-conversation-archived-toggle">
        <input
          type="checkbox"
          checked={showArchived}
          onChange={(event) => setShowArchived(event.target.checked)}
        />
        显示已归档
      </label>

      {!supported ? (
        <p className="ai-conversation-empty">会话功能需要桌面运行时（Tauri）。</p>
      ) : loading ? (
        <p className="ai-conversation-empty">加载中…</p>
      ) : visible.length === 0 ? (
        <p className="ai-conversation-empty">
          还没有会话记录。点「新对话」开始，之后可在这里继续。
        </p>
      ) : (
        <ul className="ai-conversation-list">
          {visible.map((item) => (
            <li
              key={item.conversation_id}
              className={
                "ai-conversation-row" +
                (item.conversation_id === activeId ? " active" : "") +
                (item.archived ? " archived" : "")
              }
            >
              {editingId === item.conversation_id ? (
                <input
                  className="ai-conversation-rename"
                  value={draftTitle}
                  autoFocus
                  onChange={(event) => setDraftTitle(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void commitRename(item.conversation_id);
                    if (event.key === "Escape") setEditingId(null);
                  }}
                  onBlur={() => void commitRename(item.conversation_id)}
                  aria-label="会话标题"
                />
              ) : (
                <button
                  type="button"
                  className="ai-conversation-open"
                  onClick={() => void resume(item.conversation_id)}
                  title="继续这个会话"
                >
                  <span className="ai-conversation-title">{item.title}</span>
                  <span className="ai-conversation-meta">
                    {item.message_count} 条 · {conversationRelativeTime(item.updated_at)}
                    {item.module_origin ? ` · ${item.module_origin}` : ""}
                  </span>
                </button>
              )}
              <span className="ai-conversation-actions">
                <button
                  type="button"
                  title="重命名"
                  onClick={() => {
                    setEditingId(item.conversation_id);
                    setDraftTitle(item.title);
                  }}
                >
                  <NotePencil size={14} />
                </button>
                <button
                  type="button"
                  title={item.archived ? "取消归档" : "归档"}
                  onClick={() => void toggleArchive(item)}
                >
                  <Archive size={14} />
                </button>
                <button type="button" title="删除" onClick={() => void remove(item)}>
                  <Trash size={14} />
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}

      {notice ? <p className="ai-conversation-notice">{notice}</p> : null}
    </aside>
  );
}
