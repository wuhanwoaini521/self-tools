import { describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { createLanguageClient } from "./languageClient";
import type { CommandTransport } from "../../transport";

function createMockTransport(invoke: Mock): CommandTransport {
  return {
    invoke: (cmd, args) =>
      Promise.resolve(args !== undefined ? invoke(cmd, args) : invoke(cmd)),
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

/**
 * 守住 **客户端 → Tauri 命令的线上契约**：命令名与参数键的驼峰拼写。
 *
 * 旧 Language 客户端把参数包进 `request` 并发 `{ request: { itemId } }`，而 Rust 侧
 * 三个写命令用 `#[serde(rename_all = "snake_case")]` 的结构体要求 `item_id`。
 * Tauri 只对**顶层**参数做驼峰转换，嵌套结构体按 serde 反序列化 → 那三条命令
 * 在运行时必然失败，整个学习写入路径是死的。这类断言正是为此存在。
 */
describe("LanguageClient 线上契约", () => {
  it("学习相关命令使用顶层驼峰参数，不嵌套 request 结构体", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({});
    const client = createLanguageClient(createMockTransport(mockInvoke));

    await client.recordStudy("jmdict:1", "study");
    await client.addToReview("jmdict:1");
    await client.submitReview("card_1", "good", "车站");
    await client.saveLessonPosition("lesson-1", 2);

    expect(mockInvoke).toHaveBeenNthCalledWith(1, "language_record_study", {
      entityId: "jmdict:1",
      action: "study",
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(2, "language_add_to_review", {
      entityId: "jmdict:1",
    });
    // 复习评分的三个参数都是顶层驼峰：cardId / rating / userAnswer
    expect(mockInvoke).toHaveBeenNthCalledWith(3, "language_submit_review", {
      cardId: "card_1",
      rating: "good",
      userAnswer: "车站",
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(4, "language_save_lesson_position", {
      lessonId: "lesson-1",
      stepIndex: 2,
    });
  });

  it("reviewNext / reviewRate / today / setState / toggleFavorite 已不存在", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({});
    const client = createLanguageClient(createMockTransport(mockInvoke));

    // 这些方法在语言侧的自建复习队列被删除后不应再出现。
    const removed = [
      "reviewNext",
      "reviewRate",
      "today",
      "setState",
      "toggleFavorite",
      "favorites",
    ] as const;
    for (const name of removed) {
      expect(name in client).toBe(false);
    }
  });

  it("createLesson 把 camelCase 请求包交给后端的 request 结构体", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({ id: "lesson-1" });
    const client = createLanguageClient(createMockTransport(mockInvoke));

    await client.createLesson({
      language: "jpn",
      title: "日本 · 交通基础",
      itemIds: ["jmdict:1", "jmdict:2"],
    });

    // `CreateLessonRequest` 是 camelCase 结构体，因此 request 内部必须也是驼峰。
    expect(mockInvoke).toHaveBeenCalledWith("language_create_lesson", {
      request: {
        language: "jpn",
        title: "日本 · 交通基础",
        itemIds: ["jmdict:1", "jmdict:2"],
      },
    });
  });

  it("studyQueue 把语言与数量作为顶层驼峰参数传入", async () => {
    const mockInvoke = vi.fn().mockResolvedValue([]);
    const client = createLanguageClient(createMockTransport(mockInvoke));

    await client.studyQueue("jpn", 20);

    expect(mockInvoke).toHaveBeenCalledWith("language_study_queue", {
      language: "jpn",
      limit: 20,
    });
  });

  it("查询类命令传递语言与数量", async () => {
    const mockInvoke = vi.fn().mockResolvedValue([]);
    const client = createLanguageClient(createMockTransport(mockInvoke));

    await client.search("jpn", "駅", 20);
    await client.sentences("jpn", 8);
    await client.reviewQueue(30);
    await client.mistakes(50);
    await client.progress(100);
    await client.weakItems(8);
    await client.continueLessons(5);
    await client.lesson("lesson-1");
    await client.lessons("jpn", 20);
    await client.sentenceStudy("tatoeba:1");
    await client.learningItem("jmdict:1");

    expect(mockInvoke).toHaveBeenNthCalledWith(1, "language_search", {
      language: "jpn",
      query: "駅",
      limit: 20,
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(2, "language_sentences", {
      language: "jpn",
      limit: 8,
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(3, "language_review_queue", { limit: 30 });
    expect(mockInvoke).toHaveBeenNthCalledWith(4, "language_mistakes", { limit: 50 });
    expect(mockInvoke).toHaveBeenNthCalledWith(5, "language_progress", { limit: 100 });
    expect(mockInvoke).toHaveBeenNthCalledWith(6, "language_weak_items", { limit: 8 });
    expect(mockInvoke).toHaveBeenNthCalledWith(7, "language_continue_lessons", { limit: 5 });
    expect(mockInvoke).toHaveBeenNthCalledWith(8, "language_lesson", { lessonId: "lesson-1" });
    expect(mockInvoke).toHaveBeenNthCalledWith(9, "language_lessons", {
      language: "jpn",
      limit: 20,
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(10, "language_sentence_study", {
      sentenceId: "tatoeba:1",
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(11, "language_learning_item", {
      entityId: "jmdict:1",
    });
  });

  it("addToCollection 缺省 note 时显式传 null", async () => {
    const mockInvoke = vi.fn().mockResolvedValue(null);
    const client = createLanguageClient(createMockTransport(mockInvoke));

    await client.addToCollection("col_1", "jmdict:1");
    expect(mockInvoke).toHaveBeenCalledWith("language_add_collection_item", {
      collectionId: "col_1",
      entityId: "jmdict:1",
      note: null,
    });
  });
});
