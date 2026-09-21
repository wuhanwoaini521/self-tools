/**
 * Documents 面板（V6 §76）：文档索引状态、搜索与片段阅读。
 *
 * 根目录来自设置（`settings.knowledge.document_roots`，空则复用文件允许目录）；
 * 未配置时只展示引导，不做任何扫描。索引/读取命令都不是模型工具，只能由用户
 * 在这里显式触发。
 */
import {
  ArrowClockwise,
  CaretDown,
  CaretRight,
  FileText,
  FolderOpen,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import { errorMessage, formatRelativeTime } from "../../utils";
import { knowledgeClient } from "./knowledgeClient";
import {
  DOCUMENT_TYPE_LABELS,
  DOCUMENT_TYPE_OPTIONS,
  type DocumentHitDto,
  type DocumentMetaDto,
  type DocumentReadResultDto,
  type DocumentStatusDto,
  type DocumentType,
  type IndexReportDto,
} from "./knowledgeTypes";

export interface DocumentsFocus {
  documentId: string;
  nonce: number;
}

interface DocumentsPanelProps {
  active: boolean;
  setNotice: (message: string) => void;
  onOpenSettings: () => void;
  focus?: DocumentsFocus | null;
  onFocusConsumed?: () => void;
}

function typeLabel(type: DocumentType | null | undefined): string {
  if (!type) return "文档";
  return DOCUMENT_TYPE_LABELS[type] ?? type;
}

/** 片段正文视图（搜索结果与「最近索引」共用）。 */
function DocReadView({
  reading,
  result,
}: {
  reading: boolean;
  result: DocumentReadResultDto | null;
}) {
  if (reading) return <p className="knowledge-muted">读取中…</p>;
  if (!result) return <p className="knowledge-muted">无法读取该文档内容。</p>;
  return (
    <div className="doc-read">
      <p className="doc-read-location">
        {result.location} · 共 {result.total_chunks} 个片段
      </p>
      <pre className="doc-read-text">{result.text}</pre>
      {result.truncated ? (
        <p className="knowledge-muted">内容较长，仅显示前 4000 字。</p>
      ) : null}
    </div>
  );
}

function formatSize(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function DocumentsPanel({
  active,
  setNotice,
  onOpenSettings,
  focus,
  onFocusConsumed,
}: DocumentsPanelProps) {
  const [status, setStatus] = useState<DocumentStatusDto | null>(null);
  const [recent, setRecent] = useState<DocumentMetaDto[]>([]);
  const [hits, setHits] = useState<DocumentHitDto[]>([]);
  const [queryInput, setQueryInput] = useState("");
  const [docType, setDocType] = useState<DocumentType | "">("");
  const [searched, setSearched] = useState(false);
  const [openId, setOpenId] = useState("");
  const [reading, setReading] = useState(false);
  const [readResult, setReadResult] = useState<DocumentReadResultDto | null>(
    null,
  );
  const [scanning, setScanning] = useState(false);
  const [report, setReport] = useState<IndexReportDto[]>([]);
  const [errorText, setErrorText] = useState("");

  const reload = useCallback(async () => {
    try {
      const [nextStatus, nextRecent] = await Promise.all([
        knowledgeClient.documentsStatus(),
        knowledgeClient.documentsRecent(8),
      ]);
      setStatus(nextStatus);
      setRecent(nextRecent);
      setErrorText("");
    } catch (error) {
      setStatus(null);
      setRecent([]);
      setErrorText(errorMessage(error));
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void reload();
  }, [active, reload]);

  const read = useCallback(
    async (documentId: string, chunkId?: string | null) => {
      setOpenId(documentId);
      setReading(true);
      try {
        setReadResult(
          await knowledgeClient.documentsRead({
            document_id: documentId,
            chunk_id: chunkId ?? undefined,
            max_chars: 4000,
          }),
        );
      } catch (error) {
        setReadResult(null);
        setNotice(errorMessage(error));
      } finally {
        setReading(false);
      }
    },
    [setNotice],
  );

  const toggle = (hit: DocumentHitDto) => {
    const documentId = hit.meta.document_id;
    if (openId === documentId) {
      setOpenId("");
      setReadResult(null);
      return;
    }
    void read(documentId, hit.chunk_id ?? null);
  };

  /** AI 的 `open_document` Action：切到本页并直接打开该文档（消费后清空）。 */
  useEffect(() => {
    if (!active || !focus) return;
    void read(focus.documentId).finally(() => onFocusConsumed?.());
  }, [active, focus, read, onFocusConsumed]);

  const search = async () => {
    const text = queryInput.trim();
    setSearched(true);
    if (!text) {
      setHits([]);
      return;
    }
    try {
      setHits(
        await knowledgeClient.documentsSearch({
          query: text,
          documentType: docType || undefined,
          limit: 30,
        }),
      );
      setErrorText("");
    } catch (error) {
      setHits([]);
      setErrorText(errorMessage(error));
    }
  };

  const scan = async () => {
    if (scanning) return;
    setScanning(true);
    try {
      const reports = await knowledgeClient.documentsScan();
      setReport(reports);
      const indexed = reports.reduce((total, entry) => total + entry.indexed, 0);
      const failed = reports.reduce((total, entry) => total + entry.failed, 0);
      setNotice(
        reports.some((entry) => entry.truncated)
          ? `重新索引完成：更新 ${indexed} 篇（已达文件数上限，部分目录被跳过）`
          : failed > 0
            ? `重新索引完成：更新 ${indexed} 篇，${failed} 篇失败`
            : `重新索引完成：更新 ${indexed} 篇`,
      );
      await reload();
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setScanning(false);
    }
  };

  const configured = status?.configured ?? false;
  const roots = status?.roots ?? [];

  return (
    <div className="knowledge-panel documents-panel">
      <div className="knowledge-statusbar">
        <span className="knowledge-stat">
          <b>{status?.documents ?? "—"}</b>文档
        </span>
        <span className="knowledge-stat">
          <b>{status?.chunks ?? "—"}</b>片段
        </span>
        <span className="knowledge-stat">
          <b>{status?.content_available ?? "—"}</b>可读内容
        </span>
        <span className="knowledge-stat">
          <b>{status?.metadata_only ?? "—"}</b>仅元数据
        </span>
        <span className="knowledge-stat">
          <b>{status?.failed ?? "—"}</b>失败
        </span>
        <button
          className="knowledge-refresh"
          onClick={() => void scan()}
          disabled={scanning || !configured}
          title={configured ? "重新索引文档目录" : "未配置文档目录"}
        >
          <ArrowClockwise size={14} />
          {scanning ? "索引中…" : "重新索引"}
        </button>
      </div>

      {roots.length > 0 ? (
        <ul className="knowledge-roots">
          {roots.map((root) => (
            <li key={root.id} className={root.enabled ? "" : "disabled"}>
              <FolderOpen size={13} />
              <b>{root.label}</b>
              <span>{root.path}</span>
              {root.enabled ? null : <em>已停用</em>}
            </li>
          ))}
        </ul>
      ) : null}

      {errorText ? <p className="knowledge-muted">{errorText}</p> : null}

      {!configured ? (
        <div className="knowledge-empty">
          <p>未配置文档目录：请在 设置 → 知识 中添加允许目录。</p>
          <button className="knowledge-primary" onClick={onOpenSettings}>
            打开设置
          </button>
        </div>
      ) : (
        <>
          <div className="knowledge-toolbar">
            <div className="knowledge-search">
              <FileText size={14} />
              <input
                type="search"
                placeholder="搜索文档内容…"
                value={queryInput}
                onChange={(event) => setQueryInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void search();
                }}
              />
              <button onClick={() => void search()}>搜索</button>
            </div>
            <select
              value={docType}
              onChange={(event) => {
                setDocType(event.target.value as DocumentType | "");
              }}
              title="按类型筛选"
            >
              <option value="">全部类型</option>
              {DOCUMENT_TYPE_OPTIONS.map((option) => (
                <option key={option} value={option}>
                  {DOCUMENT_TYPE_LABELS[option]}
                </option>
              ))}
            </select>
          </div>

          {report.length > 0 ? (
            <ul className="knowledge-report">
              {report.map((entry) => (
                <li key={entry.root_id}>
                  <b>{entry.root_id}</b>
                  <span>
                    扫描 {entry.scanned} · 更新 {entry.indexed} · 未变{" "}
                    {entry.unchanged} · 仅元数据 {entry.metadata_only} · 失败{" "}
                    {entry.failed} · 移除 {entry.removed} · {entry.duration_ms}ms
                    {entry.truncated ? " · 已达上限" : ""}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}

          {searched ? (
            hits.length === 0 ? (
              <div className="knowledge-empty">没有匹配的文档片段。</div>
            ) : (
              <ul className="doc-hit-list">
                {hits.map((hit) => (
                  <li
                    key={`${hit.meta.document_id}-${hit.chunk_id ?? "head"}`}
                    className={
                      "doc-hit" +
                      (openId === hit.meta.document_id ? " open" : "")
                    }
                  >
                    <button
                      className="doc-hit-main"
                      onClick={() => toggle(hit)}
                    >
                      {openId === hit.meta.document_id ? (
                        <CaretDown size={13} />
                      ) : (
                        <CaretRight size={13} />
                      )}
                      <span className="doc-hit-title">{hit.meta.title}</span>
                      <span className="memory-badge">
                        {typeLabel(hit.meta.document_type)}
                      </span>
                      {hit.location ? (
                        <span className="doc-hit-location">
                          {hit.location}
                        </span>
                      ) : null}
                      <span className="doc-hit-time">
                        {formatRelativeTime(hit.meta.modified_at)}
                      </span>
                    </button>
                    <p className="doc-hit-path">{hit.meta.relative_path}</p>
                    {hit.snippet ? (
                      <p className="doc-hit-snippet">{hit.snippet}</p>
                    ) : null}
                    {openId === hit.meta.document_id ? (
                      <DocReadView reading={reading} result={readResult} />
                    ) : null}
                  </li>
                ))}
              </ul>
            )
          ) : null}

          <section className="knowledge-subsection">
            <h4>最近索引</h4>
            {recent.length === 0 ? (
              <div className="knowledge-empty">
                还没有索引任何文档。点击「重新索引」开始。
              </div>
            ) : (
              <ul className="doc-recent">
                {recent.map((doc) => (
                  <li key={doc.document_id}>
                    <button
                      className={openId === doc.document_id ? "open" : ""}
                      onClick={() => {
                        if (openId === doc.document_id) {
                          setOpenId("");
                          setReadResult(null);
                          return;
                        }
                        void read(doc.document_id);
                      }}
                    >
                      <span className="doc-hit-title">{doc.title}</span>
                      <span className="memory-badge">
                        {typeLabel(doc.document_type)}
                      </span>
                      <span className="doc-hit-path">{doc.relative_path}</span>
                      <span className="doc-hit-time">
                        {formatSize(doc.size_bytes)} ·{" "}
                        {formatRelativeTime(doc.modified_at)}
                      </span>
                    </button>
                    {doc.index_error ? (
                      <p className="knowledge-muted">{doc.index_error}</p>
                    ) : null}
                    {openId === doc.document_id ? (
                      <DocReadView reading={reading} result={readResult} />
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}
