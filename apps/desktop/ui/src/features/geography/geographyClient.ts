/**
 * Geography 模块的前端命令客户端（Gate 3B 第二迁移案例）。
 *
 * 仅封装真实 Tauri 命令（geography_home / geography_search / geography_detail /
 * geography_toggle_favorite），参数与后端契约保持一致；`geography_map` 与
 * `geography_compare` 命令已于 Gate 4 删除（无任何前端消费者）。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type { GeoEntityDetail, GeoSearchGroup, GeographyHome } from "../../types";

export interface GeographyClient {
  home(cursor?: number): Promise<GeographyHome>;
  detail(id: string): Promise<GeoEntityDetail | null>;
  search(query: string, entityType: string | null, limit: number): Promise<GeoSearchGroup[]>;
  toggleFavorite(id: string): Promise<boolean>;
}

export function createGeographyClient(
  transport: CommandTransport = tauriTransport,
): GeographyClient {
  return {
    home: (cursor) => transport.invoke<GeographyHome>("geography_home", { cursor }),
    detail: (id) => transport.invoke<GeoEntityDetail | null>("geography_detail", { id }),
    search: (query, entityType, limit) =>
      transport.invoke<GeoSearchGroup[]>("geography_search", { query, entityType, limit }),
    toggleFavorite: (id) =>
      transport.invoke<boolean>("geography_toggle_favorite", { id }),
  };
}

export const geographyClient = createGeographyClient();