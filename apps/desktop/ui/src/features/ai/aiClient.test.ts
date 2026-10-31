import { describe, expect, it, vi } from "vitest";
import type { CommandTransport } from "../../transport";
import { capabilitiesForContext, createAiClient } from "./aiClient";

describe("AI client wire contract", () => {
  it("sends snake_case HTTP trace id and supports request cancellation", async () => {
    const invoke = vi.fn();
    const transport: CommandTransport = { invoke: async <T,>(command: string, args?: Record<string, unknown>) => { invoke(command, args); return {} as T; }, isTauriRuntime: () => false, subscribe: () => () => {} };
    const client = createAiClient(transport);
    const request = { message: "hello", app_context: {}, capabilities: [] };
    await client.chat(request, "run-http-1");
    expect(invoke).toHaveBeenNthCalledWith(1, "personal_ai_chat", { request, trace_id: "run-http-1" });
    await client.cancelRun("run-http-1");
    expect(invoke).toHaveBeenNthCalledWith(2, "personal_ai_cancel_run", { traceId: "run-http-1" });
  });

  it("uses camelCase trace id for Tauri IPC", async () => {
    const invoke = vi.fn();
    const transport: CommandTransport = { invoke: async <T,>(command: string, args?: Record<string, unknown>) => { invoke(command, args); return {} as T; }, isTauriRuntime: () => true, subscribe: () => () => {} };
    await createAiClient(transport).chat({ message: "hello", app_context: {}, capabilities: [] }, "run-desktop-1");
    expect(invoke).toHaveBeenCalledWith("personal_ai_chat", { request: { message: "hello", app_context: {}, capabilities: [] }, traceId: "run-desktop-1" });
  });
});

describe("capabilitiesForContext", () => {
  const registered = ["geography", "history", "exploration", "travel", "learning", "memory", "documents", "files"];

  it("limits Geography to Geography and History crosswalk tools", () => {
    expect(capabilitiesForContext({ module: "geography", entity: { kind: "city", id: "luoyang" } }, registered))
      .toEqual(["geography", "history", "exploration"]);
  });

  it("limits Travel to itinerary and the canonical place research modules", () => {
    expect(capabilitiesForContext({ module: "travel" }, registered))
      .toEqual(["travel", "geography", "history", "exploration"]);
  });

  it("does not grant tools to a general or unknown context", () => {
    expect(capabilitiesForContext(null, registered)).toEqual([]);
    expect(capabilitiesForContext({ module: "unknown" }, registered)).toEqual([]);
  });
});
