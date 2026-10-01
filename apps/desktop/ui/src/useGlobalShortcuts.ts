import { useEffect } from "react";

export type ShortcutPageId =
  | "home"
  | "markdown"
  | "rss"
  | "news"
  | "travel"
  | "geography"
  | "history"
  | "language"
  | "knowledge";

export interface GlobalShortcutsOptions {
  onToggleSearch: () => void;
  onToggleAi: () => void;
  onOpenSettings: () => void;
  onCloseModals: () => void;
  onSelectPage: (page: ShortcutPageId) => void;
}

/**
 * 数字快捷键 → 页面。注意这不是侧边栏的顺序：
 * 侧边栏里 Review / Graph / Collections 也有序号徽标，但它们**不在**此映射内。
 * 侧边栏必须按这张表渲染徽标，否则会出现「Review 显示 ⌘2、按 ⌘2 却去 Markdown」。
 */
export const PAGE_SHORTCUT_MAP: Record<string, ShortcutPageId> = {
  "1": "home",
  "2": "markdown",
  "3": "rss",
  "4": "news",
  "5": "travel",
  "6": "geography",
  "7": "history",
  "8": "language",
  "9": "knowledge",
};

/** 页面 → 数字快捷键；该页没有数字快捷键时返回 null（侧边栏不显示徽标）。 */
export function shortcutDigitForPage(page: string): string | null {
  for (const [digit, target] of Object.entries(PAGE_SHORTCUT_MAP)) {
    if (target === page) return digit;
  }
  return null;
}

/**
 * 全局快捷键监听:
 * - ⌘/Ctrl + K : 唤起全局搜索与命令面板
 * - ⌘/Ctrl + / : 切换 AI 助手展开/收起
 * - ⌘/Ctrl + , : 打开系统设置
 * - ⌘/Ctrl + 1..9 : 快速切换到指定核心模块 (Home..Knowledge)
 * - Escape : 关闭当前活动的弹窗 / 抽屉
 */
export function useGlobalShortcuts({
  onToggleSearch,
  onToggleAi,
  onOpenSettings,
  onCloseModals,
  onSelectPage,
}: GlobalShortcutsOptions): void {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Escape: 关闭弹窗或覆盖层
      if (event.key === "Escape") {
        onCloseModals();
        return;
      }

      const isModifier = event.metaKey || event.ctrlKey;
      if (!isModifier) return;

      const target = event.target;
      // event.target 不一定是元素（document / window 也会成为 target）。
      // 直接调用 target.closest 会在这种情况下抛 TypeError，
      // 导致本处理器后半段（⌘K / ⌘/ / ⌘, / ⌘1..9）全部失效。
      const element = target instanceof HTMLElement ? target : null;
      const isInputFocused = Boolean(
        element &&
          (element.tagName === "INPUT" ||
            element.tagName === "TEXTAREA" ||
            element.isContentEditable ||
            element.closest(".cm-editor")),
      );

      // ⌘/Ctrl + K: 唤起全局搜索
      if (event.key.toLowerCase() === "k") {
        event.preventDefault();
        onToggleSearch();
        return;
      }

      // ⌘/Ctrl + /: 切换 AI 面板
      if (event.key === "/") {
        event.preventDefault();
        onToggleAi();
        return;
      }

      // ⌘/Ctrl + ,: 打开设置
      if (event.key === ",") {
        event.preventDefault();
        onOpenSettings();
        return;
      }

      // ⌘/Ctrl + 1..9: 快速切换页面 (输入框聚焦时不拦截数字输入，避免冲突)
      if (!isInputFocused && PAGE_SHORTCUT_MAP[event.key]) {
        event.preventDefault();
        onSelectPage(PAGE_SHORTCUT_MAP[event.key]);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onToggleSearch, onToggleAi, onOpenSettings, onCloseModals, onSelectPage]);
}
