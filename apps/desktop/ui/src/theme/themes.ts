import { EditorView } from "@codemirror/view";
import { DEFAULT_THEME_ID, registerTheme, type ThemeDefinition } from "./ThemeManager";

/**
 * 内置主题注册表。
 *
 * Pixel Light:默认浅色工作区,以暖白纸面和硬边阴影表现轻量像素风格。
 *
 * Warm Editorial / Warm Editorial Dark:暖纸编辑部风格,
 * 配色通过同名 data-theme 作用域下的 Design Tokens 覆盖,不散落硬编码。
 */

/** 编辑器主题由 CSS token 统一驱动,扩展只需声明明暗 */
const pixelLightEditor = EditorView.theme({}, { dark: false });
const warmEditorialLightEditor = EditorView.theme({}, { dark: false });
const warmEditorialDarkEditor = EditorView.theme({}, { dark: true });

const defaultTheme: ThemeDefinition = {
  id: DEFAULT_THEME_ID,
  name: "Pixel Light",
  description: "浅色工作区搭配硬边阴影、阶梯动效与细小像素点阵。",
  appearance: "light",
  dataTheme: "pixel-light",
  editorTheme: pixelLightEditor,
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
