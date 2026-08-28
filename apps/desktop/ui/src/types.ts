export type ThemeMode = "light" | "dark" | "system";
export type MarkdownView = "editor" | "split" | "preview";

export interface AppSettings {
  schema_version: number;
  recent_files: string[];
  workspace_path: string | null;
  theme_mode: ThemeMode;
  /** UI 风格主题 id,由前端 ThemeManager 注册表校验;未知值回退 Default */
  ui_theme: string;
  /** RSS 自动刷新间隔(分钟) */
  rss_refresh_minutes: number;
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

/** Rust FeedDto(应用层 RSS DTO,snake_case 与设置保持一致) */
export interface FeedDto {
  id: number;
  title: string;
  url: string;
  site_url: string | null;
  unread_count: number;
  last_updated: number | null;
  last_error: string | null;
}

export interface ArticleDto {
  id: number;
  feed_id: number;
  feed_title: string;
  title: string;
  url: string;
  published_at: number | null;
  summary: string | null;
  is_read: boolean;
}

export interface RefreshReport {
  new_articles: number;
  failures: { feed_title: string; message: string }[];
}

export interface CommandFailure {
  code: string;
  message: string;
}
