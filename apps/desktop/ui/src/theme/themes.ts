import { EditorView } from "@codemirror/view";
import { oneDark } from "@codemirror/theme-one-dark";
import { DEFAULT_THEME_ID, registerTheme, type ThemeDefinition } from "./ThemeManager";

/**
 * 内置主题注册表。
 *
 * Default:项目现有深色 UI,视觉零改动——CodeMirror 沿用 oneDark,
 * 配色全部来自 styles.css 中 [data-theme="default"] 作用域(即原 :root 值)。
 *
 * Warm Editorial / Warm Editorial Dark:暖纸编辑部风格,
 * 配色通过同名 data-theme 作用域下的 Design Tokens 覆盖,不散落硬编码。
 */

/** Warm 系编辑器:颜色由 styles.css 的 Token 统一驱动(与 Default 同机制),扩展只需声明明暗 */
const warmEditorialLightEditor = EditorView.theme({}, { dark: false });
const warmEditorialDarkEditor = EditorView.theme({}, { dark: true });

const defaultTheme: ThemeDefinition = {
  id: DEFAULT_THEME_ID,
  name: "Default",
  description: "项目默认的深色 Command Center 风格。",
  appearance: "dark",
  dataTheme: "default",
  editorTheme: oneDark,
};

const warmEditorial: ThemeDefinition = {
  id: "warm-editorial",
  name: "Warm Editorial",
  description: "暖白纸张背景、黑灰排版、暖棕点缀的极简编辑部风格。",
  appearance: "light",
  dataTheme: "warm-editorial",
  editorTheme: warmEditorialLightEditor,
};

const warmEditorialDark: ThemeDefinition = {
  id: "warm-editorial-dark",
  name: "Warm Editorial Dark",
  description: "Warm Editorial 的暖黑夜间版本,适合长时间写作。",
  appearance: "dark",
  dataTheme: "warm-editorial-dark",
  editorTheme: warmEditorialDarkEditor,
};

/** 注册即出现在界面风格选择器,未来主题(如 Nord / Paper)按同样方式追加 */
export const builtinThemes: ThemeDefinition[] = [defaultTheme, warmEditorial, warmEditorialDark];

for (const theme of builtinThemes) {
  registerTheme(theme);
}
