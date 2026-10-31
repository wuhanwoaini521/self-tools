import { BookmarkSimple, FolderPlus } from "@phosphor-icons/react";
import { useEffect, useId, useState, type FormEvent } from "react";
import type { Collection } from "../../types";
import { learningClient, type LearningClient } from "./learningClient";
import "./saveToCollection.css";

const CREATE_COLLECTION = "__create_collection__";

type CollectionSaveClient = Pick<
  LearningClient,
  "createCollection" | "addCollectionItem"
>;

export interface CollectionReferenceDraft {
  module: "geography" | "history";
  entityType: string;
  entityId: string;
  title: string;
  note?: string;
}

export class CollectionItemSaveError extends Error {
  constructor(readonly createdCollection: Collection) {
    super("The collection was created, but the item could not be saved.");
  }
}

export async function saveCollectionReference(
  draft: CollectionReferenceDraft,
  selection: {
    collection?: Collection | null;
    newCollectionTitle?: string;
  },
  client: CollectionSaveClient = learningClient,
): Promise<Collection> {
  const newTitle = selection.newCollectionTitle?.trim();
  let collection = selection.collection;
  let createdCollection: Collection | null = null;
  if (newTitle) {
    createdCollection = await client.createCollection(newTitle);
    collection = createdCollection;
  }
  if (!collection) throw new Error("请选择一个合集，或先输入新合集名称。");

  try {
    await client.addCollectionItem(
      collection.id,
      draft.module,
      draft.entityType,
      draft.entityId,
      draft.title,
      draft.note?.trim() || undefined,
    );
  } catch (cause) {
    if (createdCollection) throw new CollectionItemSaveError(createdCollection);
    throw cause;
  }
  return collection;
}

export function SaveToCollection({
  module,
  entityType,
  entityId,
  title,
}: CollectionReferenceDraft) {
  const formId = useId();
  const [open, setOpen] = useState(false);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [collectionId, setCollectionId] = useState(CREATE_COLLECTION);
  const [newCollectionTitle, setNewCollectionTitle] = useState("");
  const [note, setNote] = useState("");
  const [loadingCollections, setLoadingCollections] = useState(false);
  const [loadingError, setLoadingError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoadingCollections(true);
    setLoadingError(null);
    void learningClient.listCollections().then(
      (items) => {
        if (cancelled) return;
        setCollections(items);
        setCollectionId((current) =>
          current !== CREATE_COLLECTION && items.some((item) => item.id === current)
            ? current
            : items[0]?.id ?? CREATE_COLLECTION,
        );
      },
      () => {
        if (!cancelled) setLoadingError("暂时无法读取合集。检查服务连接后重试。");
      },
    ).finally(() => {
      if (!cancelled) setLoadingCollections(false);
    });
    return () => {
      cancelled = true;
    };
  }, [open]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setSavedMessage(null);
    setSaving(true);
    try {
      const selected = collections.find((item) => item.id === collectionId) ?? null;
      const savedTo = await saveCollectionReference(
        {
          module,
          entityType,
          entityId,
          title,
          note,
        },
        {
          collection: collectionId === CREATE_COLLECTION ? null : selected,
          newCollectionTitle:
            collectionId === CREATE_COLLECTION ? newCollectionTitle : undefined,
        },
      );
      setCollections((current) =>
        current.some((item) => item.id === savedTo.id)
          ? current
          : [savedTo, ...current],
      );
      setCollectionId(savedTo.id);
      setSavedMessage(`已保存到「${savedTo.title}」。`);
      setNote("");
    } catch (cause) {
      if (cause instanceof CollectionItemSaveError) {
        setCollections((current) => [
          cause.createdCollection,
          ...current.filter((item) => item.id !== cause.createdCollection.id),
        ]);
        setCollectionId(cause.createdCollection.id);
        setError("合集已创建，但条目未保存。已选中该合集，请重试保存。");
      } else {
        setError("保存失败。请检查服务连接后重试。");
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="collection-save">
      <button
        type="button"
        className="collection-save-trigger"
        aria-expanded={open}
        aria-controls={formId}
        onClick={() => {
          setOpen((current) => !current);
          setError(null);
          setSavedMessage(null);
        }}
      >
        <BookmarkSimple size={16} />
        保存到合集
      </button>
      {savedMessage ? <p className="collection-save-success" role="status">{savedMessage}</p> : null}
      {open ? (
        <form id={formId} className="collection-save-form" onSubmit={handleSubmit}>
          {loadingCollections ? <p role="status">正在读取合集…</p> : null}
          {loadingError ? (
            <div role="alert" className="collection-save-error">
              <p>{loadingError}</p>
              <button type="button" onClick={() => setOpen(false)}>关闭后重试</button>
            </div>
          ) : null}
          {!loadingCollections && !loadingError ? (
            <>
              <label>
                合集
                <select
                  value={collectionId}
                  onChange={(event) => setCollectionId(event.target.value)}
                >
                  {collections.map((collection) => (
                    <option key={collection.id} value={collection.id}>
                      {collection.title}
                    </option>
                  ))}
                  <option value={CREATE_COLLECTION}>新建合集…</option>
                </select>
              </label>
              {collectionId === CREATE_COLLECTION ? (
                <label>
                  新合集名称
                  <span className="collection-save-new-title">
                    <FolderPlus size={16} aria-hidden="true" />
                    <input
                      required
                      maxLength={100}
                      value={newCollectionTitle}
                      onChange={(event) => setNewCollectionTitle(event.target.value)}
                      placeholder="例如：洛阳与唐代历史"
                    />
                  </span>
                </label>
              ) : null}
              <label>
                个人笔记 <span className="collection-save-optional">可选</span>
                <textarea
                  rows={3}
                  maxLength={2000}
                  value={note}
                  onChange={(event) => setNote(event.target.value)}
                  placeholder="写下你想记住的线索或问题"
                />
              </label>
              <p className="collection-save-source">
                来源会保留为 {module === "geography" ? "Geography" : "History"} 稳定 ID：<code>{entityId}</code>
              </p>
              {error ? <p className="collection-save-error" role="alert">{error}</p> : null}
              <div className="collection-save-actions">
                <button type="button" onClick={() => setOpen(false)}>取消</button>
                <button type="submit" disabled={saving || loadingCollections}>
                  {saving ? "正在保存…" : "保存条目"}
                </button>
              </div>
            </>
          ) : null}
        </form>
      ) : null}
    </div>
  );
}
