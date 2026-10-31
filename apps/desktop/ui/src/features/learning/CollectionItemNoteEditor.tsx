import { Check, NotePencil, X } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import type { CollectionItem } from "../../types";
import { errorMessage } from "../../utils";
import { learningClient } from "./learningClient";

interface CollectionItemNoteEditorProps {
  item: CollectionItem;
  onSaved: (item: CollectionItem) => void;
}

export function CollectionItemNoteEditor({ item, onSaved }: CollectionItemNoteEditorProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(item.note ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(item.note ?? "");
  }, [item.id, item.note]);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const updated = await learningClient.updateCollectionItemNote(
        item.id,
        draft.trim() || null,
      );
      onSaved(updated);
      setDraft(updated.note ?? "");
      setEditing(false);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  };

  if (!editing) {
    return (
      <div className="collection-personal-note">
        {item.note ? (
          <p>{item.note}</p>
        ) : (
          <p className="collection-personal-note-empty">还没有个人笔记。</p>
        )}
        <button
          type="button"
          className="collection-note-edit-button"
          onClick={() => {
            setDraft(item.note ?? "");
            setError(null);
            setEditing(true);
          }}
          aria-label={`${item.note ? "编辑" : "添加"}“${item.title}”的个人笔记`}
        >
          <NotePencil size={14} /> {item.note ? "编辑笔记" : "添加笔记"}
        </button>
      </div>
    );
  }

  return (
    <div className="collection-note-editor">
      <label htmlFor={`collection-note-${item.id}`}>个人笔记</label>
      <textarea
        id={`collection-note-${item.id}`}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        maxLength={4000}
        rows={3}
        disabled={saving}
      />
      <div className="collection-note-editor-footer">
        <span>{draft.length}/4000</span>
        <div>
          <button type="button" onClick={() => setEditing(false)} disabled={saving}>
            <X size={14} /> 取消
          </button>
          <button type="button" onClick={() => void save()} disabled={saving}>
            <Check size={14} /> {saving ? "保存中…" : "保存笔记"}
          </button>
        </div>
      </div>
      {error ? <p className="collection-note-error" role="alert">{error}</p> : null}
    </div>
  );
}
