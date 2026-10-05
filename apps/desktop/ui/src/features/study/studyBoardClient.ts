/**
 * 学习板（Study Board）的前端命令客户端。
 *
 * ## 为什么不再直接写 localStorage
 *
 * 画板此前把笔迹存在 `window.localStorage`：换浏览器、清缓存、换设备就没了，
 * 而且 AI 那边 `study-board.list` 看到的板和用户眼前这块**不是同一块**。
 * 现在两端共用同一份用例（`StudyBoardService`）与同一份 `config/study_boards.db`：
 * 桌面端走 Tauri IPC，网页端走 HTTP，函数签名与行为完全一致。
 */
import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";

/** 板元数据（列表用；不含笔迹正文）。 */
export interface StudyBoardSummary {
  id: string;
  title: string;
  module_origin: string;
  created_at: number;
  updated_at: number;
  stroke_count: number;
}

/** 板（含笔迹）。`strokes` 对后端不透明：原样存取，不解释。 */
export interface StudyBoardRecord {
  id: string;
  title: string;
  strokes: unknown;
  created_at: number;
  updated_at: number;
  module_origin: string;
}

export interface StudyBoardSnapshot {
  snapshot_id: string;
  board_id: string;
  title: string;
  /** 有界摘要（不含坐标）。 */
  strokes_summary: string;
  created_at: number;
  has_png: boolean;
}

export interface StudyBoardSaveResult {
  board: StudyBoardRecord;
  /** 本次是否新建（前端据此说「已创建」而不是「已覆盖」）。 */
  created: boolean;
}

export interface StudyBoardClient {
  list(limit?: number): Promise<StudyBoardSummary[]>;
  get(boardId: string): Promise<StudyBoardRecord | null>;
  save(input: {
    boardId: string;
    title?: string;
    strokes?: unknown;
    moduleOrigin?: string;
  }): Promise<StudyBoardSaveResult>;
  snapshot(boardId: string, pngBase64?: string | null): Promise<StudyBoardSnapshot>;
}

export function createStudyBoardClient(
  transport: CommandTransport = defaultTransport,
): StudyBoardClient {
  return {
    // 桌面端命令直接返回板；网页端返回 { board, created }，这里统一成同一形状。
    list: async (limit) => {
      const raw = await transport.invoke<StudyBoardSummary[] | { items: StudyBoardSummary[] }>(
        "study_board_list",
        { limit },
      );
      if (Array.isArray(raw)) return raw;
      return raw?.items ?? [];
    },
    get: (boardId) => transport.invoke<StudyBoardRecord | null>("study_board_get", { boardId }),
    save: async ({ boardId, title, strokes, moduleOrigin }) => {
      const raw = await transport.invoke<StudyBoardRecord | StudyBoardSaveResult>(
        "study_board_save",
        { boardId, title, strokes, moduleOrigin },
      );
      if (raw && typeof raw === "object" && "board" in raw) return raw as StudyBoardSaveResult;
      return { board: raw as StudyBoardRecord, created: false };
    },
    snapshot: (boardId, pngBase64) =>
      transport.invoke<StudyBoardSnapshot>("study_board_snapshot", { boardId, pngBase64 }),
  };
}

export const studyBoardClient = createStudyBoardClient();
