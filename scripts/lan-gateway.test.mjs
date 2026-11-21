import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";

import { startGateway } from "./lan-gateway.mjs";

async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", reject);
      resolve();
    });
  });
  return server.address().port;
}

async function setup(t) {
  let apiHeaders = null;
  const api = http.createServer((req, res) => {
    apiHeaders = Object.fromEntries(Object.entries(req.headers).map(([k, v]) => [k.toLowerCase(), v]));
    if (req.url === "/api/health") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ ok: true, seen: apiHeaders }));
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: true, seen: apiHeaders }));
  });
  const web = http.createServer((req, res) => {
    res.writeHead(200, { "content-type": "text/html" });
    res.end("<html><body>fake web</body></html>");
  });
  const apiPort = await listen(api);
  const webPort = await listen(web);
  const gateway = await startGateway({
    host: "127.0.0.1",
    port: 0,
    password: "a-very-secret-password",
    apiUpstream: `http://127.0.0.1:${apiPort}`,
    webUpstream: `http://127.0.0.1:${webPort}`,
  });
  t.after(async () => {
    await gateway.close();
    await new Promise((resolve) => api.close(resolve));
    await new Promise((resolve) => web.close(resolve));
  });
  return {
    base: `http://127.0.0.1:${gateway.port}`,
    apiPort,
    webPort,
    seen: () => apiHeaders,
  };
}

async function login(base) {
  const response = await fetch(`${base}/login`, {
    method: "POST",
    redirect: "manual",
    headers: { "content-type": "application/x-www-form-urlencoded", origin: base },
    body: new URLSearchParams({ password: "a-very-secret-password", next: "/" }),
  });
  assert.equal(response.status, 302);
  const cookie = response.headers.get("set-cookie")?.split(";")[0];
  assert.ok(cookie?.startsWith("self_tools_lan="));
  return cookie;
}

test("unauthenticated page request redirects to login", async (t) => {
  const { base } = await setup(t);
  const response = await fetch(`${base}/`, { redirect: "manual" });
  assert.equal(response.status, 302);
  assert.match(response.headers.get("location") ?? "", /^\/login\?next=%2F/);
});

test("login rejects wrong password", async (t) => {
  const { base } = await setup(t);
  const response = await fetch(`${base}/login`, {
    method: "POST",
    redirect: "manual",
    headers: { "content-type": "application/x-www-form-urlencoded", origin: base },
    body: new URLSearchParams({ password: "wrong-password", next: "/" }),
  });
  assert.equal(response.status, 401);
  assert.match(await response.text(), /密码不正确/);
});

test("api proxy rewrites Host and Origin for the loopback API", async (t) => {
  const { base, apiPort, webPort } = await setup(t);
  const cookie = await login(base);
  const response = await fetch(`${base}/api/health`, { headers: { cookie } });
  assert.equal(response.status, 200);
  const body = await response.json();
  assert.equal(body.ok, true);
  assert.equal(body.seen.host, `127.0.0.1:${apiPort}`);
  assert.equal(body.seen.origin, `http://127.0.0.1:${webPort}`);
  assert.equal(body.seen.cookie, undefined);
});

test("cross-site writes are rejected before proxying", async (t) => {
  const { base, seen } = await setup(t);
  const cookie = await login(base);
  const response = await fetch(`${base}/api/health`, {
    method: "POST",
    headers: { cookie, origin: "http://evil.example" },
  });
  assert.equal(response.status, 403);
  assert.equal(seen(), null);
});

test("authenticated write with same-site origin reaches the API", async (t) => {
  const { base, apiPort, webPort } = await setup(t);
  const cookie = await login(base);
  const response = await fetch(`${base}/api/health`, {
    method: "POST",
    headers: { cookie, origin: base },
  });
  assert.equal(response.status, 200);
  const body = await response.json();
  assert.equal(body.seen.host, `127.0.0.1:${apiPort}`);
  assert.equal(body.seen.origin, `http://127.0.0.1:${webPort}`);
});
