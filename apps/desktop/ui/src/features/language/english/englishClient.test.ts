/**
 * englishClient 的**线上契约**测试：命令名与参数键的驼峰拼写。
 *
 * 守住的不只是拼写，而是 Tauri v2 的两条硬规则：
 * 1. Rust 参数是 snake_case，前端必须发 camelCase（Tauri 只转换顶层参数）；
 * 2. `lessonAudio` 面对的是**二进制**响应，必须能处理 ArrayBuffer / TypedArray /
 *    number[] 三种实际形态，否则音频播放会静默失败。
 */
import { describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { createEnglishClient } from "./englishClient";
import type { CommandTransport } from "../../../transport";

function mockTransport(invoke: Mock): CommandTransport {
  return {
    invoke: (cmd, args) =>
      Promise.resolve(args !== undefined ? invoke(cmd, args) : invoke(cmd)),
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

describe("englishClient 命令契约", () => {
  it("查询命令使用顶层驼峰参数", async () => {
    const invoke = vi.fn().mockResolvedValue(null);
    const client = createEnglishClient(mockTransport(invoke));

    await client.book("nce:2");
    await client.lesson("nce:2:17");
    await client.search("hesitate", 5);

    const bookCall = invoke.mock.calls.find(([name]) => name === "language_course_book");
    expect(bookCall?.[1]).toEqual({ bookId: "nce:2" });
    const lessonCall = invoke.mock.calls.find(([name]) => name === "language_course_lesson");
    expect(lessonCall?.[1]).toEqual({ lessonId: "nce:2:17" });
    const searchCall = invoke.mock.calls.find(([name]) => name === "language_course_search");
    expect(searchCall?.[1]).toEqual({ query: "hesitate", limit: 5 });
  });

  it("写命令参数同样走顶层驼峰（不嵌套 request）", async () => {
    const invoke = vi.fn().mockResolvedValue({});
    const client = createEnglishClient(mockTransport(invoke));

    await client.markWord("nce:2:17", "hesitate", "fuzzy");
    await client.completeLesson("nce:2:17", 80);
    await client.updateProgress("nce:2:17", { stage: "listen", position_ms: 4200 });
    await client.lookupWord("hesitate", "Don't hesitate.", "nce:2:17");

    expect(invoke).toHaveBeenCalledWith("language_course_mark_word", {
      lessonId: "nce:2:17",
      word: "hesitate",
      mark: "fuzzy",
    });
    expect(invoke).toHaveBeenCalledWith("language_course_complete_lesson", {
      lessonId: "nce:2:17",
      quizScore: 80,
    });
    const update = invoke.mock.calls.find(([name]) => name === "language_course_update_progress");
    // ProgressPatch 的键保持后端 serde 的 snake_case（结构体内部不转换）。
    expect(update?.[1]).toEqual({
      lessonId: "nce:2:17",
      patch: { stage: "listen", position_ms: 4200 },
    });
    const lookup = invoke.mock.calls.find(([name]) => name === "language_course_lookup_word");
    expect(lookup?.[1]).toEqual({
      word: "hesitate",
      sentence: "Don't hesitate.",
      lessonId: "nce:2:17",
    });
  });

  it("导入命令传目录/文件路径", async () => {
    const invoke = vi.fn().mockResolvedValue({});
    const client = createEnglishClient(mockTransport(invoke));

    await client.nceScan("/Users/me/NCE");
    await client.nceImport("/Users/me/NCE");
    await client.dictImport("/Users/me/ecdict.csv");

    expect(invoke).toHaveBeenCalledWith("language_nce_scan", { sourceDir: "/Users/me/NCE" });
    expect(invoke).toHaveBeenCalledWith("language_nce_import", { sourceDir: "/Users/me/NCE" });
    expect(invoke).toHaveBeenCalledWith("language_dict_import", { path: "/Users/me/ecdict.csv" });
  });

  it("音频命令兼容 ArrayBuffer / TypedArray / number[]", async () => {
    const bytes = new Uint8Array([73, 68, 51]);

    const arrayBuffer = new Uint8Array(bytes).buffer;
    expect(
      await createEnglishClient(mockTransport(vi.fn().mockResolvedValue(arrayBuffer))).lessonAudio(
        "nce:1:1",
      ),
    ).toBe(arrayBuffer);

    expect(
      await createEnglishClient(mockTransport(vi.fn().mockResolvedValue(bytes))).lessonAudio(
        "nce:1:1",
      ),
    ).toBeInstanceOf(ArrayBuffer);

    expect(
      await createEnglishClient(mockTransport(vi.fn().mockResolvedValue([73, 68, 51]))).lessonAudio(
        "nce:1:1",
      ),
    ).toBeInstanceOf(ArrayBuffer);
  });

  it("音频响应类型异常时明确报错，不静默返回空", async () => {
    const client = createEnglishClient(mockTransport(vi.fn().mockResolvedValue(null)));
    await expect(client.lessonAudio("nce:1:1")).rejects.toThrow(/unexpected audio/);
  });
});
describe("跟读评分命令契约（V13 W2）", () => {
  it("评分只发识别结果与位置，绝不发目标句（由服务端查库）", async () => {
    const invoke = vi.fn().mockResolvedValue({
      overall: 80,
      accuracy: 80,
      completeness: 100,
      fluency: 70,
      duration_ms: 1200,
      target: "Excuse me!",
      transcript: "excuse me",
      missing: [],
      wrong: [],
      extra: [],
    });
    const client = createEnglishClient(mockTransport(invoke));

    const result = await client.shadowScore({
      lessonId: "nce:1:1",
      sentenceSeq: 0,
      transcript: "excuse me",
      durationMs: 1200,
      targetMs: 1500,
    });

    const [name, args] = invoke.mock.calls[0];
    expect(name).toBe("language_shadow_score");
    expect(args).toEqual({
      lessonId: "nce:1:1",
      sentenceSeq: 0,
      transcript: "excuse me",
      durationMs: 1200,
      targetMs: 1500,
      longPausesMs: [],
    });
    expect(args).not.toHaveProperty("target");
    expect(result.completeness).toBe(100);
  });

  it("统计查询按需带上 lessonId / since", async () => {
    const invoke = vi.fn().mockResolvedValue({ attempts: 0 });
    const client = createEnglishClient(mockTransport(invoke));

    await client.shadowStats();
    expect(invoke.mock.calls[0]).toEqual(["language_shadow_stats", { lessonId: undefined, since: undefined }]);

    invoke.mockClear();
    await client.shadowStats("nce:1:1", 1_700_000_000);
    expect(invoke.mock.calls[0][1]).toEqual({ lessonId: "nce:1:1", since: 1_700_000_000 });
  });
});
