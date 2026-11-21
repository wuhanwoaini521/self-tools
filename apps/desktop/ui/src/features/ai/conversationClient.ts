/**
 * 会话历史客户端（V11-K）：持久化会话的列表 / 读取 / 新建 / 改名 / 归档 / 删除。
 *
 * 铁律：Conversation != Memory —— 这里只是会话历史，绝不参与记忆写入。
 *
 * 传输层注入 `CommandTransport`：桌面走 Tauri IPC，网页端走 `/api/v1/conversations`，
 * 两端共用同一份后端起用例与 `conversations.db`。客户端不再直接引用
 * `@tauri-apps/api/core`（Gate 3 边界）。
 */
import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";

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

export interface ConversationClient {
  /** 列表（`includeArchived` = 「显示已归档」开关）。 */
  list(limit?: number, includeArchived?: boolean): Promise<ConversationSummaryDto[]>;
  /** 读取一个会话（含消息）；不存在返回 null（HTTP 404 / 桌面端报错由页面兜底）。 */
  load(id: string): Promise<ConversationDto | null>;
  /** 新建会话（New Chat）。 */
  create(title?: string): Promise<{ conversation_id: string; title: string }>;
  /** 重命名 / 归档（至少给一个）。 */
  update(id: string, patch: { title?: string; archived?: boolean }): Promise<void>;
  /** 追加一条消息（assistant 带来源元数据）。 */
  append(
    id: string,
    role: "user" | "assistant" | "tool",
    content: string,
    meta?: { provider?: string; model?: string },
  ): Promise<void>;
  /** 删除（连消息一起）。 */
  remove(id: string): Promise<void>;
}

export function createConversationClient(
  transport: CommandTransport = defaultTransport,
): ConversationClient {
  return {
    list: (limit = 20, includeArchived = false) =>
      transport.invoke<ConversationSummaryDto[]>("conversation_list", {
        limit,
        includeArchived,
      }),

    load: (id) =>
      transport.invoke<ConversationDto | null>("conversation_load", { id }),

    create: (title) =>
      transport.invoke<{ conversation_id: string; title: string }>(
        "conversation_create",
        { title: title ?? null },
      ),

    update: async (id, patch) => {
      await transport.invoke("conversation_update", {
        id,
        title: patch.title ?? null,
        archived: patch.archived ?? null,
      });
    },

    append: async (id, role, content, meta) => {
      await transport.invoke("conversation_append", {
        id,
        role,
        content,
        provider: meta?.provider ?? null,
        model: meta?.model ?? null,
      });
    },

    remove: async (id) => {
      await transport.invoke("conversation_delete", { id });
    },
  };
}

export const conversationClient = createConversationClient();
