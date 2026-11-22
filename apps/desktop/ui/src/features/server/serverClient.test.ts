import { describe, expect, it, vi } from "vitest";

import type { CommandTransport } from "../../transport";
import { createServerClient } from "./serverClient";

function fakeTransport() {
  const invoke = vi.fn(async (_command: string, _args?: Record<string, unknown>) => ({}));
  const transport: CommandTransport = {
    invoke: invoke as CommandTransport["invoke"],
    // 关键回归：网页端不再被客户端 guard 拦住，映射交给 transport。
    isTauriRuntime: () => false,
    subscribe: () => () => {},
  };
  return { transport, invoke };
}

describe("serverClient", () => {
  it("works on the web transport (no desktop-only guard)", async () => {
    const { transport, invoke } = fakeTransport();
    const client = createServerClient(transport);

    await client.status();
    await client.servicesList();
    await client.appsList();
    await client.actionsRecent(5);
    await client.mcpStatus();
    await client.logs({ serviceId: "self-tools", maxLines: 60 });

    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "server_status",
      "server_services_list",
      "server_apps_list",
      "server_actions_recent",
      "mcp_status",
      "services_get_logs",
    ]);
  });

  it("keeps restart as plan-then-confirm (no direct execution entry)", async () => {
    const { transport, invoke } = fakeTransport();
    const client = createServerClient(transport);

    await client.requestRestart("self-tools");
    expect(invoke).toHaveBeenLastCalledWith("services_restart", { serviceId: "self-tools" });

    await client.confirmAction("cfm-1", "self-tools");
    expect(invoke).toHaveBeenLastCalledWith("confirm_action", {
      confirmationId: "cfm-1",
      serviceId: "self-tools",
    });

    await client.cancelAction("cfm-1");
    expect(invoke).toHaveBeenLastCalledWith("cancel_action", { confirmationId: "cfm-1" });
  });
});
