import type { Extension } from "@codemirror/state";

/**
 * 主题定义:一份 Theme = CSS 作用域(data-theme)+ CodeMirror 扩展 + 展示元数据。
 * 新增主题只需新建 Definition 并 registerTheme(),即可自动出现在界面风格选择器,
 * 无需修改任何业务 UI 或出现主题分支判断。
 */
export interface ThemeDefinition {
  /** 稳定 id:持久化到 settings.json 的 ui_theme 字段,勿改动已有 id */
  id: string;
  /** 选择器中展示的名称 */
  name: string;
  /** 选择器下方的一句话描述 */
  description: string;
  /** 明暗归属,用于同步原生控件(如 select 下拉)的 color-scheme */
  appearance: "light" | "dark";
  /** 写入 <html data-theme="...">,对应 styles.css 中的变量作用域 */
  dataTheme: string;
  /** CodeMirror 扩展;Default 沿用 oneDark,其余主题提供与配色匹配的编辑器主题 */
  editorTheme: Extension;
}

export const DEFAULT_THEME_ID = "default";

/** 主题注册表:切换 / 枚举 / 查找全部经由 registry,业务代码不做主题字符串分支 */
const registry = new Map<string, ThemeDefinition>();

export function registerTheme(theme: ThemeDefinition): void {
  registry.set(theme.id, theme);
}

export function allThemes(): ThemeDefinition[] {
  return [...registry.values()];
}

/** 未知 id(旧配置、手改配置)安全回退到 Default,保证应用始终有可用主题 */
export function getTheme(id: string | null | undefined): ThemeDefinition {
  if (id && registry.has(id)) return registry.get(id)!;
  return registry.get(DEFAULT_THEME_ID)!;
}

/**
 * 应用主题到文档根节点:
 * - data-theme 驱动 styles.css 中的全部 CSS 变量作用域;
 * - color-scheme 让原生控件(select / scrollbar)跟随主题明暗。
 * CSS 变量作用于 :root,所有已打开视图(含弹层、菜单、编辑器)即时生效,无需重启。
 */
export function applyTheme(id: string): ThemeDefinition {
  const theme = getTheme(id);
  document.documentElement.dataset.theme = theme.dataTheme;
  document.documentElement.style.colorScheme = theme.appearance;
  return theme;
}

/** 主题 id 快照(localStorage):供启动首帧同步恢复,避免闪烁;Tauri 设置为持久化正源 */
const THEME_STORAGE_KEY = "devtoolbox.ui-theme";

export function storeThemeId(id: string): void {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, getTheme(id).id);
  } catch {
    // 隐私模式等 localStorage 不可用场景:静默跳过,主题仍在会话内生效
  }
}

export function readStoredThemeId(): string | null {
  try {
    return localStorage.getItem(THEME_STORAGE_KEY);
  } catch {
    return null;
  }
}

/** 启动时同步调用的初始主题:优先 localStorage 快照,否则 Default(与 Rust 侧默认一致) */
export function initialThemeId(): string {
  return getTheme(readStoredThemeId()).id;
}
