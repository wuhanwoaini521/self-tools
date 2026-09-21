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
  PaperPlaneTilt,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "../../utils";
import { aiClient } from "./aiClient";
import {
  type AgentAction,
  type AgentMessage,
  type AgentResponse,
  type AppContextPayload,
  type EntityListItem,
  type UiBlock,
  entityListItems,
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
  onOpenSettings: () => void;
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
  onOpenSettings,
}: AIPanelProps) {
  const [status, setStatus] = useState<AiPanelState>("ready");
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [input, setInput] = useState("");
  const [toolTrace, setToolTrace] = useState<AgentResponse["tool_trace"]>([]);
  const [blocks, setBlocks] = useState<UiBlock[]>([]);
  const [errorText, setErrorText] = useState("");
  const [capabilities, setCapabilities] = useState<string[]>([]);
  const sessionRef = useRef(generateSessionId());

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
      setBlocks(response.ui_blocks ?? []);
      setStatus("ready");
    } catch (cause) {
      setStatus("error");
      setErrorText(errorMessage(cause));
    }
  }, [input, status, context, capabilities]);

  const clearConversation = useCallback(() => {
    sessionRef.current = generateSessionId();
    setMessages([]);
    setToolTrace([]);
    setBlocks([]);
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
                <BlockView key={index} block={block} onNavigate={onNavigate} />
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

/** 渲染结构化 UI Block（V4 §15）：EntityList 完整渲染，其余 kind 优雅降级。 */
function BlockView({
  block,
  onNavigate,
}: {
  block: UiBlock;
  onNavigate: (action: AgentAction) => void;
}) {
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
              onNavigate={onNavigate}
            />
          ))}
        </ul>
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
