/**
 * 学习板客户端单测：两端命令形状不同，前端必须归一成同一份契约。
 *
 * 桌面端 Tauri 命令直接返回板，网页端 HTTP 返回 `{board, created}`；
 * 组件只认一种形状 —— 这里锁住归一逻辑，否则某一端会静默拿到 `undefined`。
 */
import { describe, expect, it } from "vitest";
import { createStudyBoardClient, type StudyBoardRecord } from "./studyBoardClient";
import type { CommandTransport } from "../../transport";

/** 记录调用并回放预设响应的假传输层。 */
function fakeTransport(responses: Record<string, unknown>): CommandTransport & {
  calls: { command: string; args: Record<string, unknown> }[];
} {
  const calls: { command: string; args: Record<string, unknown> }[] = [];
  return {
    calls,
    invoke: async <T,>(command: string, args: Record<string, unknown> = {}) => {
      calls.push({ command, args });
      if (!(command in responses)) throw new Error(`未预置响应: ${command}`);
      return responses[command] as T;
    },
    isTauriRuntime: () => false,
    subscribe: () => () => {},
  };
}

const board: StudyBoardRecord = {
  id: "b-1",
  title: "遵义会议",
  strokes: { strokes: [{ points: [0, 0] }] },
  created_at: 1,
  updated_at: 2,
  module_origin: "study-board",
};

describe("学习板客户端", () => {
  it("桌面端形状：save 直接返回板（created 视为 false）", async () => {
    const transport = fakeTransport({ study_board_save: board });
    const client = createStudyBoardClient(transport);
    const result = await client.save({ boardId: "b-1", title: "遵义会议" });
    expect(result.created).toBe(false);
    expect(result.board.id).toBe("b-1");
    expect(transport.calls[0].command).toBe("study_board_save");
  });

  it("网页端形状：{board, created} 归一为同一契约", async () => {
    const transport = fakeTransport({ study_board_save: { board, created: true } });
    const result = await createStudyBoardClient(transport).save({
      boardId: "b-1",
      strokes: { strokes: [] },
    });
    expect(result.created).toBe(true);
    expect(result.board.strokes).toEqual({ strokes: [{ points: [0, 0] }] });
  });

  it("list 接受裸数组与 {items} 两种形状", async () => {
    const items = [
      {
        id: "b-1",
        title: "板",
        module_origin: "study-board",
        created_at: 1,
        updated_at: 2,
        stroke_count: 1,
      },
    ];
    expect(await createStudyBoardClient(fakeTransport({ study_board_list: items })).list()).toEqual(
      items,
    );
    expect(
      await createStudyBoardClient(fakeTransport({ study_board_list: { items } })).list(),
    ).toEqual(items);
  });

  it("get 未命中返回 null（404 由传输层归一）", async () => {
    const client = createStudyBoardClient(fakeTransport({ study_board_get: null }));
    expect(await client.get("b-none")).toBeNull();
  });

  it("笔迹原样透传：客户端不解释 strokes", async () => {
    const strokes = { strokes: [{ points: [1, 2, 3, 4], brush: "pencil", pressures: [0.4] }] };
    const transport = fakeTransport({ study_board_save: board });
    await createStudyBoardClient(transport).save({ boardId: "b-1", strokes });
    expect(transport.calls[0].args.strokes).toEqual(strokes);
  });
});
