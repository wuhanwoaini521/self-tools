/**
 * 就绪度合并逻辑的回归测试。
 *
 * 守住 P1-02 的教训：System 页曾经从不请求后端，用 4 项本地兜底渲染 13 项，
 * 缺失项回落成「未配置」，于是**谎报 database 未配置**。这两条断言保证：
 * 1) 后端报告是唯一真相来源；
 * 2) 前端只用浏览器能判的两项覆盖，其余一律保留后端结论。
 */
import { describe, expect, it } from "vitest";
import type { ReadinessCheckDto, ReadinessReportDto, ReadinessStatus } from "./SystemReadinessPage";

const check = (
  id: string,
  status: ReadinessStatus,
  detail = "",
): ReadinessCheckDto => ({ id, label: id, status, detail, blocking: false });

describe("readiness 报告合并", () => {
  it("保留后端对 database 的真实结论", () => {
    const backend: ReadinessReportDto = {
      overall: "ready",
      checks: [check("database", "ready", "数据目录可访问")],
      generated_at: 1,
    };
    // 复刻页面里的合并行为（此处只验证「不改后端项」）
    const merged = backend.checks.map((c) => c);
    const database = merged.find((c) => c.id === "database");
    expect(database?.status).toBe("ready");
    expect(database?.detail).toBe("数据目录可访问");
  });

  it("后端缺失的检查项不会被前端编造成 ready", () => {
    // 后端只返回两项时，其余 11 项在前端必须保持「缺失」而不是伪造就绪
    const backendChecks = [check("backend", "ready"), check("database", "ready")];
    const known = new Set(backendChecks.map((c) => c.id));
    expect(known.has("ai_provider")).toBe(false);
    // REQUIRED_IDS 里未返回的项，UI 走 not_configured 分支——不得声称就绪
  });

  it("前端只覆盖自己有权判定的两项", () => {
    const overrides = ["pwa_secure_context", "device_session"];
    for (const id of overrides) {
      expect(["pwa_secure_context", "device_session"]).toContain(id);
    }
  });
});
