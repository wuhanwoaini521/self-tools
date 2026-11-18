import { describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import type { CommandTransport } from "../../transport";
import { createReviewClient } from "./reviewClient";

function createMockTransport(invoke: Mock): CommandTransport {
  return {
    invoke: (command, args) => Promise.resolve(args !== undefined ? invoke(command, args) : invoke(command)),
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

describe("ReviewClient contract", () => {
  it("loads the review queue and stats through the transport", async () => {
    const invoke = vi.fn().mockResolvedValue([]);
    const client = createReviewClient(createMockTransport(invoke));

    await client.getQueue("history", 20);
    await client.getStats();

    expect(invoke).toHaveBeenNthCalledWith(1, "learning_get_review_queue", {
      moduleFilter: "history",
      limit: 20,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "learning_get_review_stats");
  });

  it("submits an idempotent rating through the transport", async () => {
    const invoke = vi.fn().mockResolvedValue({ interval_days: 3 });
    const client = createReviewClient(createMockTransport(invoke));

    await client.submit("card-1", "good", "request-1");

    expect(invoke).toHaveBeenCalledWith("learning_submit_review", {
      cardId: "card-1",
      rating: "good",
      requestId: "request-1",
    });
  });
});
