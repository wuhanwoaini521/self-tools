/**
 * History 模块的前端命令客户端（Gate 3 Reference Implementation）。
 *
 * 每个方法对应一个真实 Tauri 命令（命令名 / 参数 / 返回类型与后端契约冻结一致），
 * page 只调用本 client，不直接接触 transport 或 `@tauri-apps/api/core`。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
  SemanticEventDetail,
  SemanticHome,
  SemanticPeriodDetail,
  SemanticPersonDetail,
  SemanticSearchGroup,
  SemanticStoryDetail,
  SemanticWorkDetail,
} from "./semanticTypes";

export interface HistoryClient {
  home(): Promise<SemanticHome>;
  periodDetail(periodId: string): Promise<SemanticPeriodDetail | null>;
  storyDetail(storyId: string): Promise<SemanticStoryDetail | null>;
  eventDetail(eventId: string): Promise<SemanticEventDetail | null>;
  personDetail(personId: string): Promise<SemanticPersonDetail | null>;
  workDetail(workId: string): Promise<SemanticWorkDetail | null>;
  search(query: string): Promise<SemanticSearchGroup[]>;
}

export function createHistoryClient(transport: CommandTransport = tauriTransport): HistoryClient {
  return {
    home: () => transport.invoke<SemanticHome>("history_semantic_home"),
    periodDetail: (periodId) =>
      transport.invoke<SemanticPeriodDetail | null>("history_semantic_period", { periodId }),
    storyDetail: (storyId) =>
      transport.invoke<SemanticStoryDetail | null>("history_semantic_story", { storyId }),
    eventDetail: (eventId) =>
      transport.invoke<SemanticEventDetail | null>("history_semantic_event", { eventId }),
    personDetail: (personId) =>
      transport.invoke<SemanticPersonDetail | null>("history_semantic_person", { personId }),
    workDetail: (workId) =>
      transport.invoke<SemanticWorkDetail | null>("history_semantic_work", { workId }),
    search: (query) =>
      transport.invoke<SemanticSearchGroup[]>("history_semantic_search", { query }),
  };
}

export const historyClient = createHistoryClient();