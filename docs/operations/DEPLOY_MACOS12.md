# macOS 12 部署（DEPLOY_MACOS12）

家庭服务器一次性部署。开发环境**不需要**执行本章（plist 安装是显式运维动作）。

## 0. 前置

- macOS 12（Monterey）或更高；**不要求 macOS 14+**（Rust 工具链见 `rust-toolchain.toml`，
  构建产物不依赖新 OS API）。
- Xcode Command Line Tools（链接器）：`xcode-select --install`
- Rust（rustup）：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Node 20 LTS（仅构建前端）：`brew install node@20`

## 1. 构建

```bash
cd /path/to/self-tools

# 后端（release）
cargo build --release -p devtoolbox-desktop -p devtoolbox-server -p devtoolbox-mcp

# 前端 bundle + PWA assets
npm --prefix apps/desktop/ui ci
npm --prefix apps/desktop/ui run build      # → apps/desktop/ui/dist/
```

产物：

| 项 | 路径 |
| --- | --- |
| backend binary | `target/release/devtoolbox-server`（HTTP 服务）/ `devtoolbox-mcp`（MCP） |
| frontend bundle | `apps/desktop/ui/dist/`（静态，含 `manifest.webmanifest` / `sw.js` / `icons/`） |
| config template | `config/settings.json`（见 §3） |
| launchd template | `self-tools --launchd generate`（见 §5） |
| migrations | 各 SQLite store 的 `migrate()`（启动时幂等执行） |

## 2. 目录

```bash
export SELF_TOOLS_HOME="$HOME/Library/Application Support/self-tools"
mkdir -p "$SELF_TOOLS_HOME"/{config,data,cache,logs,backup,runtime}
chmod 700 "$SELF_TOOLS_HOME" "$SELF_TOOLS_HOME/config"
```

`SELF_TOOLS_HOME` 未设置时按平台默认推导（macOS：Application Support/self-tools）。

## 3. 配置与 Secret

```bash
cp config/settings.json.example "$SELF_TOOLS_HOME/config/settings.json"
chmod 600 "$SELF_TOOLS_HOME/config/settings.json"
$EDITOR "$SELF_TOOLS_HOME/config/settings.json"
```

必填/可选项：

```jsonc
{
  "ai":    { "base_url": "https://api.deepseek.com/v1", "model": "deepseek-chat",
             "api_key": "<LLM key；本地 Ollama 可留空>", "timeout_secs": 120 },
  "decision": { "mode": "rule", "jev_api_key": "<Jev key；不配则纯规则>" },
  "knowledge": { "file_roots": [ { "id": "docs", "label": "资料", "path": "/Users/you/Documents" } ] },
  "server": { "mcp": { "enabled": true, "http_enabled": false, "remote_enabled": false } }
}
```

Secret **只**进这个文件（0600、gitignored）。**不要**把 key 写进 plist、shell 历史、
Issue/PR、日志或截图。

启动校验：非法端口 / 非 loopback 生产绑定 / 不可写目录 / 越界阈值 /
远程 MCP 无身份 / 文件根不存在 → **拒绝启动**并在 stderr 打印稳定错误码。

## 4. HTTPS（家庭手机/iPad 必需）

`http://192.168.x.x:port` **不是**可接受的最终方案（PWA 需要安全上下文）。
任选其一：

| 方案 | 说明 |
| --- | --- |
| 反向代理（推荐） | 路由器/网关注入；或本机 Caddy/nginx 终止 TLS，`self-tools` 只监听 `127.0.0.1:8080` |
| 受信任证书 | 已有域名 + ACME（Let's Encrypt）证书；`fullchain.pem` + `key.pem` 给代理 |
| 本地 CA | 家庭内私有 CA 签发的证书，逐设备安装根证书（iPad/iPhone 均需「设置 → 关于 → 证书信任」） |

代理最小配置（Caddy 反代 + 本地 CA 示例思路）：

```caddyfile
self-tools.local {
  tls internal
  reverse_proxy 127.0.0.1:8080
}
```

**不要**在产品代码里硬编码某个产品名；由部署方选择（§78）。

## 5. Hostname

提供稳定访问名，避免日常记 IP:端口：

- mDNS/Bonjour：macOS 12 自带；局域网内 `self-tools.local`（或服务器实际 hostname）。
- 路由器 DHCP 里给服务器固定 IP + hostname 绑定。
- 最终 PWA 安装地址 = `https://self-tools.local/`（以实际部署为准）。

## 6. launchd

```bash
# 生成（打印到 stdout；默认 ~/Library/Logs/self-tools 作日志目录）
target/release/devtoolbox-server --launchd generate \
  | sed "s|/var/log/self-tools|$SELF_TOOLS_HOME/logs|g" \
  > ~/Library/LaunchAgents/ai.self-tools.server.plist

launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/ai.self-tools.server.plist
launchctl enable gui/$(id -u)/ai.self-tools.server
```

管理命令：

```bash
launchctl kickstart -k gui/$(id -u)/ai.self-tools.server   # 重启
launchctl bootout  gui/$(id -u)/ai.self-tools.server       # 停止 + 注销
launchctl print gui/$(id -u)/ai.self-tools.server          # 状态
```

plist 关键点（`render_launchd_plist` 生成）：绝对可执行路径、显式
`SELF_TOOLS_HOME`、`EnvironmentVariables` 引用 env 文件、`KeepAlive.SuccessfulExit=false`
（正常退出不重启）、`ThrottleInterval=10`（崩溃重启节流）、`RunAtLoad=true`。

## 7. 启动 / 停止 / 状态

```bash
# 状态：进程 + 健康
curl -fsS https://self-tools.local/api/health | jq .

# 手动前台运行（排障）
SELF_TOOLS_HOME="$SELF_TOOLS_HOME" RUST_LOG=info target/release/devtoolbox-server --mode production
```

日志：`$SELF_TOOLS_HOME/logs/`（结构化 JSONL；`RUST_LOG` 控制级别）。
**禁止**记录完整 prompt / 文档 / Memory / 文件内容 / key / token。

## 8. 升级

```bash
cd /path/to/self-tools && git pull --ff-only
cargo build --release -p devtoolbox-server
npm --prefix apps/desktop/ui ci && npm --prefix apps/desktop/ui run build
launchctl kickstart -k gui/$(id -u)/ai.self-tools.server
```

前端带版本兼容提示（`versionNotice`）：后端版本过旧 → 升级横幅。

## 9. 回滚

```bash
cd /path/to/self-tools && git checkout <last-known-good-tag>
cargo build --release -p devtoolbox-server
npm --prefix apps/desktop/ui ci && npm --prefix apps/desktop/ui run build
launchctl kickstart -k gui/$(id -u)/ai.self-tools.server
```

数据回滚见 `BACKUP_RESTORE.md`（**先备份再回滚**）。

## 10. 验收

- `curl https://self-tools.local/api/health` → `liveness: alive`
- 打开 `https://self-tools.local/` → Personal Hub Home
- 安装 PWA（见 `V11_MORNING_ACCEPTANCE.md`）
- `docs/acceptance/V11_MORNING_ACCEPTANCE.md` 的 10 分钟路线逐项过
