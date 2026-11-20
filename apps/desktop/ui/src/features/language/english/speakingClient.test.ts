import { describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import type { CommandTransport } from "../../../transport";
import { createSpeakingClient } from "./speakingClient";

function mockTransport(invoke: Mock): CommandTransport {
  return {
    invoke: (cmd, args) =>
      Promise.resolve(args !== undefined ? invoke(cmd, args) : invoke(cmd)),
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

describe("speakingClient 命令契约", () => {
  it("创建会话与提交回合使用顶层驼峰参数", async () => {
    const invoke = vi.fn().mockResolvedValue({ session: { id: "spk:1" }, turns: [] });
    const client = createSpeakingClient(mockTransport(invoke));

    await client.createSession("speaking:cafe", "req-1", "zh", "A2");
    await client.submitTurn("spk:1", "Hello", "turn-1");
    await client.submitAudio("spk:1", "turn-2", { transcript: "A latte please" });
    await client.tts("spk:1", "spk:1:t0");
    await client.capabilities();

    expect(invoke.mock.calls[0][0]).toBe("language_speaking_create_session");
    expect(invoke.mock.calls[0][1]).toEqual({
      scenarioId: "speaking:cafe",
      clientRequestId: "req-1",
      localeAssist: "zh",
      cefr: "A2",
    });
    expect(invoke.mock.calls[1][0]).toBe("language_speaking_submit_turn");
    expect(invoke.mock.calls[1][1]).toEqual({
      sessionId: "spk:1",
      text: "Hello",
      clientRequestId: "turn-1",
    });
    expect(invoke.mock.calls[2][0]).toBe("language_speaking_submit_audio");
    expect(invoke.mock.calls[2][1]).toEqual({
      sessionId: "spk:1",
      clientRequestId: "turn-2",
      transcript: "A latte please",
      audioBase64: null,
      mimeType: null,
    });
    expect(invoke.mock.calls[3][0]).toBe("language_speaking_tts");
    expect(invoke.mock.calls[4][0]).toBe("language_speaking_capabilities");
  });

  it("场景列表接口不存在时不显示成空白成功页", async () => {
    const invoke = vi.fn().mockResolvedValue(null);
    const client = createSpeakingClient(mockTransport(invoke));
    await expect(client.scenarios()).rejects.toThrow(/口语场景列表不可用/);
  });

  it("HTTP 列表包装 { items } 时拆成数组", async () => {
    const invoke = vi.fn().mockResolvedValue({ items: [{ id: "speaking:cafe" }] });
    const client = createSpeakingClient(mockTransport(invoke));
    const list = await client.scenarios();
    expect(list).toEqual([{ id: "speaking:cafe" }]);
  });
});
