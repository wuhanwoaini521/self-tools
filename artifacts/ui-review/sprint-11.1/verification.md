# Sprint 11.1 Release Recovery Verification

## 1. 范围与结论
Sprint 11.1 按 release recovery 执行：补齐编译/迁移/恢复证据，核对边界，并分别评估三类 RC。当前源码候选可构建，但仍是本地可变 overlay，不是可分发的正式 RC。

## 2. 候选源码状态
工作分支为 `main`。保留既有工作区修改；本轮未 reset、clean、stash、stage、commit、push、tag 或 deploy。最终 HEAD、改动清单与 SHA-256 见 `source-manifest.json`。

## 3. 架构范围
新增的旧配置迁移在 Tauri app 的启动 `setup` 阶段执行；没有新增 Tauri IPC 命令、未版本化恢复路由或前端业务逻辑。后端业务仍需通过稳定版本化 API 对客户端开放。

## 4. 平台与工具链
构建平台为 Intel macOS 12.7.6 (`x86_64`)；Node `24.22.0`、Rust/Cargo `1.95.0`。项目锁定工具用于 UI 与 Rust 验证。

## 5. 依赖与代码修复
锁定 quick-xml `0.41.0` 并处理其 `GeneralRef` 事件；应用侧 ZIP 读取器升级至已修复 advisory 的 `zip@9.1.3`，并适配其 fallible entry-name API。Clippy 发现的借用、参数过多和 recovery helper 返回表达式问题均已修复。

## 6. 严格 Clippy
`cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` 通过，exit code 0。

## 7. Rust 工作区测试
`cargo +1.95.0 test --workspace --locked` 通过，exit code 0；共 1,358 项通过、0 项失败。

## 8. Rust 格式和 diff 检查
`cargo +1.95.0 fmt --all -- --check` 与 `git diff --check` 均通过。

## 9. JMdict 实体回归
解析器在单个叶子节点内累积 Text、CDATA 与 GeneralRef，再于节点结束时提交，避免 `&amp;` 或数值引用把一段 gloss 拆成多个含义。JMdict 目标测试 5/5 通过，包含未知 POS 实体与多个 gloss/POS 标签。

## 10. 显式旧配置迁移
迁移仅在显式提供 `SELF_TOOLS_LEGACY_CONFIG` 时运行；使用 allowlist、SQLite 快照和 integrity 检查、原子 no-overwrite 安装、完成 marker，并拒绝 symlink。`settings.json`、任意用户根目录和缓存不迁移。

## 11. 隔离迁移测试
迁移仅使用临时 SQLite fixture：来源为 `/tmp/self-tools-sprint11.1-startup-fixture-20261029/legacy`，新目标为 `/tmp/self-tools-sprint11.1-startup-fixture-20261030/home`。最终 bundle 启动日志显示复制 1 个 allowlisted 项；来源与目标 `dashboard.db` 的 `PRAGMA integrity_check` 均为 `ok`，目标保留 `migration_fixture` 表中的 `isolated-sprint-11.1` 行，complete marker 显示 `copied_count=1`。

## 12. 应用内启动探针
通过最终 bundle 内 `devtoolbox-desktop` 启动，设置 `SELF_TOOLS_HOME` 与 `SELF_TOOLS_LEGACY_CONFIG` 指向隔离目录。确认迁移日志、SQLite fixture 行、integrity check 与完成 marker 后，对刚启动的进程组发送 SIGTERM 并确认退出；没有访问正式用户数据，也没有调用 AI provider。

## 13. 应用备份恢复与 API 合约
`ApplicationBackupManager` 对 allowlist 数据库生成快照，验证恢复内容并写入待执行标记；活动数据库保持不变，直到无其它实例占用时的干净启动。运行时夹具测试 4/4、server `oneshot` API 测试 4/4 通过，覆盖创建/列表/暂存、活动数据未提前改写、恢复状态、显式确认、无效 JSON 错误 envelope 与跨站 Origin 拒绝。常规恢复保留备份未列出的数据库，中断回滚会按安全快照恢复原有数据库集合。安装临时文件使用受限权限、no-overwrite 创建，并在下次独占启动时清理残留。

## 14. 生产恢复 API 与客户端范围
`apps/server` 已挂载版本化 `/api/v1/backups` 合约，使用稳定 DTO 和类型化错误状态，并沿用 loopback、Host 与 Origin 防护。恢复请求只会暂存并要求下一次干净启动；当前没有桌面端备份 UI 或 Tauri IPC 命令，本轮也没有改前端。server 是此 HTTP 合约的入口，远程 LAN/WAN 暴露仍不受支持。仓库目前没有 OpenAPI 生成或兼容性快照门禁；本轮 endpoint tests 不代表整个 API 合约审计已通过。

## 15. API 与安全边界
server 使用版本化 `/api/v1`、默认 loopback 和 Host/Origin 防护；这些检查不是 per-user authentication。旧配置迁移在启动组装阶段完成，没有绕过 HTTP API 暴露产品操作。[仓库耦合审计](../../../docs/architecture/frontend-backend-coupling-audit.md)记录的 provider secret、浏览器直连 provider、AI UI-shaped DTO 与缺少 OpenAPI 门禁问题仍未解决，本轮未修改这些边界。

## 16. 功能矩阵和发布说明
`FEATURE_MATRIX.md` 中 Geography、News、Travel、Knowledge、Readiness、全局搜索及历史浏览器验收声明已按代码与当前证据修订。历史浏览器检查未再冒充当前 release-binary 验收。

## 17. UI 构建
Tauri release 命令触发 TypeScript 与 Vite production build，成功。Vite 提示主 JavaScript chunk 约 2.67 MB，超过 500 kB 建议阈值；这是性能风险提示，不是构建失败。

## 18. UI 测试
UI 测试通过：21 个文件、125 个测试。release app 的交互式验收未完成，因为 GUI 会话处于锁定状态。

## 19. macOS app bundle
`RUSTUP_TOOLCHAIN=1.95.0 mise exec node@24.22.0 -- npm --prefix apps/desktop/ui exec -- tauri build --bundles app --no-sign` 成功，Rust release 编译约 2m 30s。bundle 位于 `target/release/bundle/macos/DevToolbox.app`，版本 `0.1.0`，identifier `com.ox-tools.devtoolbox`。

## 20. Build artifact identity
bundle 内可执行文件大小为 75,969,636 bytes，SHA-256 为 `b252efca6f7551ecea73080bd5bdbbe2363b07f4203de7ce24567601b2c9c1d8`。架构为 `x86_64`；Tauri 明确报告跳过签名。

## 21. 签名与 Gatekeeper
本机 `security find-identity -v -p codesigning` 此前显示 0 个有效 Developer ID identity；最终 bundle 的 `codesign --verify --deep --strict` 报告 `code object is not signed at all`（exit 1），`spctl --assess` 以 `no usable signature` 拒绝（exit 3）。未进行签名或 notarization。

## 22. 依赖审计
`cargo audit --no-fetch` 对更新后的锁文件报告 3 条 ZIP advisory：[RUSTSEC-2026-0367](https://rustsec.org/advisories/RUSTSEC-2026-0367)、[0368](https://rustsec.org/advisories/RUSTSEC-2026-0368)、[0369](https://rustsec.org/advisories/RUSTSEC-2026-0369)，均命中 `zip@6.0.0`（DuckDB bundled build dependency）；应用侧直接依赖已升至 `zip@9.1.3`。GitHub-reviewed [GHSA-m665-v7pm-7grj](https://github.com/advisories/GHSA-m665-v7pm-7grj)（CVE-2026-115905）影响 `zip <=9.0.4`，9.1.3 已修复；zip6 版本命中范围，但项目中未发现该问题所需的 `ZipStreamReader` 混用路径。另有 9 条 unmaintained advisory 和 `glib@0.18.5` 的 1 条 unsound notice。desktop E2E/dev npm 审计报告 23 项（21 high、2 moderate）；UI 与 desktop production-only 审计均为 0。

## 23. CORE INTERNAL RC
**NO-GO。** 恢复运行时、API 测试和隔离迁移证据均通过，但桌面 release app 没有备份 UI/IPC，也未暴露仅存在于 server 的备份 API。release binary 交互验收未完成，源码仍是可变 overlay，DuckDB 构建期 ZIP 告警也尚未处置。

## 24. PUBLIC DISTRIBUTION RC
**NO-GO。** 缺少有效 Developer ID 签名身份，当前 bundle 未签名且 Gatekeeper 拒绝；尚无 notarization 和外部分发验证。依赖审计告警也需要明确处置。

## 25. AI ENABLED RC
**NO-GO。** 没有已授权的真实 provider 配置或 live acceptance；本轮没有调用付费 provider。现有 API secret/client 边界审计问题也未达到启用 AI 的发布标准。

证据记录见 [build-evidence.json](build-evidence.json) 与 [source-manifest.json](source-manifest.json)；源码清单记录 HEAD、未暂存 diff hash、变更输入哈希及 `stagedChanges` 状态。
