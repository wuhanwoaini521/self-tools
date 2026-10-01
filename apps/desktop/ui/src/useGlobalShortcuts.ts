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

const PAGE_SHORTCUT_MAP: Record<string, ShortcutPageId> = {
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

      const target = event.target as HTMLElement | null;
      const isInputFocused =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable ||
          Boolean(target.closest(".cm-editor")));

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
