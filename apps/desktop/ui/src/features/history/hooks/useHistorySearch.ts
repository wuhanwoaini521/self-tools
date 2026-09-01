import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import type { HistoryNode, HistorySearchGroup } from "../../../types";
import { errorMessage, isTauriRuntime } from "../../../utils";

export interface HistorySearch {
  query: string;
  setQuery: (value: string) => void;
  groups: HistorySearchGroup[];
  searching: boolean;
  /** 是否已有搜索结果（决定搜索面板是否覆盖首页内容） */
  hasResults: boolean;
  /** 是否已真正执行过搜索（区分「输入未提交」与「提交后无结果」） */
  submitted: boolean;
  activeIndex: number;
  setActiveIndex: (index: number) => void;
  submit: () => Promise<void>;
  clear: () => void;
  flattenedItems: HistoryNode[];
}

/**
 * Command Search：输入 → 按类型分组的结果；支持上下键导航、回车打开。
 */
export function useHistorySearch({
  setNotice,
}: {
  setNotice: (message: string) => void;
}): HistorySearch {
  const [query, setQueryRaw] = useState("");
  const [groups, setGroups] = useState<HistorySearchGroup[]>([]);
  const [searching, setSearching] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [submitted, setSubmitted] = useState(false);

  const setQuery = useCallback((value: string) => {
    setQueryRaw(value);
    if (!value.trim()) {
      setGroups([]);
      setSubmitted(false);
    }
    setActiveIndex(-1);
  }, []);

  const submit = useCallback(async () => {
    const normalized = query.trim();
    if (!normalized || !isTauriRuntime()) {
      setGroups([]);
      setActiveIndex(-1);
      return;
    }
    setSearching(true);
    try {
      setGroups(
        await invoke<HistorySearchGroup[]>("history_search", {
          query: normalized,
        }),
      );
      setActiveIndex(-1);
      setSubmitted(true);
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setSearching(false);
    }
  }, [query, setNotice]);

  const clear = useCallback(() => {
    setGroups([]);
    setQueryRaw("");
    setActiveIndex(-1);
    setSubmitted(false);
  }, []);

  const flattenedItems: HistoryNode[] = groups.flatMap((group) => group.items);

  return {
    query,
    setQuery,
    groups,
    searching,
    hasResults: groups.length > 0,
    submitted,
    activeIndex,
    setActiveIndex,
    submit,
    clear,
    flattenedItems,
  };
}
