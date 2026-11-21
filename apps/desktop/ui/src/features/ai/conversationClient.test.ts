/**
 * 会话历史客户端单测：六个命令的参数形状必须与桌面端 Tauri 命令一致。
 *
 * 客户端的唯一职责是「命令名 + 参数 → transport」；桌面与网页只是传输层不同。
 * 这里锁住参数形状，避免网页端 POST/PATCH 少字段（例如 update 只带 title）。
 */
import { describe, expect, it } from "vitest";
import { createConversationClient } from "./conversationClient";
import type { CommandTransport } from "../../transport";

function fakeTransport(responses: Record<string, unknown>): CommandTransport & {
  calls: { command: string; args: Record<string, unknown> }[];
} {
  const calls: { command: string; args: Record<string, unknown> }[] = [];
  return {
    calls,
    invoke: async <T,>(command: string, args: Record<string, unknown> = {}) => {
      calls.push({ command, args });
      return responses[command] as T;
    },
    isTauriRuntime: () => false,
    subscribe: () => () => {},
  };
}

describe("会话历史客户端", () => {
  it("list 默认 20 条且不含归档，显式参数原样透传", async () => {
    const transport = fakeTransport({ conversation_list: [] });
    const client = createConversationClient(transport);

    await client.list();
    expect(transport.calls[0]).toEqual({
      command: "conversation_list",
      args: { limit: 20, includeArchived: false },
    });

    await client.list(5, true);
    expect(transport.calls[1]).toEqual({
      command: "conversation_list",
      args: { limit: 5, includeArchived: true },
    });
  });

  it("load / remove 只带会话 id", async () => {
    const transport = fakeTransport({ conversation_load: null });
    const client = createConversationClient(transport);

    await expect(client.load("c-1")).resolves.toBeNull();
    await client.remove("c-1");

    expect(transport.calls).toEqual([
      { command: "conversation_load", args: { id: "c-1" } },
      { command: "conversation_delete", args: { id: "c-1" } },
    ]);
  });

  it("create 缺省发送 null 标题（后端归一为默认标题）", async () => {
    const transport = fakeTransport({
      conversation_create: { conversation_id: "c-2", title: "新对话" },
    });
    const client = createConversationClient(transport);

    await client.create();
    await client.create("杭州两日游");

    expect(transport.calls[0]).toEqual({
      command: "conversation_create",
      args: { title: null },
    });
    expect(transport.calls[1]).toEqual({
      command: "conversation_create",
      args: { title: "杭州两日游" },
    });
  });

  it("update 同时带 title 与 archived（未给的一侧为 null）", async () => {
    const transport = fakeTransport({ conversation_update: null });
    const client = createConversationClient(transport);

    await client.update("c-1", { title: "改名" });
    await client.update("c-1", { archived: true });

    expect(transport.calls[0]).toEqual({
      command: "conversation_update",
      args: { id: "c-1", title: "改名", archived: null },
    });
    expect(transport.calls[1]).toEqual({
      command: "conversation_update",
      args: { id: "c-1", title: null, archived: true },
    });
  });

  it("append 带上角色与 provider/model 来源", async () => {
    const transport = fakeTransport({ conversation_append: null });
    const client = createConversationClient(transport);

    await client.append("c-1", "user", "帮我规划杭州两日游");
    await client.append("c-1", "assistant", "第一天：西湖。", {
      provider: "openai-compatible",
      model: "step-5-preview",
    });

    expect(transport.calls[0]).toEqual({
      command: "conversation_append",
      args: {
        id: "c-1",
        role: "user",
        content: "帮我规划杭州两日游",
        provider: null,
        model: null,
      },
    });
    expect(transport.calls[1]).toEqual({
      command: "conversation_append",
      args: {
        id: "c-1",
        role: "assistant",
        content: "第一天：西湖。",
        provider: "openai-compatible",
        model: "step-5-preview",
      },
    });
  });
});
