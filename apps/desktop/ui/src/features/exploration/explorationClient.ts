import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";
import type { GeoEntityDetail } from "../../types";
import type { SemanticEventDetail } from "../history/semanticTypes";

export interface GeoHistoryCrosswalk {
  id: string;
  geography_entity_id: string;
  history_place_id: string;
  relation_type?: string;
  status: "reviewed" | string;
  confidence_percent?: number | null;
  evidence_time_scope?: {
    start_year: number;
    end_year: number;
    precision: string;
  } | null;
  geography_source_ids: string[];
  history_source_ids: string[];
  provenance?: Array<{
    source_id: string;
    record_id: string;
    uri: string | null;
    note: string;
  }>;
}

export interface LocationHistoryContext {
  geography: GeoEntityDetail;
  /** v1 singular field retained for older API servers. */
  crosswalk: GeoHistoryCrosswalk | null;
  /** Additive v2 field; can contain several historical places for one city. */
  crosswalks?: GeoHistoryCrosswalk[];
  events: SemanticEventDetail[];
}

export interface ExplorationClient {
  locationHistory(geographyEntityId: string): Promise<LocationHistoryContext | null>;
  geographyForHistoryPlace(historyPlaceId: string): Promise<GeoEntityDetail | null>;
}

export function createExplorationClient(
  transport: CommandTransport = defaultTransport,
): ExplorationClient {
  return {
    locationHistory: (id) =>
      transport.invoke<LocationHistoryContext | null>("exploration_location_history", { id }),
    geographyForHistoryPlace: (placeId) =>
      transport.invoke<GeoEntityDetail | null>("exploration_geography_for_history_place", { placeId }),
  };
}

export const explorationClient = createExplorationClient();
