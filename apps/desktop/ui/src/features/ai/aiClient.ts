/**
 * Personal AI 前端命令客户端（V4 §36/§56；V11 + 进度事件）。
 *
 * Frontend 只知道 AgentRequest / AgentResponse / Actions / UI Blocks；
 * 不组 Prompt、不选模型、不调 Provider、不管 Tool schema（V4 §56）。
 *
 * 桌面端走 Tauri IPC，网页端走本地只读 HTTP 服务（同一份 settings.json 与
 * provider 配置），两端能力一致。
 */
import { useEffect, useState } from "react";
import { isTauriRuntime } from "../../utils";
import type { CommandTransport } from "../../transport";
import type { TravelPlan } from "../../types";
import { defaultTransport } from "../../transport";
import type {
 AgentProgressEvent,
 AgentRequest,
 AppContextPayload,
 AgentResponse,
 AiStatus,
} from "./aiTypes";

const CONTEXT_CAPABILITY_SCOPE: Record<string, readonly string[]> = {
 geography: ["geography", "history", "exploration"],
 history: ["history", "geography", "exploration"],
 travel: ["travel", "geography", "history", "exploration"],
 learning: ["learning"],
 knowledge: ["knowledge", "memory", "documents", "files"],
 memory: ["memory"],
 documents: ["documents", "knowledge"],
 files: ["files"],
 "study-board": ["study-board"],
 language: ["language", "learning"],
 news: ["news"],
 rss: ["rss"],
 server: ["server"],
};

/** Client hint only; PersonalAgent independently enforces this scope server-side. */
export function capabilitiesForContext(
 context: AppContextPayload | null,
 registeredModules: string[],
): string[] {
 const module = context?.module;
 if (!module) return [];
 const scope = CONTEXT_CAPABILITY_SCOPE[module];
 if (!scope) return [];
 return scope.filter((candidate) => registeredModules.includes(candidate));
}

export interface AiClient {
 /** AI 面板状态（是否配置 + 已注册模块/工具；不含任何 key）。 */
 status(): Promise<AiStatus>;
 /** 发起一次 agent 对话（后端维护会话历史；traceId 只用于本次请求关联/取消）。 */
 chat(request: AgentRequest, traceId?: string): Promise<AgentResponse>;
 /** Cancel only the matching active request; does not affect its conversation. */
 cancelRun(traceId: string): Promise<boolean>;
 /** Confirm/cancel a backend-issued TravelPlan proposal ticket. */
 confirmTravelChange(proposalId: string): Promise<TravelPlan>;
 cancelTravelChange(proposalId: string): Promise<void>;
 /** 订阅后端推送的执行进度（V11：过程可见）。返回取消订阅函数。 */
 onProgress(handler: (progress: AgentProgressEvent) => void): () => void;
}

export function createAiClient(
 transport: CommandTransport = defaultTransport,
): AiClient {
 return {
  status: () => transport.invoke<AiStatus>("personal_ai_status"),
  chat: (request, traceId) =>
   transport.invoke<AgentResponse>("personal_ai_chat", transport.isTauriRuntime()
    ? { request, traceId }
    : { request, trace_id: traceId }),
  cancelRun: (traceId) =>
   transport.invoke<boolean>("personal_ai_cancel_run", { traceId }),
  confirmTravelChange: (proposalId) =>
   transport.invoke<TravelPlan>("personal_ai_confirm_travel_change", { proposalId }),
  cancelTravelChange: async (proposalId) => {
   await transport.invoke<void>("personal_ai_cancel_travel_change", { proposalId });
  },
  onProgress: (handler) => transport.subscribe<AgentProgressEvent>("agent-progress", handler),
 };
}

export const aiClient = createAiClient();

/** 订阅 agent 进度（组件用；自动在卸载时退订）。 */
export function useAgentProgress(
 handler: (progress: AgentProgressEvent) => void,
 enabled = true,
): void {
 const [stable] = useState(() => handler);
 useEffect(() => {
  if (!enabled || !isTauriRuntime()) return;
  return aiClient.onProgress(stable);
 }, [enabled, stable]);
}
