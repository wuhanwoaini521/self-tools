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

const pixelLightEditor = EditorView.theme({}, { dark: false });
const warmEditorialLightEditor = EditorView.theme({}, { dark: false });
const warmEditorialDarkEditor = EditorView.theme({}, { dark: true });
const nordDarkEditor = EditorView.theme({}, { dark: true });
const catppuccinDarkEditor = EditorView.theme({}, { dark: true });

/**
 * 默认主题 = Pencil Sketch。
 *
 * 全站的几何语言（手绘描边、纸张旋转、手写标题）由 pencil.css 统一提供，
 * 主题只负责颜色。保留 id="default" 以兼容历史 settings.json，
 * 但 data-theme 指向 pencil.css 中的 `:root[data-theme="pencil"]` 调色板。
 */
const defaultTheme: ThemeDefinition = {
  id: DEFAULT_THEME_ID,
  name: "Pencil Sketch",
  description: "暖白纸张、手绘描边与彩铅点缀——一本可以操作的知识笔记本。",
  appearance: "light",
  dataTheme: "pencil",
  editorTheme: pixelLightEditor,
  previewColors: ["#f2ebdc", "#fffdf7", "#9a6a2f"],
};

const warmEditorial: ThemeDefinition = {
  id: "warm-editorial",
  name: "Warm Editorial",
  description: "暖白纸张背景、黑灰排版、暖棕点缀的极简编辑部风格。",
  appearance: "light",
  dataTheme: "warm-editorial",
  editorTheme: warmEditorialLightEditor,
  previewColors: ["#f7f5ef", "#faf8f3", "#b97918"],
};

const warmEditorialDark: ThemeDefinition = {
  id: "warm-editorial-dark",
  name: "Warm Editorial Dark",
  description: "Warm Editorial 的暖黑夜间版本,适合长时间写作。",
  appearance: "dark",
  dataTheme: "warm-editorial-dark",
  editorTheme: warmEditorialDarkEditor,
  previewColors: ["#1d1c19", "#25231f", "#b97918"],
};

const nordTheme: ThemeDefinition = {
  id: "nord",
  name: "Nord",
  description: "极简北欧冷色调，以极光蓝绿与冰雪灰白构建冷静专注环境。",
  appearance: "dark",
  dataTheme: "nord",
  editorTheme: nordDarkEditor,
  previewColors: ["#2e3440", "#3b4252", "#88c0d0"],
};

const catppuccinTheme: ThemeDefinition = {
  id: "catppuccin-macchiato",
  name: "Catppuccin Macchiato",
  description: "现代舒适中对比度暗色主题，带有淡紫与柔和粉彩色调。",
  appearance: "dark",
  dataTheme: "catppuccin-macchiato",
  editorTheme: catppuccinDarkEditor,
  previewColors: ["#24273a", "#363a4f", "#c6a0f6"],
};

/** 注册即出现在界面风格选择器,未来主题(如 Nord / Paper)按同样方式追加 */
export const builtinThemes: ThemeDefinition[] = [
  defaultTheme,
  warmEditorial,
  warmEditorialDark,
  nordTheme,
  catppuccinTheme,
];

for (const theme of builtinThemes) {
  registerTheme(theme);
}
