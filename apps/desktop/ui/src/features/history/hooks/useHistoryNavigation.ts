import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import type { HistoryDetailView, HistoryNode } from "../../../types";
import { errorMessage, isTauriRuntime } from "../../../utils";
import type { HistoryEra } from "../historyEras";
import type { HistoryGeoNavigationRequest } from "../types/history";

export interface HistoryNavigation {
  detail: HistoryDetailView | null;
  loading: boolean;
  periodNodes: HistoryNode[];
  periodLoading: boolean;
  openDetail: (id: string) => Promise<void>;
  openEraDetail: (era: HistoryEra) => Promise<void>;
  navigateToNode: (node: HistoryNode) => Promise<void>;
  closeDetail: () => void;
  toggleFavorite: () => Promise<void>;
  loadPeriodNodes: (periodId: string) => Promise<void>;
  navigateToGeography: (request: HistoryGeoNavigationRequest) => void;
}

export function useHistoryNavigation({
  favoriteIds,
  setNotice,
  onFavoritesChange,
  onNavigateToGeography,
}: {
  favoriteIds: string[];
  setNotice: (message: string) => void;
  onFavoritesChange: (next: string[]) => void;
  onNavigateToGeography?: (request: HistoryGeoNavigationRequest) => void;
}): HistoryNavigation {
  const [detail, setDetail] = useState<HistoryDetailView | null>(null);
  const [loading, setLoading] = useState(false);
  const [periodNodes, setPeriodNodes] = useState<HistoryNode[]>([]);
  const [periodLoading, setPeriodLoading] = useState(false);

  const openDetail = useCallback(
    async (id: string) => {
      if (!isTauriRuntime()) return;
      setLoading(true);
      try {
        const value = await invoke<HistoryDetailView | null>("history_detail", {
          id,
        });
        if (value) setDetail(value);
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setLoading(false);
      }
    },
    [setNotice],
  );

  const openEraDetail = useCallback(
    async (era: HistoryEra) => {
      await openDetail(era.id);
    },
    [openDetail],
  );

  const navigateToNode = useCallback(
    async (node: HistoryNode) => {
      await openDetail(node.id);
    },
    [openDetail],
  );

  const closeDetail = useCallback(() => setDetail(null), []);

  const toggleFavorite = useCallback(async () => {
    if (!detail || !isTauriRuntime()) return;
    try {
      const saved = await invoke<boolean>("history_toggle_favorite", {
        id: detail.document.node.id,
      });
      const id = detail.document.node.id;
      onFavoritesChange(
        saved
          ? [...new Set([...favoriteIds, id])]
          : favoriteIds.filter((fav) => fav !== id),
      );
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [detail, favoriteIds, onFavoritesChange, setNotice]);

  const loadPeriodNodes = useCallback(
    async (periodId: string) => {
      if (!isTauriRuntime()) {
        setPeriodNodes([]);
        return;
      }
      setPeriodLoading(true);
      try {
        const nodes = await invoke<HistoryNode[]>("history_period_nodes", {
          periodId,
        });
        setPeriodNodes(nodes);
      } catch (error) {
        setNotice(errorMessage(error));
        setPeriodNodes([]);
      } finally {
        setPeriodLoading(false);
      }
    },
    [setNotice],
  );

  const navigateToGeography = useCallback(
    (request: HistoryGeoNavigationRequest) => {
      onNavigateToGeography?.(request);
    },
    [onNavigateToGeography],
  );

  return {
    detail,
    loading,
    periodNodes,
    periodLoading,
    openDetail,
    openEraDetail,
    navigateToNode,
    closeDetail,
    toggleFavorite,
    loadPeriodNodes,
    navigateToGeography,
  };
}
