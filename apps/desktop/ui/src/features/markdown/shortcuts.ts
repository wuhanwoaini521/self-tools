export type MarkdownShortcutId =
  | "save"
  | "commandPalette"
  | "toggleSidebar"
  | "toggleTaskOutline"
  | "toggleFocusMode"
  | "toggleZenMode"
  | "cycleTask";

export const MARKDOWN_SHORTCUTS: Record<MarkdownShortcutId, { windows: string; mac: string }> = {
  save: { windows: "Ctrl+S", mac: "⌘S" },
  commandPalette: { windows: "Ctrl+F", mac: "⌘F" },
  toggleSidebar: { windows: "Ctrl+B", mac: "⌘B" },
  toggleTaskOutline: { windows: "Ctrl+\\", mac: "⌘\\" },
  toggleFocusMode: { windows: "F11", mac: "F11" },
  toggleZenMode: { windows: "Ctrl+K, Z", mac: "⌘K, Z" },
  cycleTask: { windows: "Ctrl+Enter", mac: "⌘Enter" },
};

export function shortcutLabel(id: MarkdownShortcutId, platform = navigator.platform): string {
  return /Mac|iPhone|iPad|iPod/i.test(platform)
    ? MARKDOWN_SHORTCUTS[id].mac
    : MARKDOWN_SHORTCUTS[id].windows;
}

export function hasPrimaryModifier(event: Pick<KeyboardEvent, "ctrlKey" | "metaKey">): boolean {
  return event.ctrlKey || event.metaKey;
}

export function matchesMarkdownShortcut(
  id: MarkdownShortcutId,
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey"> & { code?: string },
): boolean {
  if (event.altKey || (event.shiftKey && id !== "toggleTaskOutline")) return false;
  const key = event.key.toLowerCase();
  const modified = hasPrimaryModifier(event);
  switch (id) {
    case "save": return modified && key === "s";
    case "commandPalette": return modified && key === "f";
    case "toggleSidebar": return modified && key === "b";
    // WebDriver and some keyboard layouts report the Backslash key as one or
    // two literal slashes. Accept the physical key token in either form.
    case "toggleTaskOutline": return modified && (event.key.includes("\\") || event.code === "Backslash");
    case "toggleFocusMode": return !modified && event.key === "F11";
    case "toggleZenMode": return modified && key === "k";
    case "cycleTask": return modified && event.key === "Enter";
  }
}
