import assert from "node:assert/strict";
import http from "node:http";
import net from "node:net";
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

async function setup(t, overrides = {}) {
  const { apiHandler, ...gatewayOverrides } = overrides;
  let apiHeaders = null;
  const api = http.createServer((req, res) => {
    apiHeaders = Object.fromEntries(Object.entries(req.headers).map(([k, v]) => [k.toLowerCase(), v]));
    if (apiHandler) {
      apiHandler(req, res);
      return;
    }
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
  web.on("upgrade", (req, socket) => {
    socket.write(
      "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\nupstream-upgrade",
    );
    socket.end();
  });
  const apiPort = await listen(api);
  const webPort = await listen(web);
  const gateway = await startGateway({
    host: "127.0.0.1",
    port: 0,
    password: "a-very-secret-password",
    apiUpstream: `http://127.0.0.1:${apiPort}`,
    webUpstream: `http://127.0.0.1:${webPort}`,
    ...gatewayOverrides,
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

function rawUpgrade(base, cookie) {
  const url = new URL(base);
  return new Promise((resolve, reject) => {
    const socket = net.connect(Number(url.port), url.hostname, () => {
      socket.write(
        [
          "GET /ws HTTP/1.1",
          `Host: ${url.host}`,
          "Connection: Upgrade",
          "Upgrade: websocket",
          "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
          "Sec-WebSocket-Version: 13",
          ...(cookie ? [`Cookie: ${cookie}`] : []),
          "",
          "",
        ].join("\r\n"),
      );
    });
    let data = "";
    socket.setTimeout(3000, () => socket.destroy(new Error("upgrade timeout")));
    const settle = () => {
      socket.destroy();
      resolve(data);
    };
    socket.on("data", (chunk) => {
      data += chunk.toString();
      if (data.includes("upstream-upgrade") || data.includes("401 Unauthorized")) settle();
    });
    socket.on("end", settle);
    socket.on("close", settle);
    socket.on("error", reject);
  });
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

test("gateway health is public and does not proxy", async (t) => {
  const { base, seen } = await setup(t);
  const response = await fetch(`${base}/__lan/health`);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), {
    ok: true,
    service: "self-tools-lan-gateway",
  });
  assert.equal(seen(), null);
});

test("tampered session cookie is rejected", async (t) => {
  const { base } = await setup(t);
  const cookie = await login(base);
  const tampered = `${cookie.slice(0, -1)}${cookie.endsWith("A") ? "B" : "A"}`;
  const response = await fetch(`${base}/api/health`, { headers: { cookie: tampered } });
  assert.equal(response.status, 401);
  assert.equal((await response.json()).code, "gateway_unauthorized");
});

test("logout revokes the session server-side", async (t) => {
  const { base } = await setup(t);
  const cookie = await login(base);
  assert.equal((await fetch(`${base}/api/health`, { headers: { cookie } })).status, 200);

  const logout = await fetch(`${base}/logout`, {
    method: "POST",
    headers: { cookie, origin: base },
    redirect: "manual",
  });
  assert.equal(logout.status, 302);
  assert.match(logout.headers.get("set-cookie") ?? "", /Max-Age=0/);

  const after = await fetch(`${base}/api/health`, { headers: { cookie } });
  assert.equal(after.status, 401);
  assert.equal((await after.json()).code, "gateway_unauthorized");
});

test("expired session is rejected", async (t) => {
  const { base } = await setup(t, { sessionHours: 0.0001 });
  const cookie = await login(base);
  await new Promise((resolve) => setTimeout(resolve, 600));
  const response = await fetch(`${base}/api/health`, { headers: { cookie } });
  assert.equal(response.status, 401);
  assert.equal((await response.json()).code, "gateway_unauthorized");
});

test("repeated wrong passwords trigger lockout", async (t) => {
  const { base } = await setup(t);
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const response = await fetch(`${base}/login`, {
      method: "POST",
      redirect: "manual",
      headers: { "content-type": "application/x-www-form-urlencoded", origin: base },
      body: new URLSearchParams({ password: `wrong-${attempt}`, next: "/" }),
    });
    assert.equal(response.status, 401);
  }
  const locked = await fetch(`${base}/login`, {
    method: "POST",
    redirect: "manual",
    headers: { "content-type": "application/x-www-form-urlencoded", origin: base },
    body: new URLSearchParams({ password: "a-very-secret-password", next: "/" }),
  });
  assert.equal(locked.status, 429);
});

test("WebSocket upgrades require login and reach the web upstream", async (t) => {
  const { base } = await setup(t);
  const anonymous = await rawUpgrade(base, null);
  assert.match(anonymous, /^HTTP\/1\.1 401 Unauthorized/);

  const cookie = await login(base);
  const authenticated = await rawUpgrade(base, cookie);
  assert.match(authenticated, /^HTTP\/1\.1 101 Switching Protocols/);
  assert.match(authenticated, /upstream-upgrade/);
});

test("SSE 进度流经网关逐块透传，不被缓冲或改写", async (t) => {
  const apiHandler = (req, res) => {
    if (!req.url.startsWith("/api/v1/ai/runs/run-sse/events")) {
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ code: "not_found", message: "no such route" }));
      return;
    }
    res.writeHead(200, {
      "content-type": "text/event-stream",
      "cache-control": "no-cache",
      "x-accel-buffering": "no",
    });
    res.write('id: 1\nevent: progress\ndata: {"seq":1,"stage":"preparing"}\n\n');
    setTimeout(() => {
      res.write('id: 2\nevent: progress\ndata: {"seq":2,"stage":"done"}\n\n');
      res.end();
    }, 50);
  };
  const { base } = await setup(t, { apiHandler });
  const cookie = await login(base);

  const response = await fetch(`${base}/api/v1/ai/runs/run-sse/events`, {
    headers: { cookie, accept: "text/event-stream" },
  });
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("content-type"), "text/event-stream");
  assert.equal(response.headers.get("cache-control"), "no-cache");
  assert.equal(response.headers.get("x-accel-buffering"), "no");

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  // 第一块必须在连接结束前到达：网关没有把整个响应 buffer 起来。
  const first = await reader.read();
  assert.equal(first.done, false);
  assert.match(decoder.decode(first.value), /id: 1/);
  let rest = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    rest += decoder.decode(value);
  }
  assert.match(rest, /id: 2/);
});
