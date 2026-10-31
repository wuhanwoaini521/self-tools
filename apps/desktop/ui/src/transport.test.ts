import { describe, expect, it, vi, afterEach } from "vitest";
import type { Mock } from "vitest";
import { HttpTransportError, httpTransport } from "./transport";

/**
 * 网页端传输层的契约测试。
 *
 * 这些用例守的是「命令 → REST 端点」的映射与错误语义：
 * 映射错 → 页面显示空白但没有报错；错误吞掉 → 服务没启动被伪装成「没有数据」。
 */

const fetchMock = vi.fn();

function stubFetch(): void {
  vi.stubGlobal("fetch", fetchMock);
}

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  } as unknown as Response;
}

function textResponse(status: number): Response {
  return {
    ok: false,
    status,
    json: () => Promise.reject(new SyntaxError("not json")),
  } as unknown as Response;
}

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

describe("httpTransport：命令映射", () => {
  it("把行程读取与保存映射到共享的 Travel REST 契约", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ context: { city: "杭州" } }));
    const context = {
      city: "浙江省杭州市",
      days: 2,
      date_range: { start: "2026-10-01", end: "2026-10-02" },
    };

    await httpTransport.invoke("travel_plan_get", { context });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/travel/plans?city=%E6%B5%99%E6%B1%9F%E7%9C%81%E6%9D%AD%E5%B7%9E%E5%B8%82&days=2&date_start=2026-10-01&date_end=2026-10-02",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    await httpTransport.invoke("travel_plan_recent", { limit: 8 });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/travel/plans/recent?limit=8",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    const plan = { context, selected_keys: ["poi-1"] };
    await httpTransport.invoke("travel_plan_save", { plan });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/travel/plans",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ plan }) }),
    );
  });

  it("maps PersonalAgent chat, cancellation, and Travel proposal resolution to versioned REST endpoints", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ messages: [], actions: [], citations: [], proposed_changes: [] }));
    const request = { message: "添加景点", app_context: { module: "travel" }, capabilities: ["travel"] };
    await httpTransport.invoke("personal_ai_chat", { request, trace_id: "run-http-1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/ai/chat",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ request, trace_id: "run-http-1" }) }),
    );
    await httpTransport.invoke("personal_ai_cancel_run", { traceId: "run/1" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/ai/runs/run%2F1/cancel", expect.objectContaining({ method: "POST" }));
    await httpTransport.invoke("personal_ai_confirm_travel_change", { proposalId: "proposal/1" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/ai/travel-changes/proposal%2F1/confirm", expect.objectContaining({ method: "POST" }));
    await httpTransport.invoke("personal_ai_cancel_travel_change", { proposalId: "proposal/1" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/ai/travel-changes/proposal%2F1/cancel", expect.objectContaining({ method: "POST" }));
  });

  it("把 history 命令映射到 /api/v1/history 端点并编码参数", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ periods: [], stories: [], stats: {} }));

    await httpTransport.invoke("history_semantic_home");
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/history/home",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    await httpTransport.invoke("history_semantic_period", { periodId: "period-xia" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/history/periods/period-xia",
      expect.anything(),
    );

    await httpTransport.invoke("history_semantic_search", { query: "商汤" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/history/search?q=%E5%95%86%E6%B1%A4",
      expect.anything(),
    );
  });

  it("通过稳定实体标识访问 Geography / History crosswalk 契约", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ crosswalk: { geography_entity_id: "chengdu" }, events: [] }));

    await httpTransport.invoke("exploration_location_history", { id: "chengdu/../../x" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/exploration/geography/chengdu%2F..%2F..%2Fx/history",
      expect.anything(),
    );

    await httpTransport.invoke("exploration_geography_for_history_place", { placeId: "cbdb-place-406333" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/exploration/history-places/cbdb-place-406333/geography",
      expect.anything(),
    );
  });

  it("对带特殊字符的参数做 URL 编码，避免路径注入", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse(null));
    await httpTransport.invoke("history_semantic_person", { personId: "a/../b?x=1" });
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toBe("/api/v1/history/people/a%2F..%2Fb%3Fx%3D1");
  });

  it("Learning progress lookup maps its entity key to the shared API contract", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ entity_key: "history:event:e1", review_count: 1 }));

    await httpTransport.invoke("learning_get_progress", { entityKey: "history:event:e/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/learning/progress/key?entityKey=history%3Aevent%3Ae%2F1",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );
  });

  it("Learning review card endpoints preserve source IDs and send browser writes explicitly", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ ok: true }));

    await httpTransport.invoke("learning_get_review_card", { cardId: "knowledge:history:event:e/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/learning/review/cards/knowledge%3Ahistory%3Aevent%3Ae%2F1",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    const args = {
      draft: {
        module: "history",
        entityType: "event",
        entityId: "e1",
        sourceTitle: "事件一",
        prompt: "问题",
        answer: "答案",
        userNote: "个人笔记",
      },
    };
    await httpTransport.invoke("learning_save_knowledge_review_card", args);
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/learning/review/cards/from-knowledge",
      expect.objectContaining({ method: "POST", body: JSON.stringify(args) }),
    );

    await httpTransport.invoke("learning_delete_knowledge_review_card", { cardId: "knowledge:history:event:e1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/learning/review/cards/knowledge%3Ahistory%3Aevent%3Ae1",
      expect.objectContaining({ method: "DELETE" }),
    );

    await httpTransport.invoke("learning_update_collection_item_note", {
      itemId: "collection-item/1",
      note: "修订后的备注",
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/learning/collection-items/collection-item%2F1/note",
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ note: "修订后的备注" }) }),
    );
  });

  it("home 原样返回服务端结构（与桌面端同一批 Rust 结构体）", async () => {
    stubFetch();
    const payload = { periods: [{ id: "p1" }], stories: [], stats: { people: 5 } };
    fetchMock.mockResolvedValue(jsonResponse(payload));
    const result = await httpTransport.invoke<typeof payload>("history_semantic_home");
    expect(result).toEqual(payload);
  });
});

describe("httpTransport：错误语义", () => {
  it("404 映射为 null，与桌面端「查不到返回 null」一致", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ message: "not found" }, 404));
    await expect(httpTransport.invoke("history_semantic_period", { periodId: "x" })).resolves.toBeNull();
  });

  it("服务端返回 JSON 错误体时透出 message、status 与 code", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(
      jsonResponse({ code: "travel_plan_conflict", message: "行程已在其他位置更新" }, 409),
    );
    const failure = await httpTransport
      .invoke("travel_plan_save", { plan: {} })
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(HttpTransportError);
    expect(failure).toMatchObject({
      status: 409,
      code: "travel_plan_conflict",
      message: "行程已在其他位置更新",
    });
  });

  it("非 JSON 错误体（代理上游未启动）提示「确认服务已启动」而不是干巴巴的 500", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(textResponse(500));
    await expect(httpTransport.invoke("history_semantic_home")).rejects.toThrow(/请确认它已启动/);
  });

  it("fetch 本身失败时给出可操作文案", async () => {
    stubFetch();
    fetchMock.mockRejectedValue(new TypeError("fetch failed"));
    await expect(httpTransport.invoke("history_semantic_home")).rejects.toThrow(
      /无法连接本地数据服务/,
    );
  });

  it("未映射的命令明确报错，而不是静默返回空", async () => {
    stubFetch();
    // 用一个**确实还没接**的命令。
    // 历史上这里先后用过 geography_home、memory_list：它们当时都没端点，
    // 后来接上了，这个反例就变成了假阳性（断言命令未映射，但命令其实已映射）。
    // 因此反例必须随接线进度更新——现在选一个仍然只存在于桌面端的命令。
    await expect(httpTransport.invoke("server_services_list")).rejects.toThrow(/尚无网页端接口/);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
