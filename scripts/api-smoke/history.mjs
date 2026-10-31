#!/usr/bin/env node
/**
 * Minimal non-React HTTP client for the existing read-only History API.
 * Run the backend first, then: node scripts/api-smoke/history.mjs
 * Override SELF_TOOLS_API_URL when the server is not on localhost:8080.
 */
import assert from "node:assert/strict";

const baseUrl = (process.env.SELF_TOOLS_API_URL ?? "http://127.0.0.1:8080").replace(/\/$/, "");

async function getJson(path) {
  const response = await fetch(`${baseUrl}${path}`, {
    headers: { accept: "application/json" },
  });
  const body = await response.json().catch(() => null);
  if (!response.ok) {
    const error = body && typeof body === "object" ? body : {};
    throw new Error(
      `${response.status} ${path}: ${error.code ?? "HTTP_ERROR"}: ${error.message ?? "request failed"}`,
    );
  }
  return body;
}

const health = await getJson("/health");
assert.ok(health && typeof health === "object", "health response must be JSON");

const home = await getJson("/api/v1/history/home");
assert.ok(Array.isArray(home.periods), "HistoryHomeDTO.periods must be an array");
assert.ok(home.periods.length > 0, "smoke test needs at least one published period");
const period = home.periods[0];
assert.equal(typeof period.id, "string", "PeriodSummaryDTO.id must be a string");
assert.equal(typeof period.name_zh_cn, "string", "PeriodSummaryDTO.name_zh_cn must be a string");

const detail = await getJson(`/api/v1/history/periods/${encodeURIComponent(period.id)}`);
assert.ok(detail.period && typeof detail.period === "object", "PeriodDetailDTO.period is required");
assert.equal(detail.period.id, period.id, "detail must match requested period");
for (const collection of ["events", "people", "regimes", "relations", "stages", "stories"]) {
  assert.ok(Array.isArray(detail[collection]), `PeriodDetailDTO.${collection} must be an array`);
}

console.log(
  `PASS history thin client: ${home.periods.length} periods; ` +
    `${detail.events.length} events for ${period.id}; API ${baseUrl}`,
);
