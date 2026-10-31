import { describe, expect, it, vi } from "vitest";
import type { Collection, CollectionItem } from "../../types";
import {
  saveCollectionReference,
  type CollectionReferenceDraft,
} from "./SaveToCollection";

const draft: CollectionReferenceDraft = {
  module: "geography",
  entityType: "city",
  entityId: "luoyang",
  title: "洛阳",
  note: "  安史之乱与东都洛阳  ",
};

function collection(id: string, title: string): Collection {
  return {
    id,
    title,
    description: null,
    tags: [],
    item_count: 0,
    created_at: 1,
    updated_at: 1,
  };
}

function savedItem(): CollectionItem {
  return {
    id: "item-1",
    collection_id: "collection-1",
    module: "geography",
    entity_type: "city",
    entity_id: "luoyang",
    title: "洛阳",
    note: "安史之乱与东都洛阳",
    added_at: 2,
  };
}

describe("saveCollectionReference", () => {
  it("stores the note with the original stable Geography reference", async () => {
    const client = {
      createCollection: vi.fn(async () => collection("unused", "unused")),
      addCollectionItem: vi.fn(async () => savedItem()),
    };

    const result = await saveCollectionReference(
      draft,
      { collection: collection("collection-1", "唐代洛阳") },
      client,
    );

    expect(result).toMatchObject({ id: "collection-1", title: "唐代洛阳" });
    expect(client.createCollection).not.toHaveBeenCalled();
    expect(client.addCollectionItem).toHaveBeenCalledWith(
      "collection-1",
      "geography",
      "city",
      "luoyang",
      "洛阳",
      "安史之乱与东都洛阳",
    );
  });

  it("creates a collection before saving a History source reference", async () => {
    const client = {
      createCollection: vi.fn(async (title: string) => collection("new-1", title)),
      addCollectionItem: vi.fn(async () => savedItem()),
    };

    const result = await saveCollectionReference(
      {
        module: "history",
        entityType: "event",
        entityId: "event-anlu-luoyang",
        title: "安史之乱攻陷洛阳",
      },
      { newCollectionTitle: "  Geography 与 History  " },
      client,
    );

    expect(result).toMatchObject({ id: "new-1", title: "Geography 与 History" });
    expect(client.createCollection).toHaveBeenCalledWith("Geography 与 History");
    expect(client.addCollectionItem).toHaveBeenCalledWith(
      "new-1",
      "history",
      "event",
      "event-anlu-luoyang",
      "安史之乱攻陷洛阳",
      undefined,
    );
  });

  it("retains the created collection identity when item saving fails", async () => {
    const client = {
      createCollection: vi.fn(async (title: string) => collection("new-2", title)),
      addCollectionItem: vi.fn(async () => {
        throw new Error("temporarily unavailable");
      }),
    };

    await expect(
      saveCollectionReference(draft, { newCollectionTitle: "洛阳线索" }, client),
    ).rejects.toMatchObject({
      createdCollection: { id: "new-2", title: "洛阳线索" },
    });
  });

  it("rejects an unselected collection rather than reporting a save", async () => {
    const client = {
      createCollection: vi.fn(async () => collection("unused", "unused")),
      addCollectionItem: vi.fn(async () => savedItem()),
    };

    await expect(saveCollectionReference(draft, {}, client)).rejects.toThrow("请选择");
    expect(client.addCollectionItem).not.toHaveBeenCalled();
  });
});
