/**
 * 会话历史客户端（V11-K）：持久化会话的列表 / 读取 / 新建 / 改名 / 归档 / 删除。
 *
 * 铁律：Conversation != Memory —— 这里只是会话历史，绝不参与记忆写入。
 */
import { invoke } from "@tauri-apps/api/core";

export interface ConversationSummaryDto {
  conversation_id: string;
  title: string;
  module_origin: string | null;
  created_at: number;
  updated_at: number;
  message_count: number;
  archived: boolean;
}

export interface ConversationMessageDto {
  role: "user" | "assistant" | "tool";
  content: string;
  provider?: string | null;
  model?: string | null;
  created_at?: number | null;
}

export interface ConversationDto extends ConversationSummaryDto {
  messages: ConversationMessageDto[];
}

/** 绝对时间 → 「x 分钟前」（会话列表用）。 */
export function conversationRelativeTime(unixSeconds: number): string {
  if (!unixSeconds) return "";
  const delta = Math.floor(Date.now() / 1000) - unixSeconds;
  if (delta < 60) return "刚刚";
  if (delta < 3600) return `${Math.floor(delta / 60)} 分钟前`;
  if (delta < 86400) return `${Math.floor(delta / 3600)} 小时前`;
  if (delta < 86400 * 30) return `${Math.floor(delta / 86400)} 天前`;
  return new Date(unixSeconds * 1000).toLocaleDateString("zh-CN");
}

export const conversationClient = {
  /** 列表（`includeArchived` = 「显示已归档」开关）。 */
  async list(limit = 20, includeArchived = false): Promise<ConversationSummaryDto[]> {
    return invoke<ConversationSummaryDto[]>("conversation_list", {
      limit,
      includeArchived,
    });
  },

  /** 读取一个会话（含消息）。 */
  async load(id: string): Promise<ConversationDto | null> {
    return invoke<ConversationDto | null>("conversation_load", { id });
  },

  /** 新建会话（New Chat）。 */
  async create(title?: string): Promise<{ conversation_id: string; title: string }> {
    return invoke<{ conversation_id: string; title: string }>("conversation_create", {
      title: title ?? null,
    });
  },

  /** 重命名 / 归档（至少给一个）。 */
  async update(id: string, patch: { title?: string; archived?: boolean }): Promise<void> {
    await invoke("conversation_update", {
      id,
      title: patch.title ?? null,
      archived: patch.archived ?? null,
    });
  },

  /** 追加一条消息（assistant 带来源元数据）。 */
  async append(
    id: string,
    role: "user" | "assistant" | "tool",
    content: string,
    meta?: { provider?: string; model?: string },
  ): Promise<void> {
    await invoke("conversation_append", {
      id,
      role,
      content,
      provider: meta?.provider ?? null,
      model: meta?.model ?? null,
    });
  },

  /** 删除（连消息一起）。 */
  async remove(id: string): Promise<void> {
    await invoke("conversation_delete", { id });
  },
};
