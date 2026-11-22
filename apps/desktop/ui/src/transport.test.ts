import { describe, expect, it, vi, afterEach } from "vitest";
import type { Mock } from "vitest";
import {
  HttpTransportError,
  defaultTransport,
  httpTransport,
  patchRequest,
} from "./transport";

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

  it("maps Home Server reads and the restart ticket flow to /api/v1/server", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({}));

    await httpTransport.invoke("server_status");
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/server/status", expect.anything());
    await httpTransport.invoke("server_services_list");
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/server/services", expect.anything());
    await httpTransport.invoke("server_apps_list");
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/server/apps", expect.anything());
    await httpTransport.invoke("server_actions_recent", { limit: 5 });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/server/actions/recent?limit=5", expect.anything());
    await httpTransport.invoke("services_get_logs", {
      serviceId: "self-tools",
      maxLines: 60,
      maxBytes: 4096,
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/server/logs?serviceId=self-tools&maxLines=60&maxBytes=4096",
      expect.anything(),
    );
    await httpTransport.invoke("mcp_status");
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/server/mcp", expect.anything());

    await httpTransport.invoke("services_restart", { serviceId: "self-tools" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/server/actions/restart",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ serviceId: "self-tools" }) }),
    );
    await httpTransport.invoke("confirm_action", {
      confirmationId: "cfm-1",
      serviceId: "self-tools",
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/server/actions/cfm-1/confirm",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ serviceId: "self-tools" }) }),
    );
    await httpTransport.invoke("cancel_action", { confirmationId: "cfm-1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/server/actions/cfm-1/cancel",
      expect.objectContaining({ method: "POST", body: "{}" }),
    );
  });

  it("uploads only remote-safe settings fields and posts secrets separately", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({}));
    const settings = {
      schema_version: 1,
      recent_files: ["E:/notes/a.md"],
      workspace_path: "E:/work",
      theme_mode: "dark",
      ui_theme: "warm-editorial",
      rss_refresh_minutes: 30,
      editor_font_size: 15,
      auto_save: true,
      markdown_default_view: "preview",
      travel: {
        search_backend: "auto",
        searxng_url: null,
        llm_base_url: null,
        llm_model: "deepseek-chat",
        qweather_api_host: null,
      },
      geography: {},
      ai: { provider: "openai", model: "gpt-5", base_url: null, timeout_secs: 60 },
      knowledge: { roots: [] },
      server: { services: [], applications: [] },
      decision: {
        mode: "rule",
        jev_base_url: null,
        jev_model: null,
        jev_timeout_secs: 30,
        jev_failure_fallback: true,
        multi_agent_enabled: false,
        max_workers: 2,
      },
      secrets: { ai_api_key_configured: true },
    };

    await httpTransport.invoke("put_settings", { settings });
    const [url, init] = fetchMock.mock.calls.at(-1) as [string, RequestInit];
    expect(url).toBe("/api/v1/settings/public");
    expect(init.method).toBe("PUT");
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    // 主机绑定字段与凭据状态都不上行；服务端对它们会直接 400。
    for (const blocked of ["workspace_path", "recent_files", "knowledge", "server", "secrets"]) {
      expect(body).not.toHaveProperty(blocked);
    }
    expect(body.ui_theme).toBe("warm-editorial");
    expect(body.ai).toEqual({
      provider: "openai",
      model: "gpt-5",
      base_url: "",
      timeout_secs: 60,
    });

    await httpTransport.invoke("put_secret_settings", {
      update: { updates: [{ field: "ai_api_key", value: "sk-test" }] },
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/settings/secrets",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ updates: [{ field: "ai_api_key", value: "sk-test" }] }),
      }),
    );
  });

  it("maps language data writes: starter install, async import cancel, speaking feedback", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({}));

    await httpTransport.invoke("language_install_starter", { only: null });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/language/starter/install",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ only: null }) }),
    );

    await httpTransport.invoke("language_nce_import_cancel", { jobId: "job-1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/language/nce/import/job-1/cancel",
      expect.objectContaining({ method: "POST" }),
    );
    await httpTransport.invoke("language_dict_import_cancel", { jobId: "job-2" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/language/dict/import/job-2/cancel",
      expect.objectContaining({ method: "POST" }),
    );

    // 评分请求只发 sentenceId + 转写；目标句由服务端查库。
    await httpTransport.invoke("language_speaking_feedback", {
      request: {
        sentenceId: "sentence-1",
        transcript: "hello",
        durationMs: 1200,
        targetMs: 1000,
        longPausesMs: [],
      },
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/language/speaking/feedback",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          sentenceId: "sentence-1",
          transcript: "hello",
          durationMs: 1200,
          targetMs: 1000,
          longPausesMs: [],
        }),
      }),
    );
  });

  it("maps RSS reads, writes, and deletion to the shared HTTP endpoints", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse([]));
    await httpTransport.invoke("list_rss_feeds");
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/rss/feeds", expect.any(Object));
    await httpTransport.invoke("list_rss_articles", { feedId: 7, limit: 50 });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/rss/feeds/7/articles?limit=50", expect.any(Object));
    await httpTransport.invoke("add_rss_feed", { url: "https://example.com/feed.xml" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/rss/feeds",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ url: "https://example.com/feed.xml" }) }),
    );
    await httpTransport.invoke("delete_rss_feed", { feedId: 7 });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/rss/feeds/7", expect.objectContaining({ method: "DELETE" }));
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

  it("Knowledge 读写映射到共享端点，请求体与桌面端 DTO 同形", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ ok: true }));

    // 单条读取：路径参数必须编码，否则带「/」的 id 会被当成多级路径。
    await httpTransport.invoke("memory_get", { id: "memory/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/memory/memory%2F1",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    await httpTransport.invoke("documents_get", { documentId: "doc/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/documents/doc%2F1",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    await httpTransport.invoke("knowledge_status");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/knowledge/status",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    // 写操作：桌面端把 DTO 包在 `request` 里，HTTP 契约直接发 DTO 本体。
    const draft = {
      category: "preference",
      content: "喜欢安静的咖啡馆",
      source_type: "explicit_user",
    };
    await httpTransport.invoke("memory_save", { request: draft });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/memory",
      expect.objectContaining({ method: "POST", body: JSON.stringify(draft) }),
    );

    await httpTransport.invoke("memory_confirm", { id: "memory/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/memory/memory%2F1/confirm",
      expect.objectContaining({ method: "POST" }),
    );

    await httpTransport.invoke("memory_update", {
      request: { id: "memory/1", content: "改过的内容" },
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/memory/update",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ id: "memory/1", content: "改过的内容" }),
      }),
    );

    await httpTransport.invoke("documents_scan", { rootId: "docs" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/documents/scan",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ rootId: "docs" }) }),
    );

    await httpTransport.invoke("documents_read", {
      request: { document_id: "doc-1", max_chars: 500 },
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/documents/read",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ document_id: "doc-1", max_chars: 500 }),
      }),
    );

    await httpTransport.invoke("files_read_text", { target: "files/1", maxChars: 200 });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/files/read_text",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ target: "files/1", maxChars: 200 }),
      }),
    );

    // 网页端没有 files_open：能力缺失必须显式报错，不能静默成功。
    await expect(httpTransport.invoke("files_open", { target: "files/1" })).rejects.toThrow(
      /尚无网页端接口/,
    );
  });

  it("PATCH 请求体按端点显式声明，多字段更新不会被截断", () => {
    // 旧的硬编码 `{ note }` 会把任何多字段 PATCH 静默截成单个 note。
    const multi = {
      conversation_update: {
        path: (args: Record<string, unknown>) => `/api/v1/conversations/${String(args.id)}`,
        body: (args: Record<string, unknown>) => ({ title: args.title, archived: args.archived }),
      },
    };
    expect(
      patchRequest("conversation_update", { id: "c1", title: "新标题", archived: true }, multi),
    ).toEqual({ url: "/api/v1/conversations/c1", body: { title: "新标题", archived: true } });

    // 已上线的备注端点契约保持不变。
    expect(patchRequest("learning_update_collection_item_note", { itemId: "i1" })).toEqual({
      url: "/api/v1/learning/collection-items/i1/note",
      body: { note: null },
    });
    expect(patchRequest("not_a_patch_command", {})).toBeNull();
  });

  it("会话历史映射到 /api/v1/conversations，HTTP 动词与桌面命令同义", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ ok: true }));

    await httpTransport.invoke("conversation_list", { limit: 20, includeArchived: true });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/conversations?limit=20&includeArchived=true",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    await httpTransport.invoke("conversation_load", { id: "conv/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/conversations/conv%2F1",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );

    await httpTransport.invoke("conversation_create", { title: null });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/conversations",
      expect.objectContaining({ method: "POST" }),
    );

    const message = {
      id: "conv/1",
      role: "assistant",
      content: "第一天：西湖。",
      provider: "openai-compatible",
      model: "step-5-preview",
    };
    await httpTransport.invoke("conversation_append", message);
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/conversations/conv%2F1/messages",
      expect.objectContaining({ method: "POST", body: JSON.stringify(message) }),
    );

    // 多字段 PATCH：改名与归档同一条命令，body 不能被截成单字段。
    await httpTransport.invoke("conversation_update", { id: "conv/1", title: "改名", archived: null });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/conversations/conv%2F1",
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({ title: "改名", archived: null }),
      }),
    );

    await httpTransport.invoke("conversation_delete", { id: "conv/1" });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/conversations/conv%2F1",
      expect.objectContaining({ method: "DELETE" }),
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
    await expect(httpTransport.invoke("cycle_task_lines")).rejects.toThrow(/尚无网页端接口/);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("httpTransport：AI 进度 SSE 订阅", () => {
  class FakeEventSource {
    static instances: FakeEventSource[] = [];
    readonly url: string;
    closed = false;
    onerror: (() => void) | null = null;
    private listeners = new Map<string, ((event: MessageEvent) => void)[]>();

    constructor(url: string) {
      this.url = url;
      FakeEventSource.instances.push(this);
    }

    addEventListener(type: string, listener: (event: MessageEvent) => void): void {
      const listeners = this.listeners.get(type) ?? [];
      listeners.push(listener);
      this.listeners.set(type, listeners);
    }

    close(): void {
      this.closed = true;
    }

    emit(type: string, payload: unknown): void {
      for (const listener of this.listeners.get(type) ?? []) {
        listener({ data: JSON.stringify(payload) } as MessageEvent);
      }
    }

    fail(): void {
      this.onerror?.();
    }
  }

  function stubEventSource(): void {
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
  }

  it("按 trace 打开 SSE，按 seq 幂等投递，并在终态主动关闭", () => {
    stubEventSource();
    const handler = vi.fn();
    const stop = httpTransport.subscribe("agent-progress", handler, { traceId: "run-1" });

    const source = FakeEventSource.instances[0];
    expect(source.url).toBe("/api/v1/ai/runs/run-1/events");

    source.emit("progress", { trace_id: "run-1", seq: 1, stage: "preparing", state: "running" });
    source.emit("progress", { trace_id: "run-1", seq: 2, stage: "thinking", state: "running" });
    source.emit("progress", { trace_id: "run-1", seq: 2, stage: "thinking", state: "running" });
    expect(handler).toHaveBeenCalledTimes(2);

    source.emit("progress", { trace_id: "run-1", seq: 3, stage: "done", state: "done" });
    expect(handler).toHaveBeenCalledTimes(3);
    expect(source.closed).toBe(true);

    stop();
    expect(source.closed).toBe(true);
  });

  it("defaultTransport 把 traceId 透传给 HTTP 分支", () => {
    stubEventSource();
    // 测试环境没有浏览器 window；补一个空对象让运行时探测走 HTTP 分支。
    vi.stubGlobal("window", {});
    defaultTransport.subscribe("agent-progress", vi.fn(), { traceId: "run-9" });
    expect(FakeEventSource.instances[0].url).toBe("/api/v1/ai/runs/run-9/events");
  });

  it("没有 trace 或不是进度事件时退化为 no-op", () => {
    stubEventSource();
    const handler = vi.fn();
    const stop = httpTransport.subscribe("agent-progress", handler);
    expect(stop).toBeTypeOf("function");
    stop();
    expect(FakeEventSource.instances).toHaveLength(0);

    httpTransport.subscribe("other-event", handler, { traceId: "run-1" });
    expect(FakeEventSource.instances).toHaveLength(0);
  });

  it("SSE 出错时用状态端点恢复最近状态（游标过期/断线）", async () => {
    stubEventSource();
    stubFetch();
    fetchMock.mockResolvedValue(
      jsonResponse({
        trace_id: "run-1",
        state: "done",
        cursor: 4,
        latest: { trace_id: "run-1", seq: 4, stage: "done", state: "done" },
      }),
    );
    const handler = vi.fn();
    httpTransport.subscribe("agent-progress", handler, { traceId: "run-1" });
    const source = FakeEventSource.instances[0];

    source.fail();
    await vi.waitFor(() => expect(handler).toHaveBeenCalledTimes(1));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/ai/runs/run-1",
      expect.objectContaining({ headers: { Accept: "application/json" } }),
    );
    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({ seq: 4, stage: "done" }),
    );
    await vi.waitFor(() => expect(source.closed).toBe(true));
  });
});
