import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  ArrowSquareOut,
  CheckCircle,
  FloppyDisk,
  PencilSimple,
  Trash,
  X,
} from "@phosphor-icons/react";
import type { CollectionItem, UniversalReviewCard } from "../../types";
import { learningClient } from "./learningClient";
import { sourceRoute } from "./sourceNavigation";
import "./knowledgeReviewCard.css";

interface KnowledgeReviewCardEditorProps {
  item: CollectionItem;
  onNavigate?: (route: string) => void;
}

export function knowledgeReviewCardId(item: Pick<CollectionItem, "module" | "entity_type" | "entity_id">): string {
  return `knowledge:${item.module}:${item.entity_type}:${item.entity_id}`;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : "学习卡操作失败，请检查连接后重试。";
}

export function KnowledgeReviewCardEditor({ item, onNavigate }: KnowledgeReviewCardEditorProps) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [card, setCard] = useState<UniversalReviewCard | null>(null);
  const [prompt, setPrompt] = useState("");
  const [answer, setAnswer] = useState("");
  const [userNote, setUserNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const errorRef = useRef<HTMLDivElement>(null);
  const route = sourceRoute(item.module, item.entity_type, item.entity_id);

  useEffect(() => {
    if (error) errorRef.current?.focus();
  }, [error]);

  const openEditor = async () => {
    setOpen(true);
    setError(null);
    setNotice(null);
    setLoading(true);
    try {
      const saved = await learningClient.getReviewCard(knowledgeReviewCardId(item));
      setCard(saved);
      setPrompt(saved?.prompt ?? "");
      setAnswer(saved?.answer ?? "");
      setUserNote(saved?.user_note ?? item.note ?? "");
    } catch (cause) {
      setError(`无法读取学习卡：${errorText(cause)}`);
    } finally {
      setLoading(false);
    }
  };

  const save = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (saving || !prompt.trim() || !answer.trim()) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await learningClient.saveKnowledgeReviewCard({
        module: item.module as "history" | "geography",
        entity_type: item.entity_type,
        entity_id: item.entity_id,
        source_title: item.title,
        prompt,
        answer,
        user_note: userNote.trim() || null,
      });
      setCard(saved);
      setPrompt(saved.prompt);
      setAnswer(saved.answer);
      setUserNote(saved.user_note ?? "");
      setNotice("学习卡已保存；复习排期和原始来源已保留。");
    } catch (cause) {
      setError(`没有保存学习卡：${errorText(cause)}。草稿仍在，可修正后重试。`);
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!card || deleting) return;
    if (!window.confirm("删除这张个人学习卡？原始知识条目和已有复习进度不会删除。")) return;
    setDeleting(true);
    setError(null);
    setNotice(null);
    try {
      await learningClient.deleteKnowledgeReviewCard(card.id);
      setCard(null);
      setPrompt("");
      setAnswer("");
      setUserNote(item.note ?? "");
      setOpen(false);
      setNotice("学习卡已删除；原始条目和既有复习记录保留。");
    } catch (cause) {
      setError(`没有删除学习卡：${errorText(cause)}。请重试。`);
    } finally {
      setDeleting(false);
    }
  };

  if (!route) return null;

  return (
    <div className="knowledge-review-editor">
      <div className="knowledge-review-actions">
        <button
          type="button"
          className="knowledge-review-trigger"
          onClick={() => void openEditor()}
          disabled={loading || saving || deleting}
        >
          <PencilSimple size={14} /> 管理复习卡
        </button>
        {card && !open ? (
          <span className="knowledge-review-saved"><CheckCircle size={14} /> 已加入 Review Center</span>
        ) : null}
      </div>

      {notice ? <p className="knowledge-review-notice" role="status">{notice}</p> : null}

      {open ? (
        <section className="knowledge-review-panel" aria-label="学习卡编辑器">
          <div className="knowledge-review-heading">
            <strong>{card ? "编辑个人学习卡" : "创建个人学习卡"}</strong>
            <button
              type="button"
              className="knowledge-review-close"
              aria-label="关闭学习卡编辑器"
              onClick={() => setOpen(false)}
              disabled={saving || deleting}
            >
              <X size={15} />
            </button>
          </div>
          <p className="knowledge-review-source">
            来源：{item.title} <span>· {item.module} / {item.entity_type} / {item.entity_id}</span>
          </p>
          <p className="knowledge-review-guidance">
            问题和参考答案由你编写，不会自动生成或标记为已核实。原合集备注会复制到个人备注，可单独修改。
          </p>

          {loading ? <p className="knowledge-review-loading" role="status">正在读取已保存的学习卡…</p> : null}
          {error ? (
            <div ref={errorRef} className="knowledge-review-error" role="alert" tabIndex={-1}>
              <strong>操作未完成</strong>
              <span>{error}</span>
            </div>
          ) : null}

          {!loading ? (
            <form className="knowledge-review-form" onSubmit={(event) => void save(event)}>
              <label>
                复习问题
                <textarea
                  value={prompt}
                  onChange={(event) => setPrompt(event.target.value)}
                  rows={2}
                  maxLength={2000}
                  required
                  disabled={saving || deleting}
                  placeholder="写一个需要自己回忆的问题"
                />
              </label>
              <label>
                参考答案
                <textarea
                  value={answer}
                  onChange={(event) => setAnswer(event.target.value)}
                  rows={3}
                  maxLength={10000}
                  required
                  disabled={saving || deleting}
                  placeholder="写下你认可的答案；复习时会显示为个人参考"
                />
              </label>
              <label>
                个人备注 <span className="knowledge-review-optional">（可选）</span>
                <textarea
                  value={userNote}
                  onChange={(event) => setUserNote(event.target.value)}
                  rows={2}
                  maxLength={4000}
                  disabled={saving || deleting}
                  placeholder="保留来源线索、自己的理解或待核对的问题"
                />
              </label>

              <div className="knowledge-review-form-actions">
                <button
                  type="submit"
                  className="knowledge-review-save"
                  disabled={saving || deleting || !prompt.trim() || !answer.trim()}
                >
                  <FloppyDisk size={15} /> {saving ? "正在保存…" : card ? "保存修改" : "保存到 Review Center"}
                </button>
                {card ? (
                  <button type="button" className="knowledge-review-delete" onClick={() => void remove()} disabled={saving || deleting}>
                    <Trash size={14} /> {deleting ? "正在删除…" : "删除个人卡"}
                  </button>
                ) : null}
                {card ? (
                  <button type="button" className="knowledge-review-open" onClick={() => onNavigate?.("#review")}>
                    去 Review Center <ArrowSquareOut size={14} />
                  </button>
                ) : null}
              </div>
            </form>
          ) : null}
          <button type="button" className="knowledge-review-source-link" onClick={() => onNavigate?.(route)}>
            <ArrowSquareOut size={14} /> 返回原始来源
          </button>
        </section>
      ) : null}
    </div>
  );
}
