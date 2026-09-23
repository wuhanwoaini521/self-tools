/**
 * Personal AI 前端命令客户端（V4 §36/§56；V11 + 进度事件）。
 *
 * Frontend 只知道 AgentRequest / AgentResponse / Actions / UI Blocks；
 * 不组 Prompt、不选模型、不调 Provider、不管 Tool schema（V4 §56）。
 */
import { useEffect, useState } from "react";
import { isTauriRuntime } from "../../utils";
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
 AgentProgressEvent,
 AgentRequest,
 AgentResponse,
 AiStatus,
} from "./aiTypes";

export interface AiClient {
 /** AI 面板状态（是否配置 + 已注册模块/工具；不含任何 key）。 */
 status(): Promise<AiStatus>;
 /** 发起一次 agent 对话（后端维护会话历史）。 */
 chat(request: AgentRequest): Promise<AgentResponse>;
 /** 订阅后端推送的执行进度（V11：过程可见）。返回取消订阅函数。 */
 onProgress(handler: (progress: AgentProgressEvent) => void): () => void;
}

export function createAiClient(
 transport: CommandTransport = tauriTransport,
): AiClient {
 return {
  status: () => transport.invoke<AiStatus>("personal_ai_status"),
  chat: (request) =>
   transport.invoke<AgentResponse>("personal_ai_chat", { request }),
  onProgress: (handler) => tauriTransport.subscribe<AgentProgressEvent>("agent-progress", handler),
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
