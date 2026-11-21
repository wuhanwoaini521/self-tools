import { expect } from "@wdio/globals";
import { waitForApp } from "./helpers.mjs";

describe("Settings credentials survive a desktop restart", () => {
  before(async () => waitForApp());

  it("reopens persisted credentials without exposing the value, then clears the fixture", async () => {
    const result = await browser.execute(async (marker) => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") throw new Error("Tauri invoke bridge unavailable");
      // 自带写入，避免依赖上一个 spec 的 UI 保存动作（spec 顺序不应决定成败）。
      await invoke("put_secret_settings", {
        update: { updates: [{ field: "ai_api_key", value: marker }] },
      });
      const settings = await invoke("get_settings");
      const serialized = JSON.stringify(settings);
      await invoke("put_secret_settings", {
        update: { updates: [{ field: "ai_api_key", value: null }] },
      });
      const cleared = await invoke("get_settings");
      return {
        persisted: settings.secrets?.ai_api_key_configured === true,
        leaked: serialized.includes(marker),
        cleared: cleared.secrets?.ai_api_key_configured === false,
      };
    }, "phase1-secret-regression-marker");

    expect(result).toEqual({ persisted: true, leaked: false, cleared: true });
  });
});
