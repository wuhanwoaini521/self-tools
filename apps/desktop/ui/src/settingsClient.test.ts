import { describe, expect, it, vi } from "vitest";
import type { CommandTransport } from "./transport";
import { createSettingsClient } from "./settingsClient";
import type { SecretSettingsStatus } from "./types";

function fakeTransport(invoke: CommandTransport["invoke"]): CommandTransport {
  return {
    invoke,
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

describe("settingsClient secret boundary", () => {
  it("sends credentials only through the explicit secret-write command", async () => {
    const status: SecretSettingsStatus = {
      ai_api_key_configured: true,
      travel_llm_api_key_configured: false,
      travel_amap_api_key_configured: false,
      travel_qweather_api_key_configured: false,
      travel_baidu_map_api_key_configured: false,
      geography_amap_api_key_configured: false,
      geography_amap_security_js_code_configured: false,
      jev_api_key_configured: false,
    };
    const invoke = vi.fn().mockResolvedValue(status);
    const client = createSettingsClient(fakeTransport(invoke));
    const update = {
      updates: [{ field: "ai_api_key" as const, value: "test-only-secret" }],
    };

    await expect(client.putSecrets(update)).resolves.toEqual(status);
    expect(invoke).toHaveBeenCalledWith("put_secret_settings", { update });
    expect(invoke).not.toHaveBeenCalledWith("put_settings", expect.anything());
  });
});
