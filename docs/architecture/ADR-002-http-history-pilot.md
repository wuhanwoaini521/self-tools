# ADR-002 — HTTP History 只读试点（HTTP Read-only History Pilot）

- 状态：**已接受（Accepted）**
- 日期：2026-09-15
- 范围：Gate 9 / 9.5（Overnight Backend Consolidation 收敛轮）
- 关联：`CURRENT_ARCHITECTURE.md`（§9）、`ARCHITECTURE_BACKLOG.md`（§10）、ADR-001（§5.3 后续工作第 3 条）

---

## 1. Context（背景）

ADR-001 完成后，application 已可脱离 infra 编译（History / Geography / Workflows 三组端口倒置），
但整个系统仍只有 Tauri 桌面端一个组合根：所有能力只能经 `invoke` 在桌面进程内取用。

本 ADR 处理以下需求与约束：

- 需要一个**面向未来 Web/PWA 前端的 HTTP 面**（Gate 6 已把前端 `invoke` 收敛到 `transport.ts`，
  可插拔 HTTP transport）；
- 约束为**最小化试点**：只暴露 History 只读面（它是 Reference 实现，契约最稳定），
  不引入鉴权 / CORS / Web UI / 通用网关；
- 服务端必须**绝不**出现 `CommandError`（那是 Tauri 命令契约）；错误必须来自 `ApplicationError`；
- duckdb 数据文件必须**显式提供**——缺失即启动失败，不做静音 fallback（与后端惯例一致）；
- 桌面端与 server 端**零互通**（无共享 crate、无共享适配器，仅共享 core/application 契约）。

## 2. Decision（决定）

新增第 5 个 workspace member **`apps/server`**（`devtoolbox-server`，axum 0.8，纯 bin crate），
作为**第二个组合根**，只做 routing / serialization / composition / config：

1. **端点面**（只读 History，7 条 + 健康检查）：

   ```text
   GET /health                      → {"status":"ok"}
   GET /api/v1/history/home
   GET /api/v1/history/search?q=…
   GET /api/v1/history/periods/{id}
   GET /api/v1/history/events/{id}
   GET /api/v1/history/people/{id}
   GET /api/v1/history/works/{id}
   GET /api/v1/history/stories/{id}
   ```

2. **错误契约**：所有错误统一 `{"code": "...", "message": "..."}`，code 恒定 `history_error`；
   handler 错误一律经 `ApplicationError` → 500 契约；`Ok(None)` → 404；缺 `q` → 400（同契约）。
   **服务端任何路径不得产生 CommandError**。
3. **配置**（优先级 CLI > env > 默认）：
   - duckdb 路径：`--history-db` / `SELF_TOOLS_HISTORY_DB` / 默认 `history-data-pipeline/dist/history.duckdb`；
     **缺失或打不开 = 启动失败（exit 1）**；
   - 绑定：`--bind` / `SELF_TOOLS_BIND` / 默认 `127.0.0.1:8080`；LAN 暴露用 env；
   - 日志：`RUST_LOG`（默认 info）：启动 / 绑定 / DB 就绪 / 请求错误 / 关闭，不打印密钥。
4. **安全边界（最小化）**：无鉴权；不注册 CORS 中间件 → 默认无跨源访问（将来若要
   跨源，只允许显式 allowlist env）；端口固定 8080（8080/8787/3001 三选一）。
5. **适配器**：`HistoryQueryAdapter`（server 端，包 `Arc<HistoryDuckDbRepository>`）实现
   `HistoryQueryPort`（31 方法）→ `HistoryService` 挂在 axum state。与桌面端同名 adapter
   结构一致，但**不共享代码**（两侧独立，避免组合根互依赖）。
6. **优雅关闭**：`axum::serve(...).with_graceful_shutdown(...)`，监听 Ctrl+C 与 SIGTERM。
7. **测试策略**：routes 测试用 `tower::ServiceExt::oneshot`（无真实 TCP）；冒烟走真进程
   （起服 → health → home 真实数据 → 400/404 → SIGTERM → 退出码 0 → 无残留进程）。

## 2. Alternatives（备选方案）

| 方案 | 内容 | 结论 |
|---|---|---|
| A. 在 desktop crate 内加 axum | 复用现有组合根，少一个 crate | 否决 —— desktop 是 Tauri 壳，加 HTTP 服务会互相牵制生命周期与打包；<br>spec 明确要求 server/desktop 无 interdependency |
| B. 用 actix-web / hyper 裸写 | 其他服务端框架 | 否决：axum 0.8 生态成熟、tower 中间件现成，且本仓所有 async 已是 tokio |
| C. 服务端复用 desktop adapter crate | 把 `apps/desktop` 部分代码抽共享 | 否决：desktop 依赖 Tauri；共享 adapter 会重新引入桌面与 server 的耦合。独立 adapter 是显式边界 |
| D. 静音 fallback 到内置空库 | DB 缺失时自动降级 | 否决：spec 明文禁止 silent fallback；显式失败便于运维快速发现 |

## 3. Why（为什么是它）

1. **最小暴露**：只开 History 只读，契约 = 现有 `HistoryQueryPort` 的 1:1 投影，零新业务逻辑；
2. **零互通验证点**：按 spec 要求 server 与 desktop 严格零 interdependency——共享的只有三层契约
   （core / application / 同一布局），错误契约可以在两个组合根分别验证；
3. **一次投入，面友好后续**：这条路由/错误/配置/优雅关闭的模式是 travel/rss/language 只读面的模板
   （Gate 10 建议）；
4. **可验证单元**：oneshot 测试不占端口，冒烟脚本可重复执行，真实数据不落地。

## 4. Consequences（结果 & 代价）

**正向（已验证）**
- `cargo check --workspace --all-targets` 零警告；`cargo test --workspace` 全绿（含 7 个路由测试）；
- 真机冒烟：启动、`/health` `{"status":"ok"}`、`/home` 真实中文 period 数据、缺参 400、无效 id 404、
  SIGTERM 退出码 0、无残留进程；
- 缺失 duckdb 路径 → 启动即 exit 1（无 fallback，符合 spec）。

**代价 / 注意**
- 桌面端与 server 端各持一份**同构 adapter**（目前 history 31 方法 → 后续 profile 面会重复这种
  “薄样板”），这是刻意的边界成本；若未来出现第三个组合根，考虑把 adapter 抽到独立 crate；
- `HistoryQueryPort: Send + Sync`（追加 supertrait）——这曾经是隐性约束（桌面 fake 原为
  `Rc<RefCell>` 会编译失败），已在 app 测试中显式化（`Arc<Mutex>` fakes）；
- server 不承担 admin / 写面 / 鉴权，仅面向本机只读消费；写面仍是桌面 / pipeline 的职责。
- 规范化绑定 `127.0.0.1`，暴露 LAN 需要显式 `SELF_TOOLS_BIND`，不会默认对外开放。

## 5. 未来 / 后续工作

1. **Gate 10**：travel / rss / language 只读 REST（复用本 ADR 的骨架与错误契约），如需 CORS 则
   只提供显式 allowlist env；
2. **`application/src/bin/language_data.rs`** 的客户端归属（server / 独立 bin）在此框架内定案；
3. PWA 前端 `HttpTransport` 实现（Gate 6 的 transport 层已预留接口）。

（全文完）