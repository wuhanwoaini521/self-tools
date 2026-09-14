/**
 * Travel 模块的前端命令客户端（Gate 6）。
 *
 * 每个方法对应一个真实 Tauri 命令（命令名 / 参数 / 返回类型与后端契约冻结一致）。
 * 只封装命令调用 —— Travel 后端（FAT `travel_research_start`、会话注册表等）属
 * Gate 7 Application Boundary 范围，本 Client 不改变任何后端行为。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
  CityGuide,
  GuideSummary,
  TravelDateRange,
  TravelResearchRequest,
  TravelResearchSnapshot,
} from "../../types";

export interface TravelClient {
  recentGuides(): Promise<GuideSummary[]>;
  researchStart(request: TravelResearchRequest): Promise<string>;
  researchProgress(sessionId: string): Promise<TravelResearchSnapshot | null>;
  loadGuide(
    city: string,
    days: number,
    dateRange: TravelDateRange | null,
  ): Promise<CityGuide | null>;
  testLlm(request: {
    baseUrl: string;
    apiKey: string | null;
    model: string;
  }): Promise<string>;
  testAmap(request: {
    apiKey: string | null;
    apiHost: string | null;
  }): Promise<string>;
  testQweather(request: {
    apiKey: string | null;
    apiHost: string | null;
  }): Promise<string>;
}

export function createTravelClient(
  transport: CommandTransport = tauriTransport,
): TravelClient {
  return {
    recentGuides: () =>
      transport.invoke<GuideSummary[]>("travel_recent_guides"),
    researchStart: (request) =>
      transport.invoke<string>("travel_research_start", { request }),
    researchProgress: (sessionId) =>
      transport.invoke<TravelResearchSnapshot | null>(
        "travel_research_progress",
        { sessionId },
      ),
    loadGuide: (city, days, dateRange) =>
      transport.invoke<CityGuide | null>("travel_load_guide", {
        city,
        days,
        dateRange,
      }),
    testLlm: (request) =>
      transport.invoke<string>("test_travel_llm", { request }),
    testAmap: (request) =>
      transport.invoke<string>("test_travel_amap", { request }),
    testQweather: (request) =>
      transport.invoke<string>("test_travel_qweather", { request }),
  };
}

export const travelClient = createTravelClient();