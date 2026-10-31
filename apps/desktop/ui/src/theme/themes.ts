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
 * 默认主题 = Sunrise Glass。
 *
 * 全站几何语言（玻璃表层、清晰描边、克制动效）由 pencil.css + liquid-glass.css
 * 统一提供，日出光场与重点表层由 sunrise-glass.css 提供；主题只负责颜色。
 * 保留 id="default" 以兼容历史 settings.json，data-theme 仍指向
 * `:root[data-theme="pencil"]` 调色板（主题 ID 与选择行为不变）。
 */
const defaultTheme: ThemeDefinition = {
  id: DEFAULT_THEME_ID,
  name: "Sunrise Glass",
  description: "天空蓝到暖杏的日出光场、通透玻璃表层与清晰石墨文字，明亮但不刺眼。",
  appearance: "light",
  dataTheme: "pencil",
  editorTheme: pixelLightEditor,
  previewColors: ["#f1f3fc", "#ffffff", "#5566e6"],
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
