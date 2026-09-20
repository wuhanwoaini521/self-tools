/**
 * Visual QA 专用 Vite 插件（不进生产）：当 `VITE_QA_BRIDGE=1` 时，
 * 在 index.html 注入一个 `window.__TAURI_INTERNALS__` 桥，把 History 模块的
 * Tauri 命令映射到经 Vite 代理的本地只读 HTTP 服务（apps/server，真实 DuckDB 数据）。
 *
 * 仅用于截图 / 无桌面壳验收：HistoryPage 检测 `__TAURI_INTERNALS__` 以启用
 * 数据访问；其它命令（geography/travel）不在桥内，页面会显示各自空态。
 */
import type { Plugin } from "vite";

const ENDPOINT_TEMPLATES: Record<string, string> = {
  history_semantic_home: "/__qa/api/v1/history/home",
  history_semantic_period: "/__qa/api/v1/history/periods/${periodId}",
  history_semantic_story: "/__qa/api/v1/history/stories/${storyId}",
  history_semantic_event: "/__qa/api/v1/history/events/${eventId}",
  history_semantic_person: "/__qa/api/v1/history/people/${personId}",
  history_semantic_work: "/__qa/api/v1/history/works/${workId}",
  history_semantic_search: "/__qa/api/v1/history/search?q=${query}",
};

const BRIDGE_SOURCE = `window.__TAURI_INTERNALS__ = {
  invoke: async (cmd, args) => {
    const templates = ${JSON.stringify(ENDPOINT_TEMPLATES)};
    const template = templates[cmd];
    if (!template) throw new Error("qa-bridge: unsupported command " + cmd);
    const render = (source, params) => {
      let out = "";
      let cursor = 0;
      for (;;) {
        const open = source.indexOf("\${", cursor);
        if (open === -1) { out += source.slice(cursor); break; }
        out += source.slice(cursor, open);
        const close = source.indexOf("}", open + 2);
        const key = source.slice(open + 2, close);
        out += encodeURIComponent(String((params ?? {})[key] ?? ""));
        cursor = close + 1;
      }
      return out;
    };
    const response = await fetch(render(template, args));
    if (!response.ok) return null;
    return response.json();
  },
};`;

/** 仅开发模式且显式开启时注入 QA 桥；生产构建绝不携带。 */
export function qaBridge(): Plugin {
  const enabled = process.env.VITE_QA_BRIDGE === "1";
  return {
    name: "visual-qa-bridge",
    apply: "serve",
    enforce: "pre",
    transformIndexHtml() {
      if (!enabled) return undefined;
      return [
        {
          tag: "script",
          attrs: { type: "text/javascript" },
          children: BRIDGE_SOURCE,
        },
      ];
    },
  };
}
