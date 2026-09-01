import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { HistoryDetailView, HistoryNode } from "../../../types";
import { isTauriRuntime } from "../../../utils";
import type { HistoryEra } from "../historyEras";
import { KIND_LABELS } from "../labels";
import { HistoryRelationGraph } from "../components/HistoryRelationGraph";

/**
 * Graph View（关系）：选择一个时代节点，查看它的关系图。
 * 数据来自 history_detail 的真实 relations；跨时代人物关系网络将在后续版本加入。
 */
export function GraphView({
  era,
  nodes,
  onOpenNode,
}: {
  era: HistoryEra;
  nodes: HistoryNode[];
  onOpenNode: (node: HistoryNode) => void;
}) {
  const [selected, setSelected] = useState<HistoryNode | null>(null);
  const [view, setView] = useState<HistoryDetailView | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setSelected((current) =>
      current && nodes.some((node) => node.id === current.id)
        ? current
        : (nodes[0] ?? null),
    );
  }, [nodes]);

  useEffect(() => {
    if (!selected) {
      setView(null);
      return;
    }
    if (!isTauriRuntime()) {
      setView(null);
      return;
    }
    setLoading(true);
    let cancelled = false;
    void (async () => {
      try {
        const value = await invoke<HistoryDetailView | null>("history_detail", {
          id: selected.id,
        });
        if (!cancelled && value) setView(value);
      } catch (error) {
        // 次级视图加载失败不打断主流程；写入控制台便于排查。
        console.warn("history graph detail failed", error);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selected]);

  return (
    <section className="history-graph-view">
      <header className="history-section-head">
        <div>
          <span>GRAPH</span>
          <h2>{era.name} · 关系图</h2>
        </div>
        <p>选择节点查看其关系脉络 · 点击图中的节点继续深入</p>
      </header>

      <div
        className="history-graph-picker"
        role="listbox"
        aria-label="选取要查看关系的节点"
      >
        {nodes.map((node) => (
          <button
            type="button"
            key={node.id}
            className={selected?.id === node.id ? "is-active" : ""}
            onClick={() => setSelected(node)}
          >
            <span>{KIND_LABELS[node.kind]}</span>
            <b>{node.title}</b>
          </button>
        ))}
      </div>

      {loading ? (
        <p className="history-view-empty">正在加载关系数据…</p>
      ) : selected && view ? (
        <HistoryRelationGraph
          center={view.document.node}
          relations={view.relations}
          onOpenNode={onOpenNode}
        />
      ) : (
        <p className="history-view-empty">
          当前时代暂无已收录节点，或节点关系尚未建立。
        </p>
      )}
    </section>
  );
}
