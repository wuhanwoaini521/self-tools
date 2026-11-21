#!/usr/bin/env node
// Password-protected LAN gateway for the browser/PWA client.
//
// The product HTTP API intentionally stays bound to 127.0.0.1 and rejects
// non-loopback Host/Origin. This gateway is the authenticated edge: pads talk
// only to this process, and it forwards /api to the loopback API with a
// loopback Origin. Do not expose the raw API port to the LAN.
//
// Environment:
//   SELF_TOOLS_LAN_PASSWORD   required, password for the login form
//   SELF_TOOLS_LAN_HOST       default 0.0.0.0
//   SELF_TOOLS_LAN_PORT       default 1421
//   SELF_TOOLS_WEB_UPSTREAM   default http://127.0.0.1:1420 (Vite)
//   SELF_TOOLS_API_UPSTREAM   default http://127.0.0.1:8080
//   SELF_TOOLS_LAN_CERT / _KEY  optional PEM pair to enable HTTPS (PWA install)
//   SELF_TOOLS_LAN_SESSION_HOURS default 168

import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import { pathToFileURL } from "node:url";

const COOKIE_NAME = "self_tools_lan";
const HOP_BY_HOP = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);
const MAX_LOGIN_BODY = 8 * 1024;
const MAX_FAILED_ATTEMPTS = 5;
const LOCKOUT_MS = 15 * 60 * 1000;

function loadConfig(overrides = {}) {
  const env = process.env;
  const config = {
    host: overrides.host ?? env.SELF_TOOLS_LAN_HOST ?? "0.0.0.0",
    port: Number(overrides.port ?? env.SELF_TOOLS_LAN_PORT ?? 1421),
    password: overrides.password ?? env.SELF_TOOLS_LAN_PASSWORD,
    webUpstream: overrides.webUpstream ?? env.SELF_TOOLS_WEB_UPSTREAM ?? "http://127.0.0.1:1420",
    apiUpstream: overrides.apiUpstream ?? env.SELF_TOOLS_API_UPSTREAM ?? "http://127.0.0.1:8080",
    cert: overrides.cert ?? env.SELF_TOOLS_LAN_CERT,
    key: overrides.key ?? env.SELF_TOOLS_LAN_KEY,
    sessionHours: Number(overrides.sessionHours ?? env.SELF_TOOLS_LAN_SESSION_HOURS ?? 168),
  };
  if (typeof config.password !== "string" || config.password.length < 8) {
    throw new Error("SELF_TOOLS_LAN_PASSWORD must be set and at least 8 characters long");
  }
  if (!Number.isInteger(config.port) || config.port < 0 || config.port > 65535) {
    throw new Error("SELF_TOOLS_LAN_PORT must be an integer between 0 and 65535");
  }
  if (!Number.isFinite(config.sessionHours) || config.sessionHours <= 0) {
    throw new Error("SELF_TOOLS_LAN_SESSION_HOURS must be a positive number");
  }
  for (const name of ["webUpstream", "apiUpstream"]) {
    const parsed = new URL(config[name]);
    if (!["http:", "https:"].includes(parsed.protocol)) {
      throw new Error(`${name} must be an http(s) URL`);
    }
    config[name] = parsed;
  }
  return config;
}

function copyHeaders(headers, { dropCookie = true } = {}) {
  const result = {};
  for (const [name, value] of Object.entries(headers)) {
    const lower = name.toLowerCase();
    if (HOP_BY_HOP.has(lower)) continue;
    if (dropCookie && (lower === "cookie" || lower === "authorization")) continue;
    result[name] = value;
  }
  return result;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function safeNext(value) {
  return typeof value === "string" && value.startsWith("/") && !value.startsWith("//") && !value.includes("\\")
    ? value
    : "/";
}

function loginHtml(next, error = "") {
  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Self Tools · 局域网登录</title>
<style>
  :root { color-scheme: light dark; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f3f5f8; color: #18202c; }
  main { width: min(92vw, 380px); padding: 32px; border-radius: 18px; background: #fff; box-shadow: 0 12px 40px rgba(20,32,50,.10); }
  h1 { font-size: 20px; margin: 0 0 6px; }
  p { font-size: 14px; color: #5d6879; margin: 0 0 22px; }
  label { display: block; font-size: 13px; font-weight: 600; margin-bottom: 8px; }
  input { width: 100%; box-sizing: border-box; padding: 12px 14px; font-size: 16px; border: 1px solid #ccd4e0; border-radius: 10px; }
  input:focus { outline: 2px solid #3d6ee8; outline-offset: 1px; border-color: #3d6ee8; }
  button { width: 100%; margin-top: 16px; padding: 12px; font-size: 16px; font-weight: 600; color: #fff; background: #3d6ee8; border: 0; border-radius: 10px; cursor: pointer; }
  button:active { transform: translateY(1px); }
  .error { margin: 0 0 14px; color: #b42318; font-size: 13px; }
  @media (prefers-color-scheme: dark) {
    body { background: #121721; color: #e8edf6; }
    main { background: #1b2230; box-shadow: none; }
    p { color: #a8b3c5; }
    input { background: #111722; border-color: #3b4658; color: inherit; }
  }
</style>
</head>
<body>
<main>
  <h1>Self Tools</h1>
  <p>输入局域网访问密码后即可使用网页版。</p>
  ${error ? `<p class="error" role="alert">${escapeHtml(error)}</p>` : ""}
  <form method="post" action="/login">
    <input type="hidden" name="next" value="${escapeHtml(next)}">
    <label for="password">访问密码</label>
    <input id="password" name="password" type="password" autocomplete="current-password" autofocus required>
    <button type="submit">登录</button>
  </form>
</main>
</body>
</html>`;
}

function readBody(req, limit) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size > limit) {
        reject(new Error("body too large"));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function send(res, status, body, headers = {}) {
  res.writeHead(status, { "content-type": "text/plain; charset=utf-8", ...headers });
  res.end(body);
}

function json(res, status, value, headers = {}) {
  send(res, status, JSON.stringify(value), { "content-type": "application/json; charset=utf-8", ...headers });
}

function redirect(res, location, headers = {}) {
  res.writeHead(302, { location, "cache-control": "no-store", ...headers });
  res.end();
}

function proxy(req, res, target, { origin, secure = false } = {}) {
  const headers = copyHeaders(req.headers);
  headers.host = target.host;
  if (origin) headers.origin = origin;
  if (secure) {
    headers["x-forwarded-proto"] = "https";
  } else if (headers["x-forwarded-proto"] === undefined) {
    headers["x-forwarded-proto"] = "http";
  }
  const upstream = (target.protocol === "https:" ? https : http).request(
    {
      hostname: target.hostname,
      port: target.port || (target.protocol === "https:" ? 443 : 80),
      method: req.method,
      path: `${target.pathname}${target.search}`,
      headers,
    },
    (upstreamRes) => {
      const responseHeaders = copyHeaders(upstreamRes.headers, { dropCookie: false });
      res.writeHead(upstreamRes.statusCode ?? 502, responseHeaders);
      upstreamRes.pipe(res);
    },
  );
  upstream.on("error", (error) => {
    if (!res.headersSent) json(res, 502, { code: "gateway_upstream_unavailable", message: `上游服务不可用：${error.message}` });
    else res.destroy(error);
  });
  req.pipe(upstream);
}

export async function startGateway(overrides = {}) {
  const config = loadConfig(overrides);
  const sessionSecret = crypto.randomBytes(32);
  const passwordSalt = crypto.randomBytes(16);
  const passwordHash = crypto.scryptSync(config.password, passwordSalt, 32);
  const sessionTtlMs = config.sessionHours * 3600_000;
  const failedAttempts = new Map();
  const revokedSessions = new Set();

  const verifyPassword = (candidate) => {
    if (typeof candidate !== "string") return false;
    const attempt = crypto.scryptSync(candidate, passwordSalt, 32);
    return attempt.length === passwordHash.length && crypto.timingSafeEqual(attempt, passwordHash);
  };

  const sign = (payload) => crypto.createHmac("sha256", sessionSecret).update(payload).digest("base64url");

  const issueSession = () => {
    const now = Date.now();
    for (const token of revokedSessions) {
      try {
        const [payload] = token.split(".");
        const parsed = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
        if (typeof parsed.exp !== "number" || parsed.exp <= now) revokedSessions.delete(token);
      } catch {
        revokedSessions.delete(token);
      }
    }
    const payload = Buffer.from(JSON.stringify({ exp: now + sessionTtlMs, nonce: crypto.randomBytes(8).toString("hex") })).toString("base64url");
    return `${payload}.${sign(payload)}`;
  };

  const sessionToken = (req) => {
    const cookie = req.headers.cookie
      ?.split(";")
      .map((part) => part.trim())
      .find((part) => part.startsWith(`${COOKIE_NAME}=`));
    return cookie ? cookie.slice(COOKIE_NAME.length + 1) : null;
  };

  const isAuthenticated = (req) => {
    const token = sessionToken(req);
    if (!token || revokedSessions.has(token)) return false;
    const [payload, signature] = token.split(".");
    if (!payload || !signature) return false;
    const expected = Buffer.from(sign(payload));
    const actual = Buffer.from(signature);
    if (expected.length !== actual.length || !crypto.timingSafeEqual(expected, actual)) return false;
    try {
      const parsed = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
      return typeof parsed.exp === "number" && parsed.exp > Date.now();
    } catch {
      return false;
    }
  };

  const cookieFor = (token, secure) =>
    `${COOKIE_NAME}=${token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${Math.floor(sessionTtlMs / 1000)}${secure ? "; Secure" : ""}`;

  const externalOrigin = (req, secure) => {
    const host = req.headers.host;
    if (!host) return null;
    return `${secure ? "https" : "http"}://${host}`;
  };

  const requestHandler = async (req, res) => {
    const secure = Boolean(config.cert && config.key);
    const url = new URL(req.url ?? "/", externalOrigin(req, secure) ?? "http://localhost");
    const pathname = url.pathname;

    if (pathname === "/__lan/health") {
      json(res, 200, { ok: true, service: "self-tools-lan-gateway" });
      return;
    }

    if (pathname === "/login" && req.method === "GET") {
      if (isAuthenticated(req)) {
        redirect(res, safeNext(url.searchParams.get("next")));
        return;
      }
      send(res, 200, loginHtml(safeNext(url.searchParams.get("next"))), { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
      return;
    }

    if (pathname === "/login" && req.method === "POST") {
      const remote = req.socket.remoteAddress ?? "unknown";
      const now = Date.now();
      const state = failedAttempts.get(remote);
      if (state && state.until > now) {
        send(res, 429, "尝试次数过多，请稍后再试。");
        return;
      }
      let next = "/";
      let password = "";
      try {
        const params = new URLSearchParams(await readBody(req, MAX_LOGIN_BODY));
        next = safeNext(params.get("next"));
        password = params.get("password") ?? "";
      } catch {
        send(res, 400, "登录请求无效。");
        return;
      }
      if (!verifyPassword(password)) {
        const failures = (state?.failures ?? 0) + 1;
        failedAttempts.set(remote, {
          failures: failures % MAX_FAILED_ATTEMPTS,
          until: failures >= MAX_FAILED_ATTEMPTS ? now + LOCKOUT_MS : 0,
        });
        send(res, 401, loginHtml(next, "密码不正确。"), { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
        return;
      }
      failedAttempts.delete(remote);
      redirect(res, next, { "set-cookie": cookieFor(issueSession(), secure) });
      return;
    }

    if (pathname === "/logout" && req.method === "POST") {
      if (!isAuthenticated(req)) {
        json(res, 401, { code: "gateway_unauthorized", message: "请先登录" });
        return;
      }
      const token = sessionToken(req);
      if (token) revokedSessions.add(token);
      redirect(res, "/login", {
        "set-cookie": `${COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0${secure ? "; Secure" : ""}`,
      });
      return;
    }

    if (!isAuthenticated(req)) {
      if (pathname.startsWith("/api/") || pathname.startsWith("/__qa/")) {
        json(res, 401, { code: "gateway_unauthorized", message: "请先登录局域网网关" });
      } else {
        const target = pathname + (url.search ?? "");
        redirect(res, `/login?next=${encodeURIComponent(target)}`);
      }
      return;
    }

    const method = req.method ?? "GET";
    if (!["GET", "HEAD", "OPTIONS"].includes(method)) {
      const origin = req.headers.origin;
      if (origin && origin !== externalOrigin(req, secure)) {
        json(res, 403, { code: "gateway_bad_origin", message: "跨站写入被拒绝" });
        return;
      }
    }

    if (pathname === "/api" || pathname.startsWith("/api/") || pathname === "/__qa" || pathname.startsWith("/__qa/")) {
      const apiUrl = new URL(req.url, config.apiUpstream);
      // The loopback API refuses unsafe browser requests whose Origin is not
      // loopback. After the gateway authenticates the request, rewrite Origin
      // to the local web origin; Host is rewritten by proxy().
      proxy(req, res, apiUrl, { origin: config.webUpstream.origin, secure });
      return;
    }

    proxy(req, res, new URL(req.url, config.webUpstream), { secure });
  };

  const server = config.cert && config.key
    ? https.createServer({ cert: fs.readFileSync(config.cert), key: fs.readFileSync(config.key) }, requestHandler)
    : http.createServer(requestHandler);

  server.on("upgrade", (req, socket, head) => {
    if (!isAuthenticated(req)) {
      socket.write("HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n");
      socket.destroy();
      return;
    }
    const target = config.webUpstream;
    const headers = copyHeaders(req.headers);
    headers.host = target.host;
    // copyHeaders removes hop-by-hop headers for normal requests; an upgrade
    // request must put Connection/Upgrade back or the upstream answers as plain GET.
    headers.connection = "Upgrade";
    headers.upgrade = req.headers.upgrade ?? "websocket";
    const upstream = (target.protocol === "https:" ? https : http).request({
      hostname: target.hostname,
      port: target.port || (target.protocol === "https:" ? 443 : 80),
      method: "GET",
      path: req.url,
      headers,
    });
    upstream.on("upgrade", (upstreamRes, upstreamSocket, upstreamHead) => {
      const responseLines = [`HTTP/1.1 ${upstreamRes.statusCode} ${upstreamRes.statusMessage ?? ""}`];
      for (const [name, value] of Object.entries(upstreamRes.headers)) {
        // A 101 response needs Upgrade/Connection; do not apply the normal
        // hop-by-hop filter here.
        if (value !== undefined) responseLines.push(`${name}: ${value}`);
      }
      socket.write(`${responseLines.join("\r\n")}\r\n\r\n`);
      if (upstreamHead?.length) socket.write(upstreamHead);
      if (head?.length) upstreamSocket.write(head);
      upstreamSocket.pipe(socket);
      socket.pipe(upstreamSocket);
    });
    upstream.on("error", () => socket.destroy());
    upstream.end();
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(config.port, config.host, () => {
      server.off("error", reject);
      resolve();
    });
  });

  const address = server.address();
  return {
    server,
    port: typeof address === "object" && address ? address.port : config.port,
    close: () => new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve()))),
  };
}

const invokedDirectly = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  try {
    const gateway = await startGateway();
    const scheme = process.env.SELF_TOOLS_LAN_CERT && process.env.SELF_TOOLS_LAN_KEY ? "https" : "http";
    const host = process.env.SELF_TOOLS_LAN_HOST || "0.0.0.0";
    console.log(`Self Tools LAN gateway listening on ${scheme}://${host}:${gateway.port}`);
    console.log(`Web upstream: ${process.env.SELF_TOOLS_WEB_UPSTREAM || "http://127.0.0.1:1420"}`);
    console.log(`API upstream: ${process.env.SELF_TOOLS_API_UPSTREAM || "http://127.0.0.1:8080"} (loopback only)`);
  } catch (error) {
    console.error(`LAN gateway failed to start: ${error.message}`);
    process.exitCode = 1;
  }
}
