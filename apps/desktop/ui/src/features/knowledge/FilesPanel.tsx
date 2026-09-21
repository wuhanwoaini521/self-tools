/**
 * Files 面板（V6 §77）：允许根内文件的元数据搜索与安全文本读取。
 *
 * 只做「搜索 + 元数据 + 只读文本」，不做文件树 / Finder 克隆；所有读取都经后端
 * `FileService::authorize`（允许根 + deny 规则），前端不自行拼接路径。
 * 打开文件走系统默认程序（`@tauri-apps/plugin-opener`），浏览器预览下禁用。
 */
import {
  ArrowClockwise,
  CaretDown,
  CaretRight,
  FolderOpen,
  Lock,
  MagnifyingGlass,
} from "@phosphor-icons/react";
import { openPath } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import { errorMessage, formatRelativeTime, isTauriRuntime } from "../../utils";
import { knowledgeClient } from "./knowledgeClient";
import type {
  FileIndexReportDto,
  FileMetadataDto,
  FileReadResultDto,
  FileStatusDto,
} from "./knowledgeTypes";

interface FilesPanelProps {
  active: boolean;
  setNotice: (message: string) => void;
  onOpenSettings: () => void;
}

function formatSize(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function FilesPanel({
  active,
  setNotice,
  onOpenSettings,
}: FilesPanelProps) {
  const [status, setStatus] = useState<FileStatusDto | null>(null);
  const [items, setItems] = useState<FileMetadataDto[]>([]);
  const [queryInput, setQueryInput] = useState("");
  const [extension, setExtension] = useState("");
  const [rootId, setRootId] = useState("");
  const [searched, setSearched] = useState(false);
  const [openId, setOpenId] = useState("");
  const [reading, setReading] = useState(false);
  const [readResult, setReadResult] = useState<FileReadResultDto | null>(null);
  const [readError, setReadError] = useState("");
  const [scanning, setScanning] = useState(false);
  const [report, setReport] = useState<FileIndexReportDto[]>([]);
  const [errorText, setErrorText] = useState("");

  const reload = useCallback(async () => {
    try {
      const [nextStatus, nextItems] = await Promise.all([
        knowledgeClient.filesStatus(),
        knowledgeClient.filesSearch({ limit: 50 }),
      ]);
      setStatus(nextStatus);
      setItems(nextItems);
      setErrorText("");
    } catch (error) {
      setStatus(null);
      setItems([]);
      setErrorText(errorMessage(error));
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void reload();
  }, [active, reload]);

  const search = async () => {
    setSearched(true);
    try {
      setItems(
        await knowledgeClient.filesSearch({
          query: queryInput.trim() || undefined,
          extension: extension.trim().replace(/^\./, "") || undefined,
          rootId: rootId || undefined,
          limit: 50,
        }),
      );
      setErrorText("");
    } catch (error) {
      setItems([]);
      setErrorText(errorMessage(error));
    }
  };

  const scan = async () => {
    if (scanning) return;
    setScanning(true);
    try {
      const reports = await knowledgeClient.filesScan();
      setReport(reports);
      const indexed = reports.reduce((total, entry) => total + entry.indexed, 0);
      setNotice(
        reports.some((entry) => entry.truncated)
          ? `重新扫描完成：更新 ${indexed} 个文件（已达文件数上限，部分目录被跳过）`
          : `重新扫描完成：更新 ${indexed} 个文件`,
      );
      await reload();
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setScanning(false);
    }
  };

  const openWithSystem = async (file: FileMetadataDto) => {
    if (!isTauriRuntime()) {
      setNotice("浏览器预览不支持打开本地文件。");
      return;
    }
    try {
      const result = await knowledgeClient.filesOpen(file.file_id);
      const target = result.action?.target as { path?: unknown } | undefined;
      const path =
        typeof target?.path === "string" ? target.path : result.file.path;
      await openPath(path);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const readText = async (file: FileMetadataDto) => {
    if (openId === file.file_id) {
      setOpenId("");
      setReadResult(null);
      setReadError("");
      return;
    }
    setOpenId(file.file_id);
    setReadResult(null);
    setReadError("");
    setReading(true);
    try {
      setReadResult(await knowledgeClient.filesReadText(file.file_id));
    } catch (error) {
      setReadError(errorMessage(error));
    } finally {
      setReading(false);
    }
  };

  const configured = status?.configured ?? false;
  const roots = status?.roots ?? [];

  return (
    <div className="knowledge-panel files-panel">
      <div className="knowledge-statusbar">
        <span className="knowledge-stat">
          <b>{status?.files ?? "—"}</b>文件
        </span>
        <span className="knowledge-stat">
          <b>{status?.text_files ?? "—"}</b>文本
        </span>
        <span className="knowledge-stat">
          <b>{status?.binary_files ?? "—"}</b>二进制
        </span>
        <span className="knowledge-stat">
          <b>{status?.restricted ?? "—"}</b>受限
        </span>
        <span className="knowledge-stat">
          <b>{status?.failed ?? "—"}</b>失败
        </span>
        <button
          className="knowledge-refresh"
          onClick={() => void scan()}
          disabled={scanning || !configured}
          title={configured ? "重新扫描允许目录" : "未配置允许目录"}
        >
          <ArrowClockwise size={14} />
          {scanning ? "扫描中…" : "重新扫描"}
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
              <MagnifyingGlass size={14} />
              <input
                type="search"
                placeholder="搜索文件名 / 路径…"
                value={queryInput}
                onChange={(event) => setQueryInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void search();
                }}
              />
              <button onClick={() => void search()}>搜索</button>
            </div>
            <input
              className="knowledge-extension"
              type="text"
              placeholder="扩展名，如 md"
              value={extension}
              onChange={(event) => setExtension(event.target.value)}
            />
            <select
              value={rootId}
              onChange={(event) => setRootId(event.target.value)}
              title="按允许根筛选"
            >
              <option value="">全部目录</option>
              {roots.map((root) => (
                <option key={root.id} value={root.id}>
                  {root.label}
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
                    {entry.unchanged} · 受限 {entry.restricted} · 移除{" "}
                    {entry.removed} · {entry.duration_ms}ms
                    {entry.truncated ? " · 已达上限" : ""}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}

          {items.length === 0 ? (
            <div className="knowledge-empty">
              {searched
                ? "没有匹配的文件。"
                : "允许目录中还没有索引文件。点击「重新扫描」开始。"}
            </div>
          ) : (
            <ul className="file-list">
              {items.map((file) => (
                <li
                  key={file.file_id}
                  className={
                    "file-row" +
                    (openId === file.file_id ? " open" : "") +
                    (file.restricted ? " restricted" : "")
                  }
                >
                  <div className="file-row-head">
                    <button
                      className="file-row-main"
                      title={file.path}
                      onClick={() => void readText(file)}
                    >
                      {openId === file.file_id ? (
                        <CaretDown size={13} />
                      ) : (
                        <CaretRight size={13} />
                      )}
                      <span className="file-name">{file.file_name}</span>
                      {file.extension ? (
                        <span className="memory-badge">{file.extension}</span>
                      ) : null}
                      {file.restricted ? (
                        <span className="memory-badge sensitivity private">
                          <Lock size={11} />
                          受限
                        </span>
                      ) : null}
                      <span className="file-meta">
                        {formatSize(file.size_bytes)} ·{" "}
                        {formatRelativeTime(file.modified_at)}
                      </span>
                    </button>
                    <div className="file-row-actions">
                      <button
                        className="memory-action"
                        onClick={() => void readText(file)}
                      >
                        查看文本
                      </button>
                      <button
                        className="memory-action primary"
                        disabled={!isTauriRuntime() || file.restricted}
                        title={
                          file.restricted
                            ? "该文件被规则限制"
                            : isTauriRuntime()
                              ? "用系统默认程序打开"
                              : "浏览器预览不可用"
                        }
                        onClick={() => void openWithSystem(file)}
                      >
                        打开
                      </button>
                    </div>
                  </div>
                  <p className="file-path">{file.relative_path}</p>
                  {file.index_error ? (
                    <p className="knowledge-muted">{file.index_error}</p>
                  ) : null}
                  {openId === file.file_id ? (
                    <div className="file-read">
                      {reading ? (
                        <p className="knowledge-muted">读取中…</p>
                      ) : readError ? (
                        <p className="knowledge-muted">{readError}</p>
                      ) : readResult ? (
                        <>
                          <pre className="doc-read-text">
                            {readResult.text}
                          </pre>
                          <p className="knowledge-muted">
                            共 {readResult.char_count} 字符
                            {readResult.truncated
                              ? "（已截断，仅显示上限内的内容）"
                              : ""}
                          </p>
                        </>
                      ) : null}
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}
