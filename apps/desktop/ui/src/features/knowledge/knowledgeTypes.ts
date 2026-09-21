/**
 * Personal Knowledge Layer（V6）前端数据契约。
 *
 * 与 `local://v6-contracts.md` §4 / §5 冻结一致：
 * - 顶层命令参数在 client 里用 camelCase（Tauri 自动转 snake_case）；
 * - 本文件的 DTO 字段一律 snake_case（serde 默认），枚举为 snake_case 字符串；
 * - 只读契约：字段名与后端冻结一致，勿改名。
 */
import type { AgentAction } from "../ai/aiTypes";

// ---------------------------------------------------------------------------
// 枚举（与 core::memory / core::documents / core::files 的 serde 形状一致）
// ---------------------------------------------------------------------------

export type MemoryCategory =
  | "preference"
  | "personal_fact"
  | "project_fact"
  | "environment"
  | "routine"
  | "instruction";

export type MemoryStatus =
  | "candidate"
  | "active"
  | "rejected"
  | "archived"
  | "expired";

export type MemorySensitivity = "normal" | "private" | "sensitive";

export type MemorySourceType =
  | "explicit_user"
  | "conversation_candidate"
  | "import"
  | "system";

export type DocumentType = "markdown" | "text" | "json" | "pdf" | "other";

export type DocumentVisibility = "normal" | "private";

export type FileContentKind = "text" | "binary" | "unknown";

/** 中文标签：后端 `category_label` 缺失时的兜底（浏览器预览 / 旧数据）。 */
export const MEMORY_CATEGORY_LABELS: Record<MemoryCategory, string> = {
  preference: "偏好",
  personal_fact: "个人事实",
  project_fact: "项目事实",
  environment: "环境",
  routine: "日常习惯",
  instruction: "指令",
};

export const MEMORY_STATUS_LABELS: Record<MemoryStatus, string> = {
  candidate: "待确认",
  active: "已生效",
  rejected: "已拒绝",
  archived: "已归档",
  expired: "已过期",
};

export const MEMORY_SOURCE_LABELS: Record<MemorySourceType, string> = {
  explicit_user: "用户明确",
  conversation_candidate: "对话候选",
  import: "导入",
  system: "系统",
};

export const MEMORY_SENSITIVITY_LABELS: Record<MemorySensitivity, string> = {
  normal: "常规",
  private: "私密",
  sensitive: "敏感",
};

export const DOCUMENT_TYPE_LABELS: Record<DocumentType, string> = {
  markdown: "Markdown",
  text: "纯文本",
  json: "JSON",
  pdf: "PDF",
  other: "其他",
};

export const MEMORY_CATEGORY_OPTIONS: MemoryCategory[] = [
  "preference",
  "personal_fact",
  "project_fact",
  "environment",
  "routine",
  "instruction",
];

export const MEMORY_SENSITIVITY_OPTIONS: MemorySensitivity[] = [
  "normal",
  "private",
  "sensitive",
];

export const DOCUMENT_TYPE_OPTIONS: DocumentType[] = [
  "markdown",
  "text",
  "json",
  "pdf",
  "other",
];

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

export interface MemoryCategoryCountDto {
  category: MemoryCategory;
  count: number;
}

export interface MemoryStatsDto {
  total: number;
  active: number;
  candidates: number;
  archived: number;
  rejected: number;
  expired: number;
  categories: MemoryCategoryCountDto[];
}

/** `MemoryItem` 原样 + `category_label`（后端给出中文标签）。 */
export interface MemoryItemDto {
  id: string;
  category: MemoryCategory;
  category_label?: string;
  content: string;
  status: MemoryStatus;
  source_type: MemorySourceType;
  source_reference?: string | null;
  created_at: number;
  updated_at: number;
  last_used_at?: number | null;
  expires_at?: number | null;
  confidence: number;
  sensitivity: MemorySensitivity;
  metadata?: unknown;
}

export interface MemoryDraftDto {
  category: MemoryCategory;
  content: string;
  source_type?: MemorySourceType;
  source_reference?: string | null;
  sensitivity?: MemorySensitivity;
  confidence?: number;
  expires_at?: number | null;
}

export interface MemoryUpdateRequestDto {
  id: string;
  content: string;
  category?: MemoryCategory;
  sensitivity?: MemorySensitivity;
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

export interface KnowledgeRootDto {
  id: string;
  label: string;
  path: string;
  enabled: boolean;
}

export interface DocumentStatusDto {
  roots: KnowledgeRootDto[];
  documents: number;
  chunks: number;
  content_available: number;
  metadata_only: number;
  failed: number;
  configured: boolean;
}

export interface IndexReportDto {
  root_id: string;
  scanned: number;
  indexed: number;
  unchanged: number;
  metadata_only: number;
  failed: number;
  removed: number;
  truncated: boolean;
  duration_ms: number;
}

export interface DocumentMetaDto {
  document_id: string;
  root_id: string;
  title: string;
  document_type: DocumentType;
  path: string;
  relative_path: string;
  size_bytes: number;
  modified_at: number;
  indexed_at: number;
  chunk_count: number;
  content_available: boolean;
  visibility: DocumentVisibility;
  index_error?: string | null;
}

export interface DocumentHitDto {
  meta: DocumentMetaDto;
  chunk_id?: string | null;
  location?: string | null;
  snippet: string;
  matched_in_title: boolean;
  score: number;
}

export interface DocumentReadRequestDto {
  document_id: string;
  chunk_id?: string;
  section?: string;
  offset?: number;
  max_chars?: number;
}

export interface DocumentReadResultDto {
  document_id: string;
  title: string;
  text: string;
  location: string;
  chunk_ids: string[];
  total_chunks: number;
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

export interface FileStatusDto {
  roots: KnowledgeRootDto[];
  files: number;
  text_files: number;
  binary_files: number;
  restricted: number;
  failed: number;
  configured: boolean;
}

export interface FileIndexReportDto {
  root_id: string;
  scanned: number;
  indexed: number;
  unchanged: number;
  restricted: number;
  removed: number;
  truncated: boolean;
  duration_ms: number;
}

export interface FileMetadataDto {
  file_id: string;
  root_id: string;
  path: string;
  relative_path: string;
  file_name: string;
  extension?: string | null;
  size_bytes: number;
  modified_at: number;
  indexed_at: number;
  content_kind: FileContentKind;
  restricted: boolean;
  index_error?: string | null;
}

export interface FileReadResultDto {
  file: FileMetadataDto;
  text: string;
  truncated: boolean;
  char_count: number;
}

export interface FileOpenResultDto {
  file: FileMetadataDto;
  action: AgentAction;
}

// ---------------------------------------------------------------------------
// Knowledge facade
// ---------------------------------------------------------------------------

export interface KnowledgeBudgetDto {
  max_results: number;
  max_chars: number;
  max_per_source: number;
  max_memories: number;
  max_document_chunks: number;
  max_files: number;
}

export interface KnowledgeMetricsSnapshotDto {
  retrievals: number;
  memory_hits: number;
  document_hits: number;
  file_hits: number;
  results_selected: number;
  dropped_by_budget: number;
  omitted_sensitive: number;
  retrieval_duration_ms: number;
  index_runs: number;
  indexed_documents: number;
  indexed_files: number;
  index_failures: number;
  index_duration_ms: number;
  tool_calls: Record<string, number>;
  tool_duration_ms: number;
  context_items_selected: number;
}

export interface KnowledgeStatusDto {
  sources: string[];
  budget: KnowledgeBudgetDto;
  metrics: KnowledgeMetricsSnapshotDto;
  configured_roots: KnowledgeRootDto[];
}

// ---------------------------------------------------------------------------
// AI Action 目标形状（§5）—— 知识域的 Action target，App 与 AI Panel 共用
// ---------------------------------------------------------------------------

/** `confirm_memory` Action 的 target；带 memory_id = 待确认候选，否则为新建。 */
export interface ConfirmMemoryTarget {
  memory_id?: string | null;
  category: MemoryCategory;
  content: string;
  source_type?: MemorySourceType | null;
  source_reference?: string | null;
}

/** `open_file` Action 的 target。 */
export interface OpenFileTarget {
  file_id?: string | null;
  path: string;
  file_name?: string | null;
  root_id?: string | null;
}

/** `open_document` Action 的 target。 */
export interface OpenDocumentTarget {
  document_id: string;
  title?: string | null;
  location?: string | null;
}

