import type { HistoryNode, HistoryRelationView } from "../../../types";

/** History 首页顶层的 View Mode：时间 / 地图 / 人物 / 关系 / 故事。 */
export type HistoryViewMode = "timeline" | "map" | "people" | "graph" | "story";

/** 每个时代的视觉身份（可选，向后兼容：无 visual 的时代使用数据驱动的 fallback）。 */
export interface EraVisual {
  /** 视觉类型：map / illustration / artifact / route / pattern */
  type: "map" | "illustration" | "artifact" | "route" | "pattern";
  /** 可选图片资源路径（本页 assets） */
  asset?: string;
  /** 视觉焦点（如 “长安城”） */
  focalPoint?: string;
  /** 主视觉符号文字（如 “篆”“甲”“诗”） */
  motif?: string;
  /** 局部强调色 hex（可选，覆盖默认青铜色） */
  accent?: string;
}

/** 时代关键事件：驱动 EraTimeline「这个时代发生了什么？」。 */
export interface EraKeyEvent {
  year: number;
  title: string;
  note: string;
  /** 若在资料库中存在对应节点，点击打开其 Detail；否则仅展示。 */
  nodeId?: string;
}

/** 历史学习路线 / 故事。 */
export interface HistoryStoryNode {
  id: string;
  title: string;
  note: string;
  /** 对应资料库节点 id（可选）：存在则可点击进入 Detail。 */
  nodeId?: string;
}

export interface HistoryStory {
  id: string;
  title: string;
  description: string;
  /** 大致阅读时长标签（分钟） */
  minutes: number;
  periodId: string;
  nodes: HistoryStoryNode[];
}

/** 跨 Feature 的「在地图中查看」请求（低耦合 adapter，不硬编码 Geography 内部）。 */
export interface HistoryGeoNavigationRequest {
  mode: "history";
  entityType:
    | "place"
    | "war"
    | "route"
    | "territory"
    | "city"
    | "person";
  entityId: string;
  entityName: string;
  /** 可选经纬度，便于将来直接定位 */
  longitude?: number | null;
  latitude?: number | null;
}

/** 详情图（HistoryRelationGraph）需要的 relation 数据源。 */
export type GraphSource = HistoryRelationView[];

export type { HistoryNode };

/** 认识这个时代：节点分类顺序。 */
export const EXPLORE_KINDS = [
  "person",
  "event",
  "place",
  "war",
  "institution",
  "culture",
  "artifact",
] as const;
export type ExploreKind = (typeof EXPLORE_KINDS)[number];

export const EXPLORE_KIND_LABELS: Record<ExploreKind, string> = {
  person: "人物",
  event: "事件",
  place: "地点",
  war: "战争",
  institution: "制度",
  culture: "文化",
  artifact: "文物",
};
