import { describe, expect, it } from "vitest";
import {
  MARKDOWN_SHORTCUTS,
  matchesMarkdownShortcut,
  shortcutLabel,
} from "./shortcuts";

describe("Markdown 键盘快捷键", () => {
  it("集中定义 Windows 与 macOS 的提示文案", () => {
    expect(shortcutLabel("save", "Win32")).toBe("Ctrl+S");
    expect(shortcutLabel("save", "MacIntel")).toBe("⌘S");
    expect(shortcutLabel("toggleZenMode", "Win32")).toBe("Ctrl+K, Z");
    expect(shortcutLabel("toggleZenMode", "MacIntel")).toBe("⌘K, Z");
    expect(Object.keys(MARKDOWN_SHORTCUTS)).toHaveLength(7);
  });

  it.each([
    ["save", "s", true],
    ["commandPalette", "f", true],
    ["toggleSidebar", "b", true],
    ["toggleTaskOutline", "\\", true],
    ["toggleFocusMode", "F11", false],
    ["toggleZenMode", "k", true],
    ["cycleTask", "Enter", true],
  ] as const)("识别 %s", (id, key, modified) => {
    expect(matchesMarkdownShortcut(id, {
      key,
      ctrlKey: modified,
      metaKey: false,
      altKey: false,
      shiftKey: false,
    })).toBe(true);
  });

  it("拒绝仅有 Alt 或 Shift 的近似组合", () => {
    expect(matchesMarkdownShortcut("save", {
      key: "s", ctrlKey: false, metaKey: false, altKey: true, shiftKey: false,
    })).toBe(false);
    expect(matchesMarkdownShortcut("commandPalette", {
      key: "f", ctrlKey: true, metaKey: false, altKey: false, shiftKey: true,
    })).toBe(false);
  });

  it("按物理键码识别不同键盘布局上的反斜杠", () => {
    expect(matchesMarkdownShortcut("toggleTaskOutline", {
      key: "|", code: "Backslash", ctrlKey: true, metaKey: false, altKey: false, shiftKey: true,
    })).toBe(true);
  });
});
