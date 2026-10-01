import { describe, expect, it, vi } from "vitest";
import { createLearningClient } from "./learningClient";
import type { CommandTransport } from "../../transport";

function createMockTransport(invoke: (...args: any[]) => any): CommandTransport {
  return {
    invoke: (cmd, args) => Promise.resolve(args !== undefined ? invoke(cmd, args) : invoke(cmd)),
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

describe("LearningClient", () => {
  it("calls learning_get_today correctly", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({
      date_str: "2026-10-01",
      greeting: "你好，开启今天的知识探索",
      studied_topics_today: 5,
      pending_reviews_count: 2,
      average_mastery: 78,
      recent_streak_days: 3,
      continue_items: [],
      review_stats: {
        due_count: 2,
        total_cards: 10,
        by_module: { history: 1, language: 1 },
        mastered_count: 4,
        learning_count: 6,
      },
      explore_recommendations: [],
      recent_collections: [],
      recent_bookmarks: [],
    });
    const mockTransport = createMockTransport(mockInvoke);

    const client = createLearningClient(mockTransport);
    const today = await client.getToday();

    expect(mockInvoke).toHaveBeenCalledWith("learning_get_today");
    expect(today.recent_streak_days).toBe(3);
    expect(today.average_mastery).toBe(78);
    expect(today.review_stats.due_count).toBe(2);
  });

  it("calls learning_submit_review with correct payload", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({
      card_id: "card_123",
      new_state: "reviewing",
      interval_days: 3,
      ease_factor: 2.5,
      due_at: 1700000000,
      lapses: 0,
      mastery_delta: 12,
    });
    const mockTransport = createMockTransport(mockInvoke);

    const client = createLearningClient(mockTransport);
    const outcome = await client.submitReview("card_123", "good");

    expect(mockInvoke).toHaveBeenCalledWith("learning_submit_review", {
      cardId: "card_123",
      rating: "good",
    });
    expect(outcome.interval_days).toBe(3);
    expect(outcome.mastery_delta).toBe(12);
  });

  it("calls learning_get_graph with rootId and hops", async () => {
    const mockInvoke = vi.fn().mockResolvedValue({
      center: { id: "silk_road", name: "丝绸之路", entity_type: "topic", module: "history" },
      nodes: [],
      edges: [],
      total_nodes: 1,
      total_edges: 0,
    });
    const mockTransport = createMockTransport(mockInvoke);

    const client = createLearningClient(mockTransport);
    const graph = await client.getGraph("silk_road", 2);

    expect(mockInvoke).toHaveBeenCalledWith("learning_get_graph", {
      rootId: "silk_road",
      hops: 2,
    });
    expect(graph.center.name).toBe("丝绸之路");
  });

  it("calls collection CRUD methods correctly", async () => {
    const mockInvoke = vi.fn().mockImplementation((cmd) => {
      if (cmd === "learning_list_collections") {
        return Promise.resolve([
          { id: "col_1", title: "测试合集", tags: ["历史"], items_count: 0, created_at: 0, updated_at: 0 },
        ]);
      }
      if (cmd === "learning_create_collection") {
        return Promise.resolve({
          id: "col_2",
          title: "新合集",
          description: "描述",
          tags: ["地理"],
          items_count: 0,
          created_at: 0,
          updated_at: 0,
        });
      }
      return Promise.resolve(null);
    });
    const mockTransport = createMockTransport(mockInvoke);

    const client = createLearningClient(mockTransport);
    const list = await client.listCollections();
    expect(list.length).toBe(1);

    const created = await client.createCollection("新合集", "描述", ["地理"]);
    expect(created.title).toBe("新合集");
  });
});
