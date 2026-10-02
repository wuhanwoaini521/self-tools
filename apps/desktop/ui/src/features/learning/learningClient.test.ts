import { describe, expect, it, vi } from "vitest";
import { createLearningClient } from "./learningClient";
import type { CommandTransport } from "../../transport";
import type { Mock } from "vitest";

function createMockTransport(invoke: Mock): CommandTransport {
  return {
    invoke: (cmd, args) =>
      Promise.resolve(args !== undefined ? invoke(cmd, args) : invoke(cmd)),
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

/**
 * 这些用例守住 **客户端 → Tauri 命令的线上契约**：命令名与参数键的驼峰拼写。
 *
 * 此前本文件断言的是 mock 自己返回的载荷（把编造的 `mastery_delta` / `card_id`
 * 回显回来），而真正的缺陷恰恰在**契约**上：`LearningEvent` 的 TS 形状与 Rust
 * 结构体没有一个字段对得上，`ReviewQueueItem` 被当成扁平卡片读 `.card_id`。
 * 那类断言无论契约怎么坏都会通过，因此换成对命令名与参数键的直接断言。
 */
describe("LearningClient 线上契约", () => {
  it("recordEvent 发送 learning_record_event 且不自行编造 id / timestamp", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({ entity_key: "language:word:jmdict:1" });
    const client = createLearningClient(createMockTransport(mockInvoke));

    await client.recordEvent({
      module: "language",
      entity_type: "word",
      entity_id: "jmdict:1",
      entity_title: "駅",
      action: "study",
    });

    const [command, args] = mockInvoke.mock.calls[0];
    expect(command).toBe("learning_record_event");
    // id / timestamp 缺省 → 由后端归一化，前端不得编造。
    expect(args.event).not.toHaveProperty("id");
    expect(args.event).not.toHaveProperty("created_at");
    expect(args.event.entity_title).toBe("駅");
  });

  it("submitReview 发送 learning_submit_review，参数键为驼峰 cardId", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({
      interval_days: 3,
      ease: 2.5,
      due_at: 1700000000,
      repetition_count: 1,
      lapses: 0,
      is_correct: true,
    });
    const client = createLearningClient(createMockTransport(mockInvoke));

    const outcome = await client.submitReview("card_123", "good");

    expect(mockInvoke).toHaveBeenCalledWith("learning_submit_review", {
      cardId: "card_123",
      rating: "good",
    });
    expect(outcome.repetition_count).toBe(1);
    expect(outcome.is_correct).toBe(true);
  });

  it("getReviewQueue 的入参键为 moduleFilter / limit", async () => {
    const mockInvoke = vi.fn().mockResolvedValue([]);
    const client = createLearningClient(createMockTransport(mockInvoke));

    await client.getReviewQueue("language", 5);

    expect(mockInvoke).toHaveBeenCalledWith("learning_get_review_queue", {
      moduleFilter: "language",
      limit: 5,
    });
  });

  it("getProgress / listProgress / getGraph 使用驼峰参数键", async () => {
    const mockInvoke = vi.fn().mockResolvedValue(null);
    const client = createLearningClient(createMockTransport(mockInvoke));

    await client.getProgress("language:word:x");
    await client.listProgress("language", "learning", 10);
    await client.getGraph("silk_road", 2);

    expect(mockInvoke).toHaveBeenNthCalledWith(1, "learning_get_progress", {
      entityKey: "language:word:x",
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(2, "learning_list_progress", {
      moduleFilter: "language",
      statusFilter: "learning",
      limit: 10,
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(3, "learning_get_graph", {
      rootId: "silk_road",
      hops: 2,
    });
  });

  it("合集 CRUD 使用驼峰参数键 collectionId", async () => {
    const mockInvoke = vi.fn().mockResolvedValue(null);
    const client = createLearningClient(createMockTransport(mockInvoke));

    await client.addCollectionItem("col_1", "language", "word", "jmdict:1", "駅");
    await client.removeCollectionItem("item_1");
    await client.deleteCollection("col_1");

    expect(mockInvoke).toHaveBeenNthCalledWith(1, "learning_add_collection_item", {
      collectionId: "col_1",
      module: "language",
      entityType: "word",
      entityId: "jmdict:1",
      title: "駅",
      note: undefined,
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(2, "learning_remove_collection_item", {
      itemId: "item_1",
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(3, "learning_delete_collection", {
      collectionId: "col_1",
    });
  });
});
