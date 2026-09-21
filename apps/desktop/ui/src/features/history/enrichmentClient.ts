/**
 * History Enrichment 前端客户端（V5 Gate 8）。
 *
 * 只读状态查询不触发生成；ensure/refresh 触发按需管线（后端单飞）。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
  EnrichmentSectionInfo,
  EnrichmentViewDto,
} from "../ai/aiTypes";

export interface EnrichmentClient {
  /** 各 section 状态（首次渲染只读；不触发搜索/生成）。 */
  state(
    entityType: string,
    entityId: string,
    locale: string,
  ): Promise<EnrichmentSectionInfo[]>;
  /** 单 section 视图（只读：payload/metadata/error；不触发搜索）。 */
  view(
    entityType: string,
    entityId: string,
    section: string,
    locale: string,
  ): Promise<EnrichmentViewDto>;
  /** 按需生成（已在生成 → GENERATING 状态返回）。 */
  ensure(
    entityType: string,
    entityId: string,
    section: string,
    locale: string,
  ): Promise<EnrichmentViewDto>;
  /** 手动重新整理（Reviewed 也允许 → 新 revision 候选）。 */
  refresh(
    entityType: string,
    entityId: string,
    section: string,
    locale: string,
  ): Promise<EnrichmentViewDto>;
  /** 人工审定（automatic refresh 之后跳过）。 */
  review(
    entityType: string,
    entityId: string,
    section: string,
    locale: string,
  ): Promise<void>;
}

export function createEnrichmentClient(
  transport: CommandTransport = tauriTransport,
): EnrichmentClient {
  return {
    state: (entityType, entityId, locale) =>
      transport.invoke<EnrichmentSectionInfo[]>("history_enrichment_state", {
        entityType,
        entityId,
        locale,
      }),
    view: (entityType, entityId, section, locale) =>
      transport.invoke<EnrichmentViewDto>("history_enrichment_view", {
        entityType,
        entityId,
        section,
        locale,
      }),
    ensure: (entityType, entityId, section, locale) =>
      transport.invoke<EnrichmentViewDto>("history_enrichment_ensure", {
        entityType,
        entityId,
        section,
        locale,
      }),
    refresh: (entityType, entityId, section, locale) =>
      transport.invoke<EnrichmentViewDto>("history_enrichment_refresh", {
        entityType,
        entityId,
        section,
        locale,
      }),
    review: (entityType, entityId, section, locale) =>
      transport.invoke<void>("history_enrichment_review", {
        entityType,
        entityId,
        section,
        locale,
      }),
  };
}

export const enrichmentClient = createEnrichmentClient();