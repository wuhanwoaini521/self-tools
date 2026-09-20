/**
 * Personal AI 前端数据契约（V4 §11/§12/§13/§15）。
 *
 * 与后端 `crates/core/src/personal_ai/` 的 serde 形状一一对应；
 * 只读契约：字段名与后端冻结一致，勿改名。
 */

export interface AppEntityRef {
  kind: string;
  id: string;
  label?: string | null;
}

export interface AppContextPayload {
  module?: string | null;
  page?: string | null;
  entity?: AppEntityRef | null;
  selection?: { kind: string; id: string; text?: string | null } | null;
  view_state?: unknown;
}

export interface AgentRequest {
  message: string;
  session_id?: string | null;
  app_context: AppContextPayload;
  /** 启用的模块列表（status.modules[].id）；空 = 全部 */
  capabilities: string[];
  locale?: string | null;
}

export interface AgentMessage {
  role: "user" | "assistant" | "tool";
  content: string;
}

export interface ToolTraceEntry {
  tool: string;
  ok: boolean;
  duration_ms: number;
  note?: string | null;
}

export type AgentActionKind =
  | "navigate"
  | "open_entity"
  | "refresh_view"
  | "show_panel";

export interface AgentAction {
  type: AgentActionKind;
  module: string;
  target: Record<string, unknown>;
}

export type UiBlockKind =
  | "entity_list"
  | "entity_card"
  | "source_list"
  | "key_value"
  | "timeline_preview";

export interface UiBlock {
  kind: UiBlockKind;
  title: string;
  data: unknown;
}

export interface AgentUsage {
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  duration_ms: number;
  tool_rounds: number;
}

export interface AgentResponse {
  session_id: string;
  message: string;
  actions: AgentAction[];
  ui_blocks: UiBlock[];
  tool_trace: ToolTraceEntry[];
  usage?: AgentUsage | null;
  messages: AgentMessage[];
  provider?: string | null;
  model?: string | null;
}

export interface ToolSpecLite {
  name: string;
  description: string;
  risk: "read" | "safe_write" | "sensitive_write" | "system";
  module: string;
}

export interface ModuleDescriptorLite {
  id: string;
  display_name: string;
  description: string;
  capabilities: string[];
  tools: string[];
}

export interface AiStatus {
  configured: boolean;
  provider: string | null;
  model: string | null;
  modules: ModuleDescriptorLite[];
  tools: ToolSpecLite[];
}

/** EntityList UI Block 的条目形状（宽松兼容后端 schemas）。 */
export interface EntityListItem {
  id?: string;
  entity_id?: string;
  kind?: string;
  title?: string;
  name?: string;
  subtitle?: string | null;
  start_year?: number | null;
  end_year?: number | null;
}

export function entityListItems(block: UiBlock): EntityListItem[] {
  const raw = block.data;
  if (!Array.isArray(raw)) return [];
  return raw as EntityListItem[];
}