/**
 * Personal AI 前端数据契约（V4 §11/§12/§13/§15）。
 *
 * 与后端 `crates/core/src/personal_ai/` 的 serde 形状一一对应；
 * 只读契约：字段名与后端冻结一致，勿改名。
 */
import type {
 DocumentType,
 MemoryCategory,
 MemorySensitivity,
 MemorySourceType,
 MemoryStatus,
} from "../knowledge/knowledgeTypes";

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
 | "show_panel"
 | "open_document"
 | "open_file"
 | "confirm_memory";

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
 | "timeline_preview"
 | "memory_list"
 | "document_list"
 | "document_card"
 | "document_reference"
 | "file_list";

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

// ---------------------------------------------------------------------------
// Personal Knowledge UI Blocks（V6 §5）—— 形状与后端模块适配器冻结一致
// ---------------------------------------------------------------------------

/** `memory_list` UI Block 的条目。 */
export interface MemoryListItem {
 id: string;
 category: MemoryCategory;
 category_label?: string | null;
 content: string;
 status?: MemoryStatus | null;
 source_type?: MemorySourceType | null;
 sensitivity?: MemorySensitivity | null;
 updated_at?: number | null;
 needs_confirmation?: boolean | null;
}

/** `document_list` UI Block 的条目。 */
export interface DocumentListItem {
 document_id: string;
 title: string;
 document_type?: DocumentType | null;
 relative_path?: string | null;
 location?: string | null;
 snippet?: string | null;
 score?: number | null;
 modified_at?: number | null;
}

/** `document_card` UI Block 的数据。 */
export interface DocumentCardData {
 document_id: string;
 title: string;
 document_type?: DocumentType | null;
 path?: string | null;
 relative_path?: string | null;
 size_bytes?: number | null;
 modified_at?: number | null;
 chunk_count?: number | null;
 content_available?: boolean | null;
 index_error?: string | null;
}

/** `document_reference` UI Block 的数据。 */
export interface DocumentReferenceData {
 document_id: string;
 title: string;
 location?: string | null;
 snippet?: string | null;
}

/** `file_list` UI Block 的条目。 */
export interface FileListItem {
 file_id?: string | null;
 file_name: string;
 relative_path?: string | null;
 path?: string | null;
 extension?: string | null;
 size_bytes?: number | null;
 modified_at?: number | null;
 restricted?: boolean | null;
}

/** Block 的 `data.items` 数组（形状不符时返回空数组，渲染层优雅降级）。 */
function blockItems<T>(block: UiBlock): T[] {
 const items = (block.data as { items?: unknown } | null | undefined)?.items;
 return Array.isArray(items) ? (items as T[]) : [];
}

export function memoryListItems(block: UiBlock): MemoryListItem[] {
 return blockItems<MemoryListItem>(block);
}

export function documentListItems(block: UiBlock): DocumentListItem[] {
 return blockItems<DocumentListItem>(block);
}

export function fileListItems(block: UiBlock): FileListItem[] {
 return blockItems<FileListItem>(block);
}

export function documentCardData(block: UiBlock): DocumentCardData | null {
 const data = block.data;
 if (!data || typeof data !== "object" || Array.isArray(data)) return null;
 return data as DocumentCardData;
}

export function documentReferenceData(
 block: UiBlock,
): DocumentReferenceData | null {
 const data = block.data;
 if (!data || typeof data !== "object" || Array.isArray(data)) return null;
 return data as DocumentReferenceData;
}

// ---------------------------------------------------------------------------
// History Enrichment（V5 Gate 4/8）—— 与后端 core/history_enrichment 契约一致
// ---------------------------------------------------------------------------

export type EnrichmentState =
 | "MISSING"
 | "GENERATING"
 | "READY"
 | "STALE"
 | "FAILED"
 | "REVIEWED";

export interface EnrichmentSectionInfo {
 section: "overview" | "background" | "impact";
 state: EnrichmentState;
}

export interface EnrichmentClaimDto {
 text: string;
 source_ids: string[];
}

export interface EnrichmentPayloadDto {
 section: string;
 content: string;
 claims: EnrichmentClaimDto[];
 uncertainties: string[];
 controversies: string[];
}

export interface EnrichmentMetadataDto {
 generated_at: number;
 refreshed_at: number;
 model: string | null;
 provider: string | null;
 prompt_version: string;
 schema_version: number;
 canonical_revision: string | null;
 source_ids: string[];
 generation_count: number;
}

export interface EnrichmentViewDto {
 key: {
  entity_type: string;
  entity_id: string;
  section: string;
  locale: string;
  schema_version: number;
 };
 state: EnrichmentState;
 payload: EnrichmentPayloadDto | null;
 metadata: EnrichmentMetadataDto | null;
 error: string | null;
}
