# 排障（TROUBLESHOOTING V11）

按症状查。所有命令假定仓库根目录；生产加 `SELF_TOOLS_HOME=...`。

## 1. 启动失败

| stderr | 原因 | 处理 |
| --- | --- | --- |
| `configuration error: invalid bind address` | `--bind` / `SELF_TOOLS_BIND` 不是 `host:port` | 用 `127.0.0.1:8080` |
| `configuration error: unknown argument: X` | CLI 参数拼错 | `--help` |
| `invalid_port` | 端口 0 或非数字 | 1..=65535 |
| `unsafe_bind` | 生产模式绑定非 loopback 未显式允许 | 设 `SELF_TOOLS_ALLOW_REMOTE=1`（确认已配 HTTPS+身份）或改回 loopback |
| `invalid_path` / `missing_required_dir` | data/config 目录不可创建或不可写 | 修权限；`mkdir -p`；检查磁盘 |
| `remote_mcp_without_auth` | `server.mcp.remote_enabled=true` 但无身份提供者 | 关远程，或配置 OIDC |
| `bad_threshold`（confirmation_ttl / ratios / jev_timeout） | 数值越界 | TTL 30..=120；比率 0..=1；timeout 1..=60 |
| `history database unavailable` | `SELF_TOOLS_HISTORY_DB` 指向不存在的 duckdb | 跑 `history-data-pipeline` build，或指到正确 dist |
| `instance lock held: <path>` | 已有实例在跑 / stale lock | `ps` 确认；删 `runtime/instance.lock`（确认无实例） |

## 2. AI 相关

| 症状 | 原因 | 处理 |
| --- | --- | --- |
| 「AI provider 未配置」 | `settings.json` 无 `ai` 段 | 填 `ai.base_url + ai.model`（+ `api_key`） |
| `personal_ai_model_unavailable` | base/model 空、网络不可达、key 错 | 看 `RUST_LOG=debug` 的 provider 错误；本地 Ollama 用 `http://localhost:11434/v1` |
| `personal_ai_provider_timeout` | 模型慢于 `ai.timeout_secs` | 调大超时；换更快模型 |
| 发图片被拒「当前模型不支持图片」 | 模型 vision=false | 换 vision 模型，或用文字描述 |
| `personal_ai_max_tool_rounds` | 工具循环超 4 轮 | 简化请求；检查工具是否返回误导内容 |

## 3. 决策 / 编排

| 症状 | 原因 | 处理 |
| --- | --- | --- |
| Trace 显示 `Provider: rule` 且「已回落到规则」 | Jev 未配/超时/低置信/限流 | 预期行为；配 key + `mode=jev_shadow` 观察差异 |
| 请求没编排（decision=direct） | 规则判定简单 / 预算档 None / 用户说「不要用多 agent」 | 用「深入研究/全面比较」或去掉关闭词 |
| Trace 无决策字段 | 未装配决策引擎 | 组合根 `build_decision_engine`（desktop 已默认装） |
| worker 失败 | 预算耗尽 / 超时 / capability 不匹配 | 看 `OrchestrationRunView.error_code` |

## 4. 搜索 / 数据

| 症状 | 原因 | 处理 |
| --- | --- | --- |
| Global Search 全降级 | 未注册任何 `GlobalSearchPort` | 组合根注册各模块端口 |
| Documents/Files 显示「未配置允许目录」 | `knowledge.file_roots` 空 | 加根（路径必须存在） |
| `documents_error` / `files_error` | 索引/读取失败（文件被移走、权限） | 看 message；重新同步 |
| `language_error` | language.db 缺数据 | Starter Pack / `language-data import` |

## 5. 服务器 / SafeAction

| 症状 | 原因 | 处理 |
| --- | --- | --- |
| `services.restart` 只返回 confirmation_required | **正确**（工具不执行，只签票） | 走确认 UI / `confirm_action` |
| 确认过期 | TTL 60s 内未确认 | 重新请求 |
| 冷却中 | 同目标 `cooldown_secs` 内重复 | 等待 |
| `server error (service_control_not_configured)` | 平台控制未装配 | 配置 ServiceControlPort |

## 6. PWA / 前端

| 症状 | 原因 | 处理 |
| --- | --- | --- |
| 装不了 PWA | 非 HTTPS / 非 localhost | 上 HTTPS（`DEPLOY_MACOS12.md` §4） |
| 没有更新提示 | SW 未注册 / 已最新 | Readiness 看 `pwa_secure_context`；硬刷新 |
| 离线后 AI 不可用 | 预期（敏感路径 network-only） | 离线只保证 App Shell |
| 布局像桌面（手机上） | `data-device` 未更新 / 视口被缩放 | 刷新；检查 `html` 无 `min-width` 覆盖 |
| React #321「Invalid hook call」 | 组件里 hook 嵌套/条件调用 | 把 hook 提到组件顶层（V11 修过 LanguagePage 一处） |

## 7. 备份 / 恢复

| 症状 | 原因 | 处理 |
| --- | --- | --- |
| `backup` 目标写不进去 | 目录不存在/不可写 | BackupService 会 create_dir_all；仍失败 → 权限/磁盘 |
| restore 校验失败 | 快照被篡改/截断 | 从旧备份恢复；检查磁盘 |
| restore 拒绝路径 | manifest 含 `../` 或绝对路径 | 只信自产 manifest |
| SQLite `SQLITE_BUSY` | 两进程共开同一库 | 单实例锁；别让 MCP 指向桌面在用目录 |

## 8. 日志与诊断

```bash
RUST_LOG=debug target/release/devtoolbox-server --mode production 2>&1 | tee run.log
```

- 结构化字段：component / request_id / trace_id / event / duration_ms / result。
- **不要**把日志贴到 Issue：可能含用户路径；先过 `redact_log_value` 规则自查。
- 浏览器侧：DevTools Console + Application → Service Workers（更新/卸载）。

## 9. 一键自检

```bash
cargo test --workspace
cargo check --workspace --all-targets
npm --prefix apps/desktop/ui run build
python -m src.history_data_pipeline backbone validate   # history-data-pipeline/
curl -fsS http://127.0.0.1:8080/api/health | jq .       # 后端在跑时
```
