# DevToolbox Rust Workspace

这是 PySide6 → Rust 渐进迁移的 Rust workspace 骨架。

当前仅包含：

- `crates/core`：未来承载无 UI、无 I/O 的领域规则；
- `apps/desktop`：未来的 Tauri 桌面适配器；目前仅能编译的占位二进制；
- workspace 级 Rust/Clippy lint 及 GitHub Actions 质量门禁。

本阶段不包含 Tauri、前端依赖、Markdown 业务逻辑、Python 互操作或 UI 实现。它们必须按 `docs/migration/05-migration-plan.md` 的后续 Phase 单独实施。

本地开发使用 Rust stable；workspace 的 `rust-version` 与 CI 固定为 Rust 1.95.0，防止 CI 漂移并声明最低兼容版本。

在 `rust-app/` 运行：

```powershell
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
