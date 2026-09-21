/**
 * Knowledge 页面（V6 Personal Knowledge Layer）。
 *
 * 三个 tab：Memory（记忆）/ Documents（文档索引）/ Files（允许目录内的文件）。
 * 页面只负责 tab 与 AppContext 上报；数据与命令全部下沉到各 Panel + knowledgeClient。
 */
import { Brain, FileText, FolderOpen } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import type { AppContextPayload } from "../ai/aiTypes";
import { DocumentsPanel } from "./DocumentsPanel";
import { FilesPanel } from "./FilesPanel";
import { MemoryPanel } from "./MemoryPanel";

export type KnowledgeTab = "memory" | "documents" | "files";

/** 外部（AI Action / 首页）请求打开某个 tab / 文档。 */
export interface KnowledgeIntent {
  tab?: KnowledgeTab;
  documentId?: string | null;
  nonce: number;
}

const TABS: { id: KnowledgeTab; label: string; icon: typeof Brain }[] = [
  { id: "memory", label: "Memory", icon: Brain },
  { id: "documents", label: "Documents", icon: FileText },
  { id: "files", label: "Files", icon: FolderOpen },
];

interface KnowledgePageProps {
  active: boolean;
  setNotice: (message: string) => void;
  onOpenSettings: () => void;
  intent?: KnowledgeIntent | null;
  onContextChange?: (ctx: AppContextPayload | null) => void;
}

export function KnowledgePage({
  active,
  setNotice,
  onOpenSettings,
  intent,
  onContextChange,
}: KnowledgePageProps) {
  const [tab, setTab] = useState<KnowledgeTab>("memory");
  const [documentFocus, setDocumentFocus] = useState<{
    documentId: string;
    nonce: number;
  } | null>(null);

  /** AppContext 桥：告诉 AI「我现在在 Knowledge 的哪个 tab」；离开时清空。 */
  useEffect(() => {
    if (!active || !onContextChange) return;
    onContextChange({ module: "knowledge", page: tab });
    return () => onContextChange(null);
  }, [active, tab, onContextChange]);

  /** AI 的 `open_document`：切到 Documents 并展开该文档。 */
  useEffect(() => {
    if (!intent) return;
    if (intent.tab) setTab(intent.tab);
    if (intent.documentId) {
      setTab("documents");
      setDocumentFocus({
        documentId: intent.documentId,
        nonce: intent.nonce,
      });
    }
  }, [intent]);

  return (
    <div className="knowledge-page">
      <header className="knowledge-head">
        <div className="knowledge-tabs">
          {TABS.map((entry) => (
            <button
              key={entry.id}
              className={tab === entry.id ? "active" : ""}
              onClick={() => setTab(entry.id)}
            >
              <entry.icon size={15} />
              {entry.label}
            </button>
          ))}
        </div>
        <p className="knowledge-subtitle">
          本地优先：记忆需要你确认才会生效；文件与文档只在你配置的允许目录内读取。
        </p>
      </header>

      {tab === "memory" ? (
        <MemoryPanel active={active} setNotice={setNotice} />
      ) : null}
      {tab === "documents" ? (
        <DocumentsPanel
          active={active}
          setNotice={setNotice}
          onOpenSettings={onOpenSettings}
          focus={documentFocus}
          onFocusConsumed={() => setDocumentFocus(null)}
        />
      ) : null}
      {tab === "files" ? (
        <FilesPanel
          active={active}
          setNotice={setNotice}
          onOpenSettings={onOpenSettings}
        />
      ) : null}
    </div>
  );
}
