import { useEffect } from "react";
import {
  PAGE_SHORTCUT_MAP,
  type ShortcutPageId,
} from "./shell/productNav";

export type { ShortcutPageId };
export { PAGE_SHORTCUT_MAP, shortcutDigitForPage } from "./shell/productNav";

export interface GlobalShortcutsOptions {
  onToggleSearch: () => void;
  onToggleAi: () => void;
  onOpenSettings: () => void;
  onCloseModals: () => void;
  onSelectPage: (page: ShortcutPageId) => void;
}

/**
 * 全局快捷键监听:
 * - ⌘/Ctrl + K : 唤起全局搜索与命令面板
 * - ⌘/Ctrl + / : 切换 AI 助手展开/收起
 * - ⌘/Ctrl + , : 打开系统设置
 * - ⌘/Ctrl + 1..4 : 首页 / 学习 / 旅行 / 资讯
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
      if (event.key === "Escape") {
        onCloseModals();
        return;
      }

      if (event.defaultPrevented) return;
      const isModifier = event.metaKey || event.ctrlKey;
      if (!isModifier) return;

      const target = event.target;
      const element = target instanceof HTMLElement ? target : null;
      const isInputFocused = Boolean(
        element &&
          (element.tagName === "INPUT" ||
            element.tagName === "TEXTAREA" ||
            element.isContentEditable ||
            element.closest(".cm-editor")),
      );

      if (event.key.toLowerCase() === "k") {
        event.preventDefault();
        onToggleSearch();
        return;
      }

      if (event.key === "/") {
        event.preventDefault();
        onToggleAi();
        return;
      }

      if (event.key === ",") {
        event.preventDefault();
        onOpenSettings();
        return;
      }

      if (!isInputFocused && PAGE_SHORTCUT_MAP[event.key]) {
        event.preventDefault();
        onSelectPage(PAGE_SHORTCUT_MAP[event.key]);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onToggleSearch, onToggleAi, onOpenSettings, onCloseModals, onSelectPage]);
}
