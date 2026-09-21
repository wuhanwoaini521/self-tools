/**
 * Personal AI 前端命令客户端（V4 §36/§56）。
 *
 * Frontend 只知道 AgentRequest / AgentResponse / Actions / UI Blocks；
 * 不组 Prompt、不选模型、不调 Provider、不管 Tool schema（V4 §56）。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type { AgentRequest, AgentResponse, AiStatus } from "./aiTypes";

export interface AiClient {
 /** AI 面板状态（是否配置 + 已注册模块/工具；不含任何 key）。 */
 status(): Promise<AiStatus>;
 /** 发起一次 agent 对话（后端维护会话历史）。 */
 chat(request: AgentRequest): Promise<AgentResponse>;
}

export function createAiClient(
 transport: CommandTransport = tauriTransport,
): AiClient {
 return {
  status: () => transport.invoke<AiStatus>("personal_ai_status"),
  chat: (request) =>
   transport.invoke<AgentResponse>("personal_ai_chat", { request }),
 };
}

export const aiClient = createAiClient();
