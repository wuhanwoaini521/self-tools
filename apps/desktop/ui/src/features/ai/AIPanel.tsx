/**
 * AI Panel（V4 §36-§40）：侧边 Context-aware 对话面板。
 *
 * - 顶部：上下文 chip（当前模块/实体）+ 清除上下文；可关闭。
 * - 中部：消息流（user/assistant/tool trace），结构化 UI Block 与 Action 按钮。
 * - 底部：输入框 + 发送。
 * - 状态：unconfigured（未配置模型）/ loading（思考/调工具）/ error / ready。
 *
 * Frontend 只消费 AgentResponse；不接触 Provider/Prompt/Tool schema（V4 §56）。
 * 关闭 Panel 只是隐藏（V4 §37 侧栏而非新页面）。
 */
import {
  ArrowClockwise,
  CaretRight,
  PaperPlaneTilt,
  ShieldWarning,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, formatRelativeTime } from "../../utils";
import { MemoryConfirmCard } from "../knowledge/MemoryConfirmCard";
import {
  DOCUMENT_TYPE_LABELS,
  MEMORY_CATEGORY_LABELS,
  MEMORY_SOURCE_LABELS,
  MEMORY_STATUS_LABELS,
  type ConfirmMemoryTarget,
  type OpenDocumentTarget,
  type OpenFileTarget,
} from "../knowledge/knowledgeTypes";
import type { ConfirmationDto } from "../server/serverTypes";
import { aiClient } from "./aiClient";
import {
  agentRoleLabel,
  agentStateLabel,
  decisionConfidenceLabel,
  decisionProviderLabel,
  decisionStrategyLabel,
  type AgentAction,
  type AgentMessage,
  type AgentResponse,
  type AppContextPayload,
  type DocumentListItem,
  type EntityListItem,
  type FileListItem,
  type MemoryListItem,
  type OrchestrationTrace,
  type UiBlock,
  documentCardData,
  documentListItems,
  documentReferenceData,
  entityListItems,
  fileListItems,
  memoryListItems,
} from "./aiTypes";

export type AiPanelState = "unconfigured" | "ready" | "loading" | "error";

export interface AIPanelProps {
  open: boolean;
  onClose: () => void;
  /** 当前页面上下文（null/空 = General）。 */
  context: AppContextPayload | null;
  /** 上下文 chip 文案，如 "History · 毛泽东"；null = General */
  contextLabel: string | null;
  onClearContext: () => void;
  /** 执行 Action 请求（Frontend 决定是否执行，V4 §51）。 */
  onNavigate: (action: AgentAction) => void;
  /** 确认一条记忆（memory_save / memory_confirm，V6 §25）。 */
  onConfirmMemory?: (target: ConfirmMemoryTarget) => Promise<void>;
  /** 放弃一条记忆（带 memory_id 时由调用方 memory_reject）。 */
  onDismissMemory?: (target: ConfirmMemoryTarget) => Promise<void>;
  /** 用系统默认程序打开文件（`open_file` Action）。 */
  onOpenFile?: (target: OpenFileTarget) => void;
  /** 跳转 Knowledge 页并打开文档（`open_document` Action）。 */
  onOpenDocument?: (target: OpenDocumentTarget) => void;
  /**
   * V7：SYSTEM 操作确认卡（`confirm_action`）。前端负责调用
   * `confirm_action` / `cancel_action` 命令；**模型不参与执行**（§68/§69）。
   */
  onConfirmSystemAction?: (
    confirmation: ConfirmationDto,
    decision: "confirm" | "cancel",
  ) => Promise<void>;
  onOpenSettings: () => void;
}

/** `confirm_action` 的 target（后端 Action.target 形状）。 */
interface SystemConfirmTarget {
  confirmation_id: string;
  action_type: string;
  target_id: string;
  risk: string;
  expires_at: number;
  label?: string;
}

/** 记忆候选的稳定 key（同一候选只渲染一张确认卡）。 */
function confirmKey(target: ConfirmMemoryTarget): string {
  return target.memory_id ?? `new:${target.category}:${target.content}`;
}

/** Action / UI Block 里的记忆候选是否形状完整（后端适配器产出，宽松校验）。 */
function asConfirmActionTarget(raw: unknown): SystemConfirmTarget | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  if (typeof record.confirmation_id !== "string") return null;
  if (typeof record.action_type !== "string") return null;
  if (typeof record.target_id !== "string") return null;
  return {
    confirmation_id: record.confirmation_id,
    action_type: record.action_type,
    target_id: record.target_id,
    risk: typeof record.risk === "string" ? record.risk : "system",
    expires_at: typeof record.expires_at === "number" ? record.expires_at : 0,
    label: typeof record.label === "string" ? record.label : undefined,
  };
}

function asConfirmTarget(raw: unknown): ConfirmMemoryTarget | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  if (typeof record.content !== "string" || !record.content.trim()) return null;
  if (typeof record.category !== "string") return null;
  return {
    memory_id:
      typeof record.memory_id === "string" ? record.memory_id : null,
    category: record.category as ConfirmMemoryTarget["category"],
    content: record.content,
    source_type:
      typeof record.source_type === "string"
        ? (record.source_type as ConfirmMemoryTarget["source_type"])
        : null,
    source_reference:
      typeof record.source_reference === "string"
        ? record.source_reference
        : null,
  };
}

function generateSessionId(): string {
  return `ui-${Date.now().toString(36)}-${Math.floor(Math.random() * 0xffffff).toString(36)}`;
}

/** 当前会话内最大渲染条数（后端已裁剪快照，这里再兜底）。 */
const MAX_MESSAGES = 60;

export function AIPanel({
  open,
  onClose,
  context,
  contextLabel,
  onClearContext,
  onNavigate,
  onConfirmMemory,
  onDismissMemory,
  onOpenFile,
  onOpenDocument,
  onConfirmSystemAction,
  onOpenSettings,
}: AIPanelProps) {
  const [status, setStatus] = useState<AiPanelState>("ready");
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [input, setInput] = useState("");
  const [toolTrace, setToolTrace] = useState<AgentResponse["tool_trace"]>([]);
  const [orchestration, setOrchestration] = useState<AgentResponse["orchestration"]>(null);
  const [blocks, setBlocks] = useState<UiBlock[]>([]);
  const [pendingConfirms, setPendingConfirms] = useState<ConfirmMemoryTarget[]>(
    [],
  );
  const [confirmBusy, setConfirmBusy] = useState("");
  const [errorText, setErrorText] = useState("");
  const [capabilities, setCapabilities] = useState<string[]>([]);
  const sessionRef = useRef(generateSessionId());

  /** 记忆确认卡：确认 → memory_save / memory_confirm；放弃 → memory_reject（或仅关闭）。 */
  const resolveConfirm = useCallback(
    async (target: ConfirmMemoryTarget, accepted: boolean) => {
      const key = confirmKey(target);
      setConfirmBusy(key);
      try {
        if (accepted) await onConfirmMemory?.(target);
        else await onDismissMemory?.(target);
      } catch (cause) {
        setErrorText(errorMessage(cause));
      } finally {
        setConfirmBusy("");
        setPendingConfirms((current) =>
          current.filter((entry) => confirmKey(entry) !== key),
        );
      }
    },
    [onConfirmMemory, onDismissMemory],
  );

  const enqueueConfirm = useCallback((target: ConfirmMemoryTarget) => {
    setPendingConfirms((current) =>
      current.some((entry) => confirmKey(entry) === confirmKey(target))
        ? current
        : [...current, target],
    );
  }, []);

  /** V7：SYSTEM 确认卡队列（同一票据只显示一张）。 */
  const [systemConfirms, setSystemConfirms] = useState<SystemConfirmTarget[]>([]);
  const enqueueSystemConfirm = useCallback((target: SystemConfirmTarget) => {
    setSystemConfirms((current) =>
      current.some((entry) => entry.confirmation_id === target.confirmation_id)
        ? current
        : [...current, target],
    );
  }, []);

  /** 打开时拉取状态（No API Key Gate：未配置只是面板提示，App 不崩溃）。 */
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void (async () => {
      try {
        const state = await aiClient.status();
        if (cancelled) return;
        setCapabilities(state.modules.map((module) => module.id));
        setStatus(state.configured ? "ready" : "unconfigured");
      } catch (cause) {
        if (cancelled) return;
        setStatus("unconfigured");
        setErrorText(errorMessage(cause));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open]);

  const send = useCallback(async () => {
    const text = input.trim();
    if (!text || status === "loading") return;
    setInput("");
    setErrorText("");
    setStatus("loading");
    setToolTrace([]);
    try {
      const response = await aiClient.chat({
        message: text,
        session_id: sessionRef.current,
        app_context: context ?? {},
        capabilities,
        locale: "zh-CN",
      });
      setMessages(response.messages.slice(-MAX_MESSAGES));
      setToolTrace(response.tool_trace ?? []);
      setOrchestration(response.orchestration ?? null);
      setBlocks(response.ui_blocks ?? []);
      // V6：知识类 Action —— 确认记忆在面板内联确认，打开文件/文档交给外壳执行。
      // V7：`confirm_action` 进服务器确认卡（SYSTEM 操作，§56）；`open_app`
      // 用系统默认浏览器打开注册表内的 URL（§46）。
      for (const action of response.actions ?? []) {
        if (action.type === "confirm_memory") {
          const target = asConfirmTarget(action.target);
          if (target) enqueueConfirm(target);
        } else if (action.type === "confirm_action") {
          const target = asConfirmActionTarget(action.target);
          if (target) enqueueSystemConfirm(target);
        } else if (action.type === "open_file" && onOpenFile) {
          onOpenFile(action.target as unknown as OpenFileTarget);
        } else if (action.type === "open_document" && onOpenDocument) {
          onOpenDocument(action.target as unknown as OpenDocumentTarget);
        } else if (action.type === "open_app") {
          const target = action.target as { url?: string | null };
          if (typeof target.url === "string" && target.url) {
            void openUrl(target.url);
          }
        }
      }
      setStatus("ready");
    } catch (cause) {
      setStatus("error");
      setErrorText(errorMessage(cause));
    }
  }, [
    input,
    status,
    context,
    capabilities,
    enqueueConfirm,
    onOpenFile,
    onOpenDocument,
  ]);

  const clearConversation = useCallback(() => {
    sessionRef.current = generateSessionId();
    setMessages([]);
    setToolTrace([]);
    setBlocks([]);
    setPendingConfirms([]);
    setErrorText("");
  }, []);

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (
        event.key === "Enter" &&
        !event.shiftKey &&
        !event.nativeEvent.isComposing
      ) {
        event.preventDefault();
        void send();
      }
    },
    [send],
  );

  if (!open) return null;

  return (
    <aside className="ai-panel" aria-label="AI 助手">
      <header className="ai-panel-head">
        <div className="ai-panel-title">
          <Sparkle size={16} weight="fill" />
          <strong>Ask AI</strong>
        </div>
        <div className="ai-panel-actions">
          <button
            className="ai-icon-button"
            title="新建对话"
            onClick={clearConversation}
            disabled={status === "loading"}
          >
            <ArrowClockwise size={15} />
          </button>
          <button className="ai-icon-button" title="关闭" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
      </header>

      <div className="ai-panel-context">
        <span className="ai-context-label">当前上下文</span>
        {contextLabel ? (
          <>
            <span
              className="ai-context-chip"
              title="AI 可据此解析“这个/他/这里”"
            >
              {contextLabel}
            </span>
            <button
              className="ai-context-clear"
              onClick={onClearContext}
              title="清除上下文（回到 General）"
            >
              清除
            </button>
          </>
        ) : (
          <span className="ai-context-chip general">General</span>
        )}
      </div>

      {status === "unconfigured" ? (
        <div className="ai-panel-unconfigured">
          <Sparkle size={28} />
          <p>AI provider 未配置。</p>
          <p className="ai-unconfigured-hint">
            在设置中填写 OpenAI 兼容的 base_url 与 model （本地 Ollama 可留空
            key）。普通模块不受影响。
          </p>
          {errorText ? (
            <p className="ai-unconfigured-error">{errorText}</p>
          ) : null}
          <div className="ai-unconfigured-actions">
            <button className="ai-go-settings" onClick={onOpenSettings}>
              打开设置
            </button>
            <button className="ai-close-hint" onClick={onClose}>
              关闭
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="ai-panel-messages" data-testid="ai-messages">
            {messages.length === 0 ? (
              <p className="ai-empty-hint">
                问点什么吧 —— AI 会结合当前页面上下文回答， 需要数据时会调用
                self-tools 的工具。
              </p>
            ) : (
              messages.map((message, index) => (
                <MessageRow key={index} message={message} />
              ))
            )}
            {status === "loading" ? (
              <div className="ai-loading">
                <span className="ai-dot" />
                <span className="ai-dot" />
                <span className="ai-dot" />
                <span className="ai-loading-text">思考中…</span>
              </div>
            ) : null}
          </div>
          {status === "error" ? (
            <div className="ai-panel-error">
              {errorText || "发生错误，请重试。"}
            </div>
          ) : null}
          {orchestration ? (
            <OrchestrationTraceView trace={orchestration} />
          ) : null}
          {toolTrace.length > 0 ? (
            <div className="ai-tool-trace">
              <span className="ai-trace-label">工具调用</span>
              {toolTrace.map((entry, index) => (
                <span
                  key={`${entry.tool}-${index}`}
                  className={"ai-trace-chip" + (entry.ok ? "" : " failed")}
                  title={entry.note ?? entry.tool}
                >
                  {entry.ok ? "✓" : "✕"} {entry.tool} · {entry.duration_ms}ms
                </span>
              ))}
            </div>
          ) : null}
          {blocks.length > 0 ? (
            <div className="ai-panel-blocks">
              {blocks.map((block, index) => (
                <BlockView
                  key={index}
                  block={block}
                  onNavigate={onNavigate}
                  onOpenFile={onOpenFile}
                  onOpenDocument={onOpenDocument}
                  onConfirmMemory={(target) => void resolveConfirm(target, true)}
                  onDismissMemory={(target) =>
                    void resolveConfirm(target, false)
                  }
                  confirmBusyKey={confirmBusy}
                />
              ))}
            </div>
          ) : null}
          {pendingConfirms.length > 0 ? (
            <div className="ai-panel-confirms">
              {pendingConfirms.map((target) => (
                <MemoryConfirmCard
                  key={confirmKey(target)}
                  target={target}
                  busy={confirmBusy === confirmKey(target)}
                  onConfirm={() => void resolveConfirm(target, true)}
                  onDismiss={() => void resolveConfirm(target, false)}
                />
              ))}
            </div>
          ) : null}
          {systemConfirms.length > 0 ? (
            <div className="ai-panel-confirms">
              {systemConfirms.map((target) => (
                <SystemConfirmCard
                  key={target.confirmation_id}
                  target={target}
                  onDecide={(decision) => {
                    if (!onConfirmSystemAction) return;
                    const dto: ConfirmationDto = {
                      confirmation_id: target.confirmation_id,
                      action_type: target.action_type,
                      target_id: target.target_id,
                      summary: target.label ?? target.target_id,
                      risk: target.risk as ConfirmationDto["risk"],
                      created_at: Math.floor(Date.now() / 1000),
                      expires_at: target.expires_at,
                    };
                    void onConfirmSystemAction(dto, decision).finally(() => {
                      setSystemConfirms((current) =>
                        current.filter(
                          (entry) =>
                            entry.confirmation_id !== target.confirmation_id,
                        ),
                      );
                    });
                  }}
                />
              ))}
            </div>
          ) : null}
          <footer className="ai-panel-input">
            <textarea
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="问点什么，或说“打开遵义会议”…"
              rows={2}
              disabled={status === "loading"}
            />
            <button
              className="ai-send"
              onClick={() => void send()}
              disabled={!input.trim() || status === "loading"}
              title="发送"
            >
              {status === "loading" ? (
                <ArrowClockwise size={16} />
              ) : (
                <PaperPlaneTilt size={16} />
              )}
            </button>
          </footer>
        </>
      )}
    </aside>
  );
}

function MessageRow({ message }: { message: AgentMessage }) {
  if (message.role === "user") {
    return <div className="ai-msg user">{message.content}</div>;
  }
  return (
    <div className="ai-msg assistant">
      <div className="ai-msg-text">{message.content}</div>
    </div>
  );
}

/** Block 渲染所需的 Action 回调（由 AIPanel 组装，App 执行）。 */
interface BlockHandlers {
  onNavigate: (action: AgentAction) => void;
  onOpenFile?: (target: OpenFileTarget) => void;
  onOpenDocument?: (target: OpenDocumentTarget) => void;
  onConfirmMemory: (target: ConfirmMemoryTarget) => void;
  onDismissMemory: (target: ConfirmMemoryTarget) => void;
  confirmBusyKey: string;
}

/** 渲染结构化 UI Block（V4 §15 / V6 §5）：已知 kind 完整渲染，其余优雅降级。 */
function BlockView({ block, ...handlers }: { block: UiBlock } & BlockHandlers) {
  if (block.kind === "entity_list") {
    const items = entityListItems(block);
    if (items.length === 0) return null;
    return (
      <section className="ai-block">
        <h4 className="ai-block-title">{block.title || "相关实体"}</h4>
        <ul className="ai-entity-list">
          {items.map((item, index) => (
            <EntityCard
              key={`${item.id ?? item.entity_id ?? index}-${index}`}
              item={item}
              onNavigate={handlers.onNavigate}
            />
          ))}
        </ul>
      </section>
    );
  }
  if (block.kind === "memory_list") {
    const items = memoryListItems(block);
    if (items.length === 0) return null;
    return (
      <section className="ai-block">
        <h4 className="ai-block-title">{block.title || "相关记忆"}</h4>
        <ul className="memory-list">
          {items.map((item, index) => (
            <MemoryListRow
              key={`${item.id}-${index}`}
              item={item}
              busyKey={handlers.confirmBusyKey}
              onConfirmMemory={handlers.onConfirmMemory}
              onDismissMemory={handlers.onDismissMemory}
            />
          ))}
        </ul>
      </section>
    );
  }
  if (block.kind === "document_list") {
    const items = documentListItems(block);
    if (items.length === 0) return null;
    return (
      <section className="ai-block">
        <h4 className="ai-block-title">{block.title || "相关文档"}</h4>
        <ul className="ai-doc-list">
          {items.map((item, index) => (
            <DocumentListRow
              key={`${item.document_id}-${index}`}
              item={item}
              onOpenDocument={handlers.onOpenDocument}
            />
          ))}
        </ul>
      </section>
    );
  }
  if (block.kind === "file_list") {
    const items = fileListItems(block);
    if (items.length === 0) return null;
    return (
      <section className="ai-block">
        <h4 className="ai-block-title">{block.title || "相关文件"}</h4>
        <ul className="ai-doc-list">
          {items.map((item, index) => (
            <FileListRow
              key={`${item.file_id ?? item.relative_path ?? index}-${index}`}
              item={item}
              onOpenFile={handlers.onOpenFile}
            />
          ))}
        </ul>
      </section>
    );
  }
  if (block.kind === "document_card") {
    const card = documentCardData(block);
    if (!card) return null;
    return (
      <section className="ai-block">
        <h4 className="ai-block-title">{block.title || card.title}</h4>
        <div className="ai-doc-card">
          <div className="ai-doc-card-head">
            <span className="memory-badge">
              {card.document_type
                ? (DOCUMENT_TYPE_LABELS[card.document_type] ??
                  card.document_type)
                : "文档"}
            </span>
            <span className="memory-badge muted">
              {card.content_available ? "可读内容" : "仅元数据"}
            </span>
            <span className="ai-doc-card-title">{card.title}</span>
          </div>
          <dl className="ai-doc-card-meta">
            <div>
              <dt>路径</dt>
              <dd>{card.relative_path ?? card.path ?? "—"}</dd>
            </div>
            <div>
              <dt>大小</dt>
              <dd>{formatBytes(card.size_bytes)}</dd>
            </div>
            <div>
              <dt>修改时间</dt>
              <dd>{formatRelativeTime(card.modified_at)}</dd>
            </div>
            <div>
              <dt>片段数</dt>
              <dd>{card.chunk_count ?? "—"}</dd>
            </div>
          </dl>
          {card.index_error ? (
            <p className="ai-doc-card-error">{card.index_error}</p>
          ) : null}
          {handlers.onOpenDocument ? (
            <button
              className="memory-action primary"
              onClick={() =>
                handlers.onOpenDocument?.({
                  document_id: card.document_id,
                  title: card.title,
                })
              }
            >
              在 Knowledge 中打开
            </button>
          ) : null}
        </div>
      </section>
    );
  }
  if (block.kind === "document_reference") {
    const reference = documentReferenceData(block);
    if (!reference) return null;
    return (
      <section className="ai-block">
        <h4 className="ai-block-title">{block.title || "引用来源"}</h4>
        <div className="ai-doc-reference">
          <button
            className="ai-doc-reference-main"
            disabled={!handlers.onOpenDocument}
            onClick={() =>
              handlers.onOpenDocument?.({
                document_id: reference.document_id,
                title: reference.title,
                location: reference.location ?? null,
              })
            }
          >
            <span className="ai-doc-reference-title">{reference.title}</span>
            {reference.location ? (
              <span className="ai-doc-reference-location">
                {reference.location}
              </span>
            ) : null}
          </button>
          {reference.snippet ? (
            <p className="ai-doc-reference-snippet">{reference.snippet}</p>
          ) : null}
        </div>
      </section>
    );
  }
  const raw = Array.isArray(block.data) ? block.data : [block.data];
  return (
    <section className="ai-block">
      <h4 className="ai-block-title">{block.title}</h4>
      <ul className="ai-generic-list">
        {raw.slice(0, 20).map((entry, index) => (
          <li key={index}>{formatValue(entry)}</li>
        ))}
      </ul>
    </section>
  );
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** `memory_list` 条目：候选（needs_confirmation）内联确认卡，其余只读展示。 */
function MemoryListRow({
  item,
  busyKey,
  onConfirmMemory,
  onDismissMemory,
}: {
  item: MemoryListItem;
  busyKey: string;
  onConfirmMemory: (target: ConfirmMemoryTarget) => void;
  onDismissMemory: (target: ConfirmMemoryTarget) => void;
}) {
  const label =
    item.category_label ?? MEMORY_CATEGORY_LABELS[item.category] ?? item.category;
  if (item.needs_confirmation) {
    const target: ConfirmMemoryTarget = {
      memory_id: item.id,
      category: item.category,
      content: item.content,
      source_type: item.source_type ?? null,
    };
    return (
      <li className="ai-block-plain">
        <MemoryConfirmCard
          target={target}
          busy={busyKey === confirmKey(target)}
          onConfirm={() => onConfirmMemory(target)}
          onDismiss={() => onDismissMemory(target)}
        />
      </li>
    );
  }
  return (
    <li className="memory-card">
      <div className="memory-card-head">
        <span className="memory-badge">{label}</span>
        {item.source_type ? (
          <span className="memory-badge muted">
            {MEMORY_SOURCE_LABELS[item.source_type] ?? item.source_type}
          </span>
        ) : null}
        {item.status ? (
          <span className="memory-badge status">
            {MEMORY_STATUS_LABELS[item.status] ?? item.status}
          </span>
        ) : null}
        <span className="memory-time">
          {formatRelativeTime(item.updated_at)}
        </span>
      </div>
      <p className="memory-content">{item.content}</p>
    </li>
  );
}

/** `document_list` 条目：点击 → `open_document` Action。 */
function DocumentListRow({
  item,
  onOpenDocument,
}: {
  item: DocumentListItem;
  onOpenDocument?: (target: OpenDocumentTarget) => void;
}) {
  return (
    <li className="doc-hit">
      <button
        className="doc-hit-main"
        disabled={!onOpenDocument}
        onClick={() =>
          onOpenDocument?.({
            document_id: item.document_id,
            title: item.title,
            location: item.location ?? null,
          })
        }
      >
        <span className="doc-hit-title">{item.title}</span>
        <span className="memory-badge">
          {item.document_type
            ? (DOCUMENT_TYPE_LABELS[item.document_type] ?? item.document_type)
            : "文档"}
        </span>
        {item.location ? (
          <span className="doc-hit-location">{item.location}</span>
        ) : null}
        <span className="doc-hit-time">
          {formatRelativeTime(item.modified_at)}
        </span>
      </button>
      {item.relative_path ? (
        <p className="doc-hit-path">{item.relative_path}</p>
      ) : null}
      {item.snippet ? <p className="doc-hit-snippet">{item.snippet}</p> : null}
    </li>
  );
}

/** `file_list` 条目：点击 → `open_file` Action（无 path 时不可点）。 */
function FileListRow({
  item,
  onOpenFile,
}: {
  item: FileListItem;
  onOpenFile?: (target: OpenFileTarget) => void;
}) {
  const path = item.path ?? null;
  return (
    <li className={"file-row" + (item.restricted ? " restricted" : "")}>
      <button
        className="doc-hit-main"
        disabled={!path || !onOpenFile}
        title={path ?? "缺少路径"}
        onClick={() => {
          if (!path) return;
          onOpenFile?.({
            file_id: item.file_id ?? null,
            path,
            file_name: item.file_name,
          });
        }}
      >
        <span className="file-name">{item.file_name}</span>
        {item.extension ? (
          <span className="memory-badge">{item.extension}</span>
        ) : null}
        {item.restricted ? (
          <span className="memory-badge sensitivity private">受限</span>
        ) : null}
        <span className="file-meta">
          {formatBytes(item.size_bytes)} ·{" "}
          {formatRelativeTime(item.modified_at)}
        </span>
      </button>
      {item.relative_path ? (
        <p className="file-path">{item.relative_path}</p>
      ) : null}
    </li>
  );
}

function EntityCard({
  item,
  onNavigate,
}: {
  item: EntityListItem;
  onNavigate: (action: AgentAction) => void;
}) {
  const id = item.id ?? item.entity_id;
  const title = item.title ?? item.name ?? "未命名";
  const kind = item.kind ?? "entity";
  const years =
    item.start_year != null || item.end_year != null
      ? `${item.start_year ?? "?"}–${item.end_year ?? "?"}`
      : null;
  const subtitle = item.subtitle ?? null;
  return (
    <li className="ai-entity-card">
      <button
        className="ai-entity-card-main"
        title={id ? `打开 ${title}` : undefined}
        onClick={() => {
          if (!id) return;
          onNavigate({
            type: "open_entity",
            module: "history",
            target: { kind, id },
          });
        }}
      >
        <span className="ai-entity-kind">{kind}</span>
        <span className="ai-entity-title">{title}</span>
        {subtitle ? (
          <span className="ai-entity-subtitle">{subtitle}</span>
        ) : null}
        {years ? <span className="ai-entity-years">{years}</span> : null}
      </button>
    </li>
  );
}

function formatValue(entry: unknown): string {
  if (typeof entry === "string") return entry;
  if (entry == null) return "";
  if (typeof entry === "object") {
    const record = entry as Record<string, unknown>;
    const label =
      record.title ?? record.name ?? record.id ?? record.term ?? record.work;
    if (typeof label === "string") return label;
    try {
      return JSON.stringify(record);
    } catch {
      return "";
    }
  }
  return String(entry);
}

/**
 * V7 SYSTEM 操作确认卡（§56/§82）。
 *
 * 与 Server Dashboard 的确认卡同语义：显示目标 / 风险 / 影响 / 过期倒计时；
 * 没有「跳过确认直接执行」的路径。
 */
function SystemConfirmCard({
  target,
  onDecide,
}: {
  target: {
    confirmation_id: string;
    action_type: string;
    target_id: string;
    risk: string;
    expires_at: number;
    label?: string;
  };
  onDecide: (decision: "confirm" | "cancel") => void;
}) {
  const secondsLeft = () =>
    Math.max(0, target.expires_at - Math.floor(Date.now() / 1000));
  const [remaining, setRemaining] = useState(secondsLeft);

  useEffect(() => {
    const timer = window.setInterval(() => setRemaining(secondsLeft()), 1_000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target.expires_at]);

  return (
    <section className="ai-system-confirm" role="dialog" aria-label="确认系统操作">
      <header>
        <ShieldWarning size={15} />
        AI 请求执行系统操作
      </header>
      <p className="ai-system-confirm-summary">
        重启服务「{target.label ?? target.target_id}」
      </p>
      <p className="ai-system-confirm-meta">
        {target.action_type} · {target.target_id} · 风险 {target.risk} ·{" "}
        {remaining > 0 ? `${remaining} 秒后失效` : "已过期"}
      </p>
      <p className="ai-system-confirm-impact">影响：服务将短暂停止后重新启动</p>
      <div className="ai-system-confirm-actions">
        <button
          className="danger"
          disabled={remaining <= 0}
          onClick={() => onDecide("confirm")}
        >
          确认执行
        </button>
        <button onClick={() => onDecide("cancel")}>取消</button>
      </div>
    </section>
  );
}

/**
 * 编排追踪（V9 §74/§75；V10 §20/§38：折叠区；只展示任务/角色/状态/工具数/
 * 来源/时长 + 决策 label —— 不展示模型隐藏推理）。
 */
function OrchestrationTraceView({ trace }: { trace: OrchestrationTrace }) {
  const [open, setOpen] = useState(false);
  const done = trace.runs.filter((run) => run.status === "completed").length;
  return (
    <div className="ai-orchestration">
      <button
        type="button"
        className="ai-orchestration-head"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className={"ai-orchestration-caret" + (open ? " open" : "")}>
          <CaretRight size={12} />
        </span>
        执行过程
        <span className="ai-orchestration-summary">
          {trace.decision_strategy
            ? ` · 决策 ${decisionStrategyLabel(trace.decision_strategy)}`
            : ""}
          {trace.decision_provider
            ? ` · ${decisionProviderLabel(trace.decision_provider)}`
            : ""}
          {trace.decision_confidence
            ? ` · 置信 ${decisionConfidenceLabel(trace.decision_confidence)}`
            : ""}
          {done}/{trace.runs.length} 个任务完成
          {trace.review ? ` · 审查 ${trace.review}` : ""}
          {trace.runs.some((run) => run.status === "failed" || run.status === "timed_out") ? " · 有失败" : ""}
          {trace.decision_fallback ? " · 已回落规则" : ""}
        </span>
      </button>
      {open ? (
        <>
          {trace.decision_strategy || trace.shadow_decision ? (
            <div className="ai-orchestration-decision">
              决策：
              {trace.decision_strategy
                ? decisionStrategyLabel(trace.decision_strategy)
                : "—"}
              {trace.decision_provider
                ? ` · 提供方 ${decisionProviderLabel(trace.decision_provider)}`
                : ""}
              {trace.decision_confidence
                ? ` · 置信度 ${decisionConfidenceLabel(trace.decision_confidence)}`
                : ""}
              {trace.decision_latency_ms != null
                ? ` · ${trace.decision_latency_ms}ms`
                : ""}
              {trace.decision_fallback ? " · 已回落到规则决策" : ""}
              {trace.shadow_decision
                ? ` · 影子决策 ${decisionStrategyLabel(trace.shadow_decision)}`
                : ""}
            </div>
          ) : null}
          <ul className="ai-orchestration-runs">
            {trace.runs.map((run) => (
              <li key={run.task_id} className={"ai-orchestration-run " + run.status}>
                <span className="ai-orchestration-status">
                  {run.status === "completed" ? "✓" : run.status === "failed" || run.status === "timed_out" ? "✕" : "•"}
                </span>
                <span className="ai-orchestration-role">
                  {agentRoleLabel(run.agent_id)}
                </span>
                <span className="ai-orchestration-task">{run.task_id}</span>
                <span className="ai-orchestration-meta">
                  {agentStateLabel(run.status)} · {run.tool_calls} 次工具 ·{" "}
                  {run.duration_ms}ms
                  {run.error_code ? ` · ${run.error_code}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}
