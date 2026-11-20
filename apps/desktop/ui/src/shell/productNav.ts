/**
 * 七项产品壳的导航契约（FRONTEND）。
 *
 * 分组只影响呈现：每个 PAGE_IDS 条目仍是独立 hash。旧深链必须继续能打开。
 */
import {
  BookOpen,
  Brain,
  Cards,
  Compass,
  FolderSimple,
  HardDrives,
  Heartbeat,
  House,
  MagnifyingGlass,
  MapTrifold,
  Newspaper,
  Notebook,
  Rss,
  Scroll,
  Translate,
  TreeStructure,
} from "@phosphor-icons/react";

export type PageId =
  | "home"
  | "learn"
  | "review"
  | "graph"
  | "collections"
  | "markdown"
  | "rss"
  | "news"
  | "travel"
  | "geography"
  | "history"
  | "language"
  | "knowledge"
  | "study-board"
  | "server"
  | "system"
  | "search";

export const PAGE_IDS: PageId[] = [
  "home",
  "learn",
  "review",
  "graph",
  "collections",
  "markdown",
  "rss",
  "news",
  "travel",
  "geography",
  "history",
  "language",
  "knowledge",
  "study-board",
  "server",
  "system",
  "search",
];

export function isPageId(value: string): value is PageId {
  return (PAGE_IDS as string[]).includes(value);
}

/** 学习区：hub + 四科 + 复习中心（复习仍走旧深链，不进一级导航）。 */
export const LEARN_PAGE_IDS: readonly PageId[] = [
  "learn",
  "history",
  "geography",
  "language",
  "study-board",
  "review",
];

export const NEWS_PAGE_IDS: readonly PageId[] = ["news", "rss"];

export type PrimaryNavId = "home" | "learn" | "travel" | "news";

export interface NavItem {
  id: PageId;
  label: string;
  icon: typeof House;
}

/** 顶栏 / 底栏四个一级入口。首页靠品牌进入，三项产品靠链接。 */
export const PRIMARY_NAV: NavItem[] = [
  { id: "home", label: "首页", icon: House },
  { id: "learn", label: "学习", icon: BookOpen },
  { id: "travel", label: "旅行", icon: Compass },
  { id: "news", label: "资讯", icon: Newspaper },
];

/** 手机底栏不含「更多」时的四项。 */
export const BOTTOM_NAV_IDS: PageId[] = ["home", "learn", "travel", "news"];

export const LEARN_SUBNAV: NavItem[] = [
  { id: "learn", label: "总览", icon: BookOpen },
  { id: "history", label: "历史", icon: Scroll },
  { id: "geography", label: "地理", icon: MapTrifold },
  { id: "language", label: "语言", icon: Translate },
  { id: "study-board", label: "画板", icon: Notebook },
];

export const NEWS_SUBNAV: NavItem[] = [
  { id: "news", label: "新闻", icon: Newspaper },
  { id: "rss", label: "RSS", icon: Rss },
];

/** 溢出入口：旧功能仍可通过 hash / 更多 打开。 */
export const OVERFLOW_NAV_GROUPS: Array<{ label: string; items: NavItem[] }> = [
  {
    label: "学习（更多）",
    items: [
      { id: "review", label: "复习中心", icon: Cards },
      { id: "collections", label: "收藏", icon: FolderSimple },
      { id: "graph", label: "知识图谱", icon: TreeStructure },
    ],
  },
  {
    label: "工具",
    items: [
      { id: "markdown", label: "Markdown", icon: Notebook },
      { id: "search", label: "搜索", icon: MagnifyingGlass },
      { id: "knowledge", label: "知识库", icon: Brain },
    ],
  },
  {
    label: "系统",
    items: [
      { id: "server", label: "服务器", icon: HardDrives },
      { id: "system", label: "系统", icon: Heartbeat },
    ],
  },
];

export type ShortcutPageId = PrimaryNavId;

/** ⌘/Ctrl+1..4 → 一级入口。旧的 5–9 不再抢主导航。 */
export const PAGE_SHORTCUT_MAP: Record<string, ShortcutPageId> = {
  "1": "home",
  "2": "learn",
  "3": "travel",
  "4": "news",
};

export function shortcutDigitForPage(page: string): string | null {
  for (const [digit, target] of Object.entries(PAGE_SHORTCUT_MAP)) {
    if (target === page) return digit;
  }
  return null;
}

export function isLearnSection(page: PageId): boolean {
  return LEARN_PAGE_IDS.includes(page);
}

export function isNewsSection(page: PageId): boolean {
  return NEWS_PAGE_IDS.includes(page);
}

/** 当前页对应的一级高亮；溢出页不高亮三项产品。 */
export function primaryNavIdForPage(page: PageId): PrimaryNavId | null {
  if (page === "home") return "home";
  if (isLearnSection(page)) return "learn";
  if (page === "travel") return "travel";
  if (isNewsSection(page)) return "news";
  return null;
}

/** 点一级入口：已在该区内则保持当前页，否则进入该区首页。 */
export function resolvePrimaryNavigation(current: PageId, target: PrimaryNavId): PageId {
  if (target === "learn" && isLearnSection(current)) return current;
  if (target === "news" && isNewsSection(current)) return current;
  return target;
}

export function parseHashPath(hash: string): PageId | null {
  const path = hash.replace(/^#/, "").trim().split("?")[0];
  if (!path) return null;
  return isPageId(path) ? path : null;
}
