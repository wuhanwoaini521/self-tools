import { describe, expect, it, vi, afterEach } from "vitest";
import type { Mock } from "vitest";
import { httpTransport } from "./transport";

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

  it("对带特殊字符的参数做 URL 编码，避免路径注入", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse(null));
    await httpTransport.invoke("history_semantic_person", { personId: "a/../b?x=1" });
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toBe("/api/v1/history/people/a%2F..%2Fb%3Fx%3D1");
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

  it("服务端返回 JSON 错误体时透出其中的 message", async () => {
    stubFetch();
    fetchMock.mockResolvedValue(jsonResponse({ message: "数据库文件缺失" }, 400));
    await expect(httpTransport.invoke("history_semantic_home")).rejects.toThrow("数据库文件缺失");
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
    await expect(httpTransport.invoke("geography_home")).rejects.toThrow(/尚无网页端接口/);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
