# V11 发布检查单（RELEASE CHECKLIST）

发布前逐项确认。全部打勾 = 可发布到家庭 macOS Server。

## A. 构建

- [ ] `cargo build --release -p devtoolbox-server -p devtoolbox-mcp` 成功
- [ ] `npm --prefix apps/desktop/ui ci && npm --prefix apps/desktop/ui run build` 成功
- [ ] `dist/` 含 `manifest.webmanifest` / `sw.js` / `icons/`（192/512/maskable）
- [ ] `apps/desktop/ui/scripts/gen_icons.py` 生成的三个 PNG 在 `dist/icons/`

## B. 测试与质量

- [ ] `cargo test --workspace` 全绿（当前基线 1017 passed）
- [ ] `cargo check --workspace --all-targets` **0 warning**
- [ ] `npm --prefix apps/desktop/ui run build`（tsc --noEmit + vite build）PASS
- [ ] `npm --prefix apps/desktop/ui test`（vitest）PASS
- [ ] `cd history-data-pipeline && python -m src.history_data_pipeline backbone validate` OK
- [ ] 视觉 QA：`node apps/desktop/scripts/visual-qa.mjs` 全 viewport 0 overflow

## C. 安全

- [ ] `config/settings.json` **未被 git 跟踪**（`git ls-files config/` 为空）
- [ ] `config/settings.json` 权限 0600；`SELF_TOOLS_HOME` 0700
- [ ] 无 key/token 进入日志（抽查 `logs/`）
- [ ] 未把 key 写进 plist / Issue / PR / 截图
- [ ] `server.mcp.remote_enabled` = false（除非已配身份）
- [ ] `decision.mode` = `rule`（除非已配 Jev key 且 eval 通过）

## D. 配置

- [ ] `SELF_TOOLS_HOME` 目录结构存在（config/data/cache/logs/backup/runtime）
- [ ] `ai.base_url` + `ai.model` 已填（或明确接受「未配置」）
- [ ] `knowledge.file_roots` 指向存在的目录
- [ ] 启动校验通过（无 fail-closed 错误码）

## E. 网络与 PWA

- [ ] HTTPS 就绪（反向proxy + 受信任证书 / 本地 CA）
- [ ] `https://<hostname>/` 可访问（建议 `self-tools.local`）
- [ ] PWA 可安装（manifest + service worker 注册成功）
- [ ] 更新提示可用（部署新 bundle 后出现「重新加载」）
- [ ] 离线打开显示离线条，且 AI/搜索/远程 MCP 明确不可用

## F. 服务与运维

- [ ] launchd plist 已生成并 `bootstrap`
- [ ] `launchctl print` 显示运行中；重启一次确认 `kickstart -k` 生效
- [ ] 优雅退出验证：`bootout` 后日志出现完整 shutdown 序列
- [ ] 备份已跑一次；manifest.json 生成
- [ ] 恢复演练通过（隔离目标 + integrity_check）

## G. 验收

- [ ] `docs/acceptance/V11_MORNING_ACCEPTANCE.md` 的 10 分钟路线逐项过
- [ ] `docs/qa/v11/` 截图已审阅（无 overflow/clipping/塌陷）
- [ ] System Readiness 页 13 项检查符合预期
- [ ] 已知外部阻塞已在验收文档记录（Jev / LLM / OIDC / 证书 / 真机）

## H. 回滚准备

- [ ] 上一个已知良好 tag/commit 已记录
- [ ] 升级前全量备份完成
- [ ] 回滚步骤（`DEPLOY_MACOS12.md` §9）已读

---

签署：______________  日期：______________
