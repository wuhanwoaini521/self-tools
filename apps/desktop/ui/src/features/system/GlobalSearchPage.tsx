/**
 * Global Search（V11 §119-§122）：跨模块统一检索与命令流 (Command Flow)。
 *
 * 铁律：
 * - 不依赖 LLM（§121）：本次检索零模型调用，即使 AI 不可用也能工作；
 * - 结果带 action_target，前端直接导航到来源模块；
 * - 单源失败只降级该源（degraded_sources），不影响其它结果；
 * - 空查询或前缀指令提供常用快捷操作与键盘导航。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Brain,
  Compass,
  FileText,
  Gear,
  Heartbeat,
  MagnifyingGlass,
  MapPin,
  NotePencil,
  Rss,
  Scroll,
  Sparkle,
  Translate,
} from "@phosphor-icons/react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, isTauriRuntime } from "../../utils";

export type SearchSourceName =
  | "history"
  | "travel"
  | "geography"
  | "language"
  | "memory"
  | "documents"
  | "files"
  | "study_board"
  | "applications";

export interface GlobalSearchHitDto {
  source: SearchSourceName;
  kind: string;
  title: string;
  snippet: string;
  action_target: Record<string, unknown>;
  score: number;
}

export interface GlobalSearchResultDto {
  query: string;
  hits: GlobalSearchHitDto[];
  degraded_sources: SearchSourceName[];
  total: number;
}

const SOURCE_LABELS: Record<string, string> = {
  history: "History",
  travel: "Travel",
  geography: "Geography",
  language: "Language",
  memory: "Memory",
  documents: "Documents",
  files: "Files",
  study_board: "Study Board",
  applications: "Applications",
};

export interface QuickActionItem {
  id: string;
  title: string;
  description: string;
  shortcut?: string;
  icon: typeof NotePencil;
  run: () => void;
  keywords: string[];
}

export interface GlobalSearchPageProps {
  active: boolean;
  /** 打开来源模块（action_target 的 module 字段）。 */
  onNavigate: (module: string, target: Record<string, unknown>) => void;
  onNewNote?: () => void;
  onRefreshRss?: () => void;
  onAskAi?: () => void;
  onOpenSettings?: () => void;
}

export function GlobalSearchPage({
  active,
  onNavigate,
  onNewNote,
  onRefreshRss,
  onAskAi,
  onOpenSettings,
}: GlobalSearchPageProps) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<GlobalSearchResultDto | null>(null);
  const [searching, setSearching] = useState(false);
  const [notice, setNotice] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (active) inputRef.current?.focus();
  }, [active]);

  const quickActions: QuickActionItem[] = useMemo(
    () => [
      {
        id: "new-note",
        title: "新建 Markdown 笔记",
        description: "快速创建新笔记文档进入编辑",
        shortcut: "⌘2",
        icon: NotePencil,
        keywords: ["note", "markdown", "write", "新建", "笔记", "写字"],
        run: () => {
          if (onNewNote) onNewNote();
          else onNavigate("markdown", {});
        },
      },
      {
        id: "ask-ai",
        title: "唤起 AI 智能助手",
        description: "打开全局 AI Copilot 侧边栏进行问答或指令下发",
        shortcut: "⌘/",
        icon: Sparkle,
        keywords: ["ai", "ask", "copilot", "chat", "问", "助手", "对话"],
        run: () => {
          if (onAskAi) onAskAi();
        },
      },
      {
        id: "refresh-rss",
        title: "刷新 RSS 订阅源",
        description: "立即检查并抓取所有已订阅 RSS 的最新文章",
        shortcut: "⌘3",
        icon: Rss,
        keywords: ["rss", "feed", "refresh", "news", "订阅", "刷新", "资讯"],
        run: () => {
          if (onRefreshRss) onRefreshRss();
          onNavigate("rss", {});
        },
      },
      {
        id: "open-geography",
        title: "探索世界地理 (Geography)",
        description: "浏览 3D 地形、国家统计与实体脉络",
        shortcut: "⌘6",
        icon: MapPin,
        keywords: ["geo", "map", "geography", "地理", "地图", "国家"],
        run: () => onNavigate("geography", {}),
      },
      {
        id: "open-history",
        title: "历史长河 (History)",
        description: "查看历史时期事件与语义历史脉络",
        shortcut: "⌘7",
        icon: Scroll,
        keywords: ["history", "timeline", "历史", "时间线", "事件"],
        run: () => onNavigate("history", {}),
      },
      {
        id: "open-language",
        title: "语言学习 (Language Practice)",
        description: "开启今日词汇复习与间隔重复闪卡练习",
        shortcut: "⌘8",
        icon: Translate,
        keywords: ["lang", "language", "words", "flashcard", "语言", "单词", "背词"],
        run: () => onNavigate("language", {}),
      },
      {
        id: "open-knowledge",
        title: "本地记忆与知识图谱 (Knowledge)",
        description: "检索记忆实体、索引文档与文件关联",
        shortcut: "⌘9",
        icon: Brain,
        keywords: ["knowledge", "memory", "doc", "知识", "记忆", "文档"],
        run: () => onNavigate("knowledge", {}),
      },
      {
        id: "open-travel",
        title: "旅行规划 (Travel Planner)",
        description: "自然语言行程规划与可编辑地图路线",
        shortcut: "⌘5",
        icon: Compass,
        keywords: ["travel", "trip", "plan", "旅行", "行程", "攻略"],
        run: () => onNavigate("travel", {}),
      },
      {
        id: "open-settings",
        title: "系统设置 (Settings)",
        description: "调整界面主题、AI 模型凭据与存储路径",
        shortcut: "⌘,",
        icon: Gear,
        keywords: ["setting", "config", "theme", "设置", "主题", "配置"],
        run: () => {
          if (onOpenSettings) onOpenSettings();
        },
      },
      {
        id: "open-system",
        title: "系统就绪状态 (System Readiness)",
        description: "查看后端服务自检、MCP 连接与端口状态",
        icon: Heartbeat,
        keywords: ["system", "health", "ready", "系统", "健康", "就绪", "mcp"],
        run: () => onNavigate("system", {}),
      },
    ],
    [onNewNote, onAskAi, onRefreshRss, onOpenSettings, onNavigate],
  );

  const filteredQuickActions = useMemo(() => {
    const raw = query.trim().toLowerCase();
    if (!raw) return quickActions;
    const clean = raw.startsWith(">") ? raw.slice(1).trim() : raw;
    if (!clean) return quickActions;
    return quickActions.filter(
      (action) =>
        action.title.toLowerCase().includes(clean) ||
        action.description.toLowerCase().includes(clean) ||
        action.keywords.some((kw) => kw.includes(clean)),
    );
  }, [query, quickActions]);

  const run = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) {
      setResult(null);
      setNotice("");
      return;
    }
    // 如果是命令模式 (> 开头)，不发起后端 search，由前端 action 处理
    if (trimmed.startsWith(">")) {
      setResult(null);
      return;
    }
    setSearching(true);
    setNotice("");
    try {
      // 真实后端命令（V11-O：零 LLM，AI 未配置也能搜）。
      const response = await invoke<GlobalSearchResultDto>("global_search", {
        query: trimmed,
        limit: 8,
      });
      setResult(response);
    } catch (error) {
      setResult(null);
      setNotice(`搜索失败：${errorMessage(error)}`);
    } finally {
      setSearching(false);
    }
  }, []);

  return (
    <div className="page-scroll search-page page-shell">
      <header className="page-shell-head">
        <div className="page-shell-title">
          <span className="page-shell-eyebrow">search</span>
          <h1>Global Search &amp; Actions</h1>
          <p className="page-shell-desc">
            输入关键词检索全模块内容，或直接点击下方快捷动作以极速启动任务。
          </p>
        </div>
        <div className="page-shell-actions">
          <span className="shortcut-chip">⌘K</span>
        </div>
      </header>

      <div className="search-box">
        <MagnifyingGlass size={18} />
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => {
            const val = event.target.value;
            setQuery(val);
            if (!val.trim()) {
              setResult(null);
              setNotice("");
            }
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              if (query.trim().startsWith(">") && filteredQuickActions.length > 0) {
                filteredQuickActions[0].run();
              } else {
                void run(query);
              }
            }
          }}
          placeholder="搜索历史、旅行、地理、语言、记忆、文档，或输入 > 筛选命令…"
          aria-label="全局搜索与快捷命令"
        />
        <button
          type="button"
          onClick={() => void run(query)}
          disabled={searching || !query.trim()}
        >
          {searching ? "搜索中…" : "搜索"}
        </button>
      </div>

      {notice ? <div className="search-error-notice">{notice}</div> : null}

      {result ? (
        <section className="search-results" aria-label="搜索结果">
          <div className="search-meta">
            共 {result.total} 条命中
            {result.degraded_sources.length > 0
              ? ` · ${result.degraded_sources.length} 个源暂不可用`
              : ""}
          </div>
          {result.total === 0 ? (
            <div className="search-empty-box">
              <p className="search-empty">
                没有找到与 “{query}” 相关的内容。
              </p>
              <small>提示：本地知识库与文档索引将在后台自动增量构建。</small>
            </div>
          ) : (
            <ul className="search-hits-list">
              {result.hits.map((hit, index) => (
                <li key={`${hit.source}-${index}`}>
                  <button
                    type="button"
                    onClick={() =>
                      onNavigate(
                        String(hit.action_target.module ?? hit.source),
                        hit.action_target,
                      )
                    }
                  >
                    <span className="search-hit-source">
                      {SOURCE_LABELS[hit.source] ?? hit.source}
                    </span>
                    <span className="search-hit-title">{hit.title}</span>
                    <span className="search-hit-snippet">{hit.snippet}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : (
        <section className="search-quick-actions" aria-label="快捷动作与命令">
          <div className="search-meta">
            <span>快捷命令与导航</span>
            <small>支持通过 ⌘1~9 快捷键随时直达</small>
          </div>
          <div className="quick-action-grid">
            {filteredQuickActions.map((action) => {
              const IconComp = action.icon;
              return (
                <button
                  key={action.id}
                  type="button"
                  className="quick-action-card"
                  onClick={action.run}
                >
                  <div className="quick-action-icon">
                    <IconComp size={20} weight="duotone" />
                  </div>
                  <div className="quick-action-info">
                    <div className="quick-action-title-row">
                      <strong>{action.title}</strong>
                      {action.shortcut ? (
                        <span className="shortcut-chip">{action.shortcut}</span>
                      ) : null}
                    </div>
                    <p>{action.description}</p>
                  </div>
                </button>
              );
            })}
          </div>

          <div className="search-shortcuts-reference">
            <h4>全局快捷键速查</h4>
            <div className="shortcuts-legend">
              <div><kbd>⌘ / Ctrl</kbd> + <kbd>K</kbd> <span>全局搜索 / 命令</span></div>
              <div><kbd>⌘ / Ctrl</kbd> + <kbd>/</kbd> <span>展开 / 收起 AI 侧栏</span></div>
              <div><kbd>⌘ / Ctrl</kbd> + <kbd>,</kbd> <span>打开系统设置</span></div>
              <div><kbd>⌘ / Ctrl</kbd> + <kbd>1 ~ 9</kbd> <span>快速直达 1~9 核心模块</span></div>
              <div><kbd>Esc</kbd> <span>快速关闭弹层 / 侧边栏</span></div>
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
