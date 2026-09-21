/**
 * Memory 确认卡（V6 §25）：AI 想记住一条信息时，由用户显式确认。
 *
 * 纯展示组件：确认 / 拒绝的真实命令调用由调用方（AI Panel）负责，
 * 卡片只负责把「记住什么、从哪来」摆清楚，避免静默写入。
 */
import { Brain, Check, X } from "@phosphor-icons/react";
import {
  MEMORY_CATEGORY_LABELS,
  MEMORY_SOURCE_LABELS,
  type ConfirmMemoryTarget,
} from "./knowledgeTypes";

export interface MemoryConfirmCardProps {
  target: ConfirmMemoryTarget;
  onConfirm: () => void;
  onDismiss: () => void;
  busy?: boolean;
}

export function MemoryConfirmCard({
  target,
  onConfirm,
  onDismiss,
  busy = false,
}: MemoryConfirmCardProps) {
  const category = MEMORY_CATEGORY_LABELS[target.category] ?? target.category;
  const source = target.source_type
    ? MEMORY_SOURCE_LABELS[target.source_type] ?? target.source_type
    : null;
  return (
    <section className="memory-confirm-card" aria-label="记忆确认">
      <header className="memory-confirm-head">
        <Brain size={15} weight="fill" />
        <strong>是否记住这条信息？</strong>
      </header>
      <div className="memory-confirm-meta">
        <span className="memory-badge">{category}</span>
        {source ? <span className="memory-badge muted">{source}</span> : null}
      </div>
      <p className="memory-confirm-content">{target.content}</p>
      <p className="memory-confirm-hint">
        确认后这条信息会成为「已生效」记忆，AI 之后可以引用它。
      </p>
      <div className="memory-confirm-actions">
        <button
          className="memory-confirm-accept"
          onClick={onConfirm}
          disabled={busy}
        >
          <Check size={14} />
          {busy ? "处理中…" : "记住"}
        </button>
        <button
          className="memory-confirm-dismiss"
          onClick={onDismiss}
          disabled={busy}
        >
          <X size={14} />
          不要
        </button>
      </div>
    </section>
  );
}
