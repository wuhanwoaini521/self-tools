export type ThemeMode = "light" | "dark" | "system";
export type MarkdownView = "editor" | "split" | "preview";

export interface AppSettings {
  schema_version: number;
  recent_files: string[];
  workspace_path: string | null;
  theme_mode: ThemeMode;
  /** UI 风格主题 id,由前端 ThemeManager 注册表校验;未知值回退 Default */
  ui_theme: string;
  editor_font_size: number;
  auto_save: boolean;
  markdown_default_view: MarkdownView;
}

export interface DocumentDto {
  path: string;
  text: string;
}

export interface WorkspaceFile {
  path: string;
  relative_path: string;
}

export interface CommandFailure {
  code: string;
  message: string;
}
