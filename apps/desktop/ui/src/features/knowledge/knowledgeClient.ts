/**
 * Personal Knowledge Layer（V6）前端命令客户端。
 *
 * 每个方法对应一个真实 Tauri 命令（命令名 / 参数 / 返回类型与 freeze sheet §4
 * 冻结一致）；page 只调用本 client，不直接接触 transport 或 `@tauri-apps/api/core`。
 *
 * 顶层命令参数用 camelCase（Tauri 自动转 snake_case）；payload 内部字段保持
 * snake_case（serde 默认）。浏览器预览（非 Tauri）下所有命令都抛错：读取类方法
 * 由页面捕获后展示「未配置 / 无数据」空态，写操作一律失败（绝不假装成功）。
 */
import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";
import type {
  DocumentHitDto,
  DocumentMetaDto,
  DocumentReadRequestDto,
  DocumentReadResultDto,
  DocumentStatusDto,
  DocumentType,
  FileIndexReportDto,
  FileMetadataDto,
  FileOpenResultDto,
  FileReadResultDto,
  FileStatusDto,
  IndexReportDto,
  KnowledgeStatusDto,
  MemoryCategory,
  MemoryDraftDto,
  MemoryItemDto,
  MemorySensitivity,
  MemoryStatsDto,
  MemoryStatus,
  MemoryUpdateRequestDto,
} from "./knowledgeTypes";

/** 浏览器预览下的统一错误文案（页面据此展示空态）。 */
/**
 * @deprecated 历史占位文案。知识库已支持网页端，保留导出仅为兼容旧引用；
 * 新代码不要依赖它——「不支持」应当来自**真实后端错误**，而不是前端预设。
 */
export const BROWSER_PREVIEW_MESSAGE = "知识库暂不可用，请检查本地数据服务是否已启动。";

export interface MemoryListParams {
  query?: string;
  category?: MemoryCategory;
  status?: MemoryStatus;
  limit?: number;
}

export interface DocumentSearchParams {
  query: string;
  documentType?: DocumentType;
  limit?: number;
}

export interface FileSearchParams {
  query?: string;
  extension?: string;
  rootId?: string;
  modifiedAfter?: number;
  limit?: number;
}

export interface KnowledgeClient {
  memoryStatus(): Promise<MemoryStatsDto>;
  memoryList(params?: MemoryListParams): Promise<MemoryItemDto[]>;
  memoryGet(id: string): Promise<MemoryItemDto>;
  memorySave(request: MemoryDraftDto): Promise<MemoryItemDto>;
  memoryConfirm(id: string): Promise<MemoryItemDto>;
  memoryUpdate(request: MemoryUpdateRequestDto): Promise<MemoryItemDto>;
  memoryArchive(id: string): Promise<MemoryItemDto>;
  memoryReject(id: string): Promise<MemoryItemDto>;
  documentsStatus(): Promise<DocumentStatusDto>;
  documentsScan(rootId?: string): Promise<IndexReportDto[]>;
  documentsSearch(params: DocumentSearchParams): Promise<DocumentHitDto[]>;
  documentsGet(documentId: string): Promise<DocumentMetaDto>;
  documentsRead(request: DocumentReadRequestDto): Promise<DocumentReadResultDto>;
  documentsRecent(limit?: number): Promise<DocumentMetaDto[]>;
  filesStatus(): Promise<FileStatusDto>;
  filesScan(rootId?: string): Promise<FileIndexReportDto[]>;
  filesSearch(params?: FileSearchParams): Promise<FileMetadataDto[]>;
  filesMetadata(target: string): Promise<FileMetadataDto>;
  filesReadText(target: string, maxChars?: number): Promise<FileReadResultDto>;
  filesOpen(target: string): Promise<FileOpenResultDto>;
  filesRecent(limit?: number): Promise<FileMetadataDto[]>;
  knowledgeStatus(): Promise<KnowledgeStatusDto>;
}

export function createKnowledgeClient(
  transport: CommandTransport = defaultTransport,
): KnowledgeClient {
  // 不再按运行时设卡：Memory / Documents / Files / 全局检索已由服务端
  // （apps/server → knowledge_api）用**同一个** KnowledgeRuntime 提供，
  // 浏览器与桌面读的是同一批索引库。此前这里对每个方法
  // `if (!isTauriRuntime()) throw` —— 于是「这台机器就是服务器」，
  // 浏览器打开却只能看到「不支持本地知识库」。
  return {
    memoryStatus: () => {
      return transport.invoke<MemoryStatsDto>("memory_status");
    },
    memoryList: (params) => {
      return transport.invoke<MemoryItemDto[]>("memory_list", {
        query: params?.query,
        category: params?.category,
        status: params?.status,
        limit: params?.limit,
      });
    },
    memoryGet: (id) => {
      return transport.invoke<MemoryItemDto>("memory_get", { id });
    },
    memorySave: (request) => {
      return transport.invoke<MemoryItemDto>("memory_save", { request });
    },
    memoryConfirm: (id) => {
      return transport.invoke<MemoryItemDto>("memory_confirm", { id });
    },
    memoryUpdate: (request) => {
      return transport.invoke<MemoryItemDto>("memory_update", { request });
    },
    memoryArchive: (id) => {
      return transport.invoke<MemoryItemDto>("memory_archive", { id });
    },
    memoryReject: (id) => {
      return transport.invoke<MemoryItemDto>("memory_reject", { id });
    },
    documentsStatus: () => {
      return transport.invoke<DocumentStatusDto>("documents_status");
    },
    documentsScan: (rootId) => {
      return transport.invoke<IndexReportDto[]>("documents_scan", { rootId });
    },
    documentsSearch: (params) => {
      return transport.invoke<DocumentHitDto[]>("documents_search", {
        query: params.query,
        documentType: params.documentType,
        limit: params.limit,
      });
    },
    documentsGet: (documentId) => {
      return transport.invoke<DocumentMetaDto>("documents_get", { documentId });
    },
    documentsRead: (request) => {
      return transport.invoke<DocumentReadResultDto>("documents_read", {
        request,
      });
    },
    documentsRecent: (limit) => {
      return transport.invoke<DocumentMetaDto[]>("documents_recent", { limit });
    },
    filesStatus: () => {
      return transport.invoke<FileStatusDto>("files_status");
    },
    filesScan: (rootId) => {
      return transport.invoke<FileIndexReportDto[]>("files_scan", { rootId });
    },
    filesSearch: (params) => {
      return transport.invoke<FileMetadataDto[]>("files_search", {
        query: params?.query,
        extension: params?.extension,
        rootId: params?.rootId,
        modifiedAfter: params?.modifiedAfter,
        limit: params?.limit,
      });
    },
    filesMetadata: (target) => {
      return transport.invoke<FileMetadataDto>("files_metadata", { target });
    },
    filesReadText: (target, maxChars) => {
      return transport.invoke<FileReadResultDto>("files_read_text", {
        target,
        maxChars,
      });
    },
    filesOpen: (target) => {
      return transport.invoke<FileOpenResultDto>("files_open", { target });
    },
    filesRecent: (limit) => {
      return transport.invoke<FileMetadataDto[]>("files_recent", { limit });
    },
    knowledgeStatus: () => {
      return transport.invoke<KnowledgeStatusDto>("knowledge_status");
    },
  };
}

export const knowledgeClient = createKnowledgeClient();
