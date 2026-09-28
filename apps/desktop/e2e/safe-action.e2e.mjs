import { expect } from "@wdio/globals";
import { resolve } from "node:path";
import { openPage, waitForApp } from "./helpers.mjs";

const fixture = {
  status: {
    hostname: "e2e-host", platform: "windows", os_version: "test", uptime_secs: 120,
    cpu_usage_ratio: 0.1, cpu_cores: 8, memory_usage_ratio: 0.3, memory_total_bytes: 8_000_000,
    tightest_volume: "C:", tightest_volume_ratio: 0.2,
    health: { overall: "healthy", reasons: [] }, services: [], apps: [],
  },
  services: [{
    service_id: "fixture-service", display_name: "Fixture Service", description: "E2E fixture",
    status: "healthy", detail: "ready", checked_at: 1_790_580_000, allowed_actions: ["restart"],
  }],
  apps: [],
  audit: [],
  mcp: {
    mcp: { enabled: true, stdio_enabled: true, http_enabled: false, bind: "127.0.0.1", remote_enabled: false, port: 8787 },
    identity_configured: false, auth_status: "本地模式",
  },
  confirmation: {
    confirmation_id: "e2e-confirmation", action_type: "restart", target_id: "fixture-service",
    summary: "测试服务将重新启动", risk: "system", created_at: 1_790_580_000,
    expires_at: 1_790_580_120,
  },
  cancelCalls: 0,
  confirmCalls: 0,
};

describe("Home Server 安全操作确认", () => {
  before(async () => {
    await waitForApp();
    const installedFixture = await browser.execute((data) => {
      window.__DEVTOOLBOX_E2E_SERVER_FIXTURE__ = data;
      return window.__DEVTOOLBOX_E2E_SERVER_FIXTURE__?.services?.[0]?.service_id;
    }, fixture);
    expect(installedFixture).toBe("fixture-service");
    await openPage("Server", "server");
    await browser.waitUntil(async () => (await $(".server-row").isDisplayed()), {
      timeout: 5_000,
      timeoutMsg: "E2E Server fixture 未渲染",
    });
  });

  it("重启请求先展示确认卡；取消后不执行系统操作", async () => {
    await $(".server-refresh").click();
    await $("//li[contains(@class,'server-row')]//button[normalize-space(.)='重启']").click();
    const confirmation = await $(".server-confirm[role='dialog']");
    await expect(confirmation).toBeDisplayed();
    expect((await confirmation.getText()).includes("测试服务将重新启动")).toBe(true);
    await browser.saveScreenshot(resolve(process.cwd(), "../../output/desktop-e2e/safe-action-confirm.png"));

    await $("//section[contains(@class,'server-confirm')]//button[normalize-space(.)='取消']").click();
    await expect(confirmation).not.toBeDisplayed();
    const calls = await browser.execute(() => ({
      cancel: window.__DEVTOOLBOX_E2E_SERVER_FIXTURE__?.cancelCalls,
      confirm: window.__DEVTOOLBOX_E2E_SERVER_FIXTURE__?.confirmCalls,
    }));
    expect(calls.cancel).toBe(1);
    expect(calls.confirm).toBe(0);
  });
});
