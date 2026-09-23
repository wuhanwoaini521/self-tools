//! Study Board 模块测试（V11 §107-§114）。
//!
//! 全部用内存 Fake 存储端口驱动（不触真实 SQLite / 文件系统）：
//! - 模块面：descriptor id = `study-board`、4 个工具、Read/SafeWrite 风险；
//! - 列表/读取只回有界摘要、**不回传笔迹坐标**（§109/§111 隐私铁律）；
//! - `save` 幂等 upsert；新板必须有 title；注入形态的 board_id 被拒；
//! - `snapshot` 只登记引用、不生成图像；
//! - ContextProvider 对其他模块返回 None / 对 study-board 返回当前板标题。
#![allow(
    clippy::field_reassign_with_default,
    clippy::unnecessary_sort_by,
    clippy::drop_non_drop,
    clippy::uninlined_format_args
)]

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::ToolRisk;
use devtoolbox_core::personal_ai::{AppContext, EntityRef};
use devtoolbox_core::study_board::{StudyBoard, StudyBoardSnapshot, StudyBoardSummary};
use serde_json::json;

use crate::error::ApplicationError;
use crate::personal_ai::context::{ContextBudget, ModuleContextProvider};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};
use crate::personal_ai::study_board::{
    STUDY_BOARD_MODULE_ID, StudyBoardProviderOwned, StudyBoardTools, register_study_board,
    study_board_tool_names,
};
use crate::study_board::ports::{StudyBoardStoreError, StudyBoardStorePort};

// ---------------------------------------------------------------------------
// Fake 存储（内存；可控失败注入）
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeStore {
    boards: Mutex<HashMap<String, StudyBoard>>,
    snapshots: Mutex<HashMap<String, StudyBoardSnapshot>>,
    /// 注入失败的工具面（None = 正常）。
    fail: Mutex<Option<String>>,
}

impl FakeStore {
    /// 预置一块板（测试直插，不经工具路径）。
    fn seed(&self, board: StudyBoard) {
        self.boards.lock().insert(board.id.clone(), board);
    }

    /// 注入某个 reason 的存储失败（内部可变；`Arc` 共享下仍可写）。
    fn inject_failure(&self, reason: &str) {
        *self.fail.lock() = Some(reason.to_string());
    }

    fn check(&self, reason: &str) -> Result<(), StudyBoardStoreError> {
        match self.fail.lock().as_deref() {
            Some(injected) if injected == reason => {
                Err(StudyBoardStoreError(format!("boom:{reason}")))
            }
            _ => Ok(()),
        }
    }
}

impl StudyBoardStorePort for FakeStore {
    fn upsert_board(&self, board: &StudyBoard) -> Result<(), StudyBoardStoreError> {
        self.check("board_save_failed")?;
        self.boards.lock().insert(board.id.clone(), board.clone());
        Ok(())
    }

    fn get_board(&self, id: &str) -> Result<Option<StudyBoard>, StudyBoardStoreError> {
        self.check("board_read_failed")?;
        Ok(self.boards.lock().get(id).cloned())
    }

    fn list_boards(&self, limit: usize) -> Result<Vec<StudyBoardSummary>, StudyBoardStoreError> {
        self.check("board_list_failed")?;
        let boards = self.boards.lock();
        let mut items: Vec<StudyBoardSummary> = boards.values().map(StudyBoard::summary).collect();
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(items.into_iter().take(limit).collect())
    }

    fn upsert_snapshot(&self, snapshot: &StudyBoardSnapshot) -> Result<(), StudyBoardStoreError> {
        self.check("snapshot_save_failed")?;
        self.snapshots
            .lock()
            .insert(snapshot.id.clone(), snapshot.clone());
        Ok(())
    }

    fn get_snapshot(&self, id: &str) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError> {
        Ok(self.snapshots.lock().get(id).cloned())
    }

    fn latest_snapshot(
        &self,
        board_id: &str,
    ) -> Result<Option<StudyBoardSnapshot>, StudyBoardStoreError> {
        let snapshots = self.snapshots.lock();
        Ok(snapshots
            .values()
            .filter(|snapshot| snapshot.board_id == board_id)
            .max_by_key(|snapshot| snapshot.created_at)
            .cloned())
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct Hub {
    tools: ToolRegistry,
    modules: ModuleRegistry,
    store: Arc<FakeStore>,
}

fn hub() -> Hub {
    let store = Arc::new(FakeStore::default());
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_study_board(&mut modules, &mut tools, store.clone()).expect("register study board");
    Hub {
        tools,
        modules,
        store,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

/// 走完整注册表路径执行（含 schema 校验），返回 ToolResult。
fn call(hub: &Hub, name: &str, arguments: serde_json::Value) -> devtoolbox_core::ToolResult {
    block_on(hub.tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: name.into(),
        arguments,
    }))
    .expect("tool call")
}

/// 期望执行失败（参数错误 / 注入形态 / 存储失败）。
fn call_err(hub: &Hub, name: &str, arguments: serde_json::Value) -> devtoolbox_core::AgentError {
    block_on(hub.tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: name.into(),
        arguments,
    }))
    .expect_err("tool call should fail")
}

fn board(id: &str, title: &str, updated_at: i64) -> StudyBoard {
    StudyBoard::new(
        id,
        title,
        json!({"strokes": [{"points": [[0, 0], [9, 9]], "color": "#fff"}]}),
        1_000,
        "history",
    )
    .with_updated_at(updated_at)
}

fn board_context(id: &str) -> AppContext {
    AppContext {
        module: Some(STUDY_BOARD_MODULE_ID.into()),
        page: Some("board-detail".into()),
        entity: Some(EntityRef {
            kind: "board".into(),
            id: id.into(),
            label: None,
        }),
        ..AppContext::default()
    }
}

// ---------------------------------------------------------------------------
// 模块面（§12/§106/§108）
// ---------------------------------------------------------------------------

#[test]
fn registers_descriptor_and_four_tools_with_expected_risks() {
    let hub = hub();
    let descriptors = hub.modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "study-board");
    assert_eq!(descriptors[0].display_name, "学习板");
    assert_eq!(
        descriptors[0].capabilities,
        vec!["board".to_string(), "snapshot".to_string()]
    );
    assert_eq!(descriptors[0].tools.len(), 4);
    assert!(hub.modules.context_provider("study-board").is_some());

    let mut names: Vec<String> = hub
        .tools
        .specs()
        .iter()
        .map(|spec| spec.name.clone())
        .collect();
    names.sort();
    let mut expected: Vec<String> = study_board_tool_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected.sort();
    assert_eq!(names, expected);
    // 工具名必须是 module.action 形态（注册表强制，这里显式断言契约）。
    for name in &names {
        assert!(name.starts_with("study-board."), "{name}");
    }

    let specs = hub.tools.specs();
    let risks: Vec<(&str, ToolRisk)> = specs
        .iter()
        .map(|spec| (spec.name.as_str(), spec.risk))
        .collect();
    assert_eq!(
        risks,
        vec![
            ("study-board.get", ToolRisk::Read),
            ("study-board.list", ToolRisk::Read),
            ("study-board.save", ToolRisk::SafeWrite),
            ("study-board.snapshot", ToolRisk::Read),
        ]
    );
    for spec in &specs {
        assert_eq!(spec.module, "study-board", "{}", spec.name);
    }
}

#[test]
fn module_id_constant_matches_descriptor_and_tools() {
    let hub = hub();
    assert_eq!(STUDY_BOARD_MODULE_ID, "study-board");
    let provider = StudyBoardProviderOwned {
        tools: Arc::new(StudyBoardTools::new(hub.store.clone())),
    };
    assert_eq!(provider.module_id(), "study-board");
}

#[test]
fn no_write_or_draw_capability_is_exposed() {
    let hub = hub();
    // 后端永不执行绘画/删除/执行命令（§108）。
    for spec in hub.tools.specs() {
        for forbidden in [
            "draw", "render", "delete", "remove", "exec", "shell", "command",
        ] {
            assert!(
                !spec.name.contains(forbidden),
                "禁止暴露 {forbidden} 能力: {}",
                spec.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// list / get（Read；隐私铁律）
// ---------------------------------------------------------------------------

#[test]
fn list_returns_metadata_without_stroke_geometry() {
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));
    hub.store.seed(board("b-2", "岳阳楼记", 2_000));

    let result = call(&hub, "study-board.list", json!({"limit": 10}));
    assert!(result.ok);
    assert_eq!(result.data["count"], 2);
    assert_eq!(result.data["items"][0]["id"], "b-2", "按更新时间倒序");
    let rendered = serde_json::to_string(&result.data).unwrap();
    assert!(!rendered.contains("points"), "列表不得回传笔迹坐标");
    assert!(!rendered.contains("0, 0"));
    // UI block 来自同一份不含量坐标的数据。
    let blocks = result.metadata["ui_hint"]["ui_blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["kind"], "entity_list");
}

#[test]
fn get_returns_metadata_and_bounded_summary_only() {
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));
    hub.store.snapshots.lock().insert(
        "snap-seed".into(),
        StudyBoardSnapshot::new("snap-seed", "b-1", "t", None, "1 条矢量笔画", 5_000),
    );

    let result = call(&hub, "study-board.get", json!({"board_id": "b-1"}));
    assert!(result.ok);
    assert_eq!(result.data["title"], "遵义会议");
    assert_eq!(result.data["module_origin"], "history");
    assert_eq!(result.data["stroke_count"], 1);
    assert_eq!(result.data["latest_snapshot_id"], "snap-seed");
    // 有界摘要，不含坐标。
    let summary = result.data["strokes_summary"].as_str().unwrap();
    assert_eq!(summary, "1 条矢量笔画（坐标由前端渲染，不在此展示）");
    let rendered = serde_json::to_string(&result.data).unwrap();
    assert!(!rendered.contains("points"));
    assert!(!rendered.contains("9, 9"));
}

#[test]
fn get_missing_board_is_controlled_failure_not_panic() {
    let hub = hub();
    let result = call(&hub, "study-board.get", json!({"board_id": "b-missing"}));
    assert!(!result.ok);
    let error = result.error.expect("error text");
    assert!(error.contains("不存在"), "{error}");
}

#[test]
fn get_with_injection_shaped_board_id_is_rejected() {
    let hub = hub();
    let error = call_err(
        &hub,
        "study-board.get",
        json!({"board_id": "b-1; DROP TABLE boards"}),
    );
    assert_eq!(error.code(), "personal_ai_tool_invalid_argument");
    // schema 缺必填参数同样被前置校验拒绝（不进执行器）。
    let missing = call_err(&hub, "study-board.get", json!({}));
    assert_eq!(missing.code(), "personal_ai_tool_invalid_argument");
}

#[test]
fn storage_failure_is_controlled_error_without_payload() {
    let hub = hub();
    hub.store.inject_failure("board_read_failed");
    let error = call_err(&hub, "study-board.get", json!({"board_id": "b-1"}));
    let rendered = error.to_string();
    assert!(rendered.contains("board_read_failed"), "{rendered}");
    // 错误不带笔迹正文（端口本身也不返回正文）。
    assert!(!rendered.contains("points"));
}

// ---------------------------------------------------------------------------
// save（SafeWrite；幂等）
// ---------------------------------------------------------------------------

#[test]
fn save_creates_then_updates_idempotently() {
    let hub = hub();
    let created = call(
        &hub,
        "study-board.save",
        json!({
            "board_id": "b-1",
            "title": "新板",
            "strokes": {"strokes": [{"points": [[1, 2]]}]},
            "module_origin": "history",
        }),
    );
    assert!(created.ok);
    assert_eq!(created.data["created"], true);
    assert_eq!(created.data["title"], "新板");
    assert_eq!(created.data["stroke_count"], 1);

    let updated = call(
        &hub,
        "study-board.save",
        json!({"board_id": "b-1", "title": "改名了"}),
    );
    assert!(updated.ok);
    assert_eq!(updated.data["created"], false);
    assert_eq!(updated.data["stroke_count"], 1, "不改 strokes 时保留");
    let stored = hub.store.boards.lock();
    assert_eq!(stored.len(), 1, "按 id upsert，不新增行");
    assert_eq!(stored["b-1"].title, "改名了");
    assert_eq!(stored["b-1"].module_origin, "history", "更新保留来源");
}

#[test]
fn save_rejects_new_board_without_title_and_invalid_payloads() {
    let hub = hub();
    // 新板必须带标题。
    let error = call_err(
        &hub,
        "study-board.save",
        json!({"board_id": "b-new", "strokes": {"strokes": []}}),
    );
    assert!(error.to_string().contains("必须提供 title"), "{error}");

    // 既不给 title 也不给 strokes。
    let empty = call_err(&hub, "study-board.save", json!({"board_id": "b-1"}));
    assert!(empty.to_string().contains("至少提供"), "{empty}");

    // 超长标题。
    let long = call_err(
        &hub,
        "study-board.save",
        json!({"board_id": "b-1", "title": "板".repeat(201)}),
    );
    assert!(long.to_string().contains("超过"), "{long}");

    // 注入形态 id。
    let injected = call_err(
        &hub,
        "study-board.save",
        json!({"board_id": "../etc/passwd", "title": "x"}),
    );
    assert_eq!(injected.code(), "personal_ai_tool_invalid_argument");
}

#[test]
fn save_storage_failure_is_controlled() {
    let hub = hub();
    hub.store.inject_failure("board_save_failed");
    let error = call_err(
        &hub,
        "study-board.save",
        json!({"board_id": "b-1", "title": "t"}),
    );
    assert!(error.to_string().contains("board_save_failed"), "{error}");
}

// ---------------------------------------------------------------------------
// snapshot（Read；只登记引用）
// ---------------------------------------------------------------------------

#[test]
fn snapshot_registers_reference_without_generating_image() {
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));

    // 不带 PNG：只登记摘要。
    let result = call(&hub, "study-board.snapshot", json!({"board_id": "b-1"}));
    assert!(result.ok);
    assert_eq!(result.data["kind"], "board_snapshot");
    assert_eq!(result.data["board_id"], "b-1");
    assert_eq!(result.data["has_png"], false);
    let snapshot_id = result.data["snapshot_id"].as_str().unwrap();
    assert!(snapshot_id.starts_with("snap-"), "{snapshot_id}");
    let stored = hub.store.snapshots.lock();
    assert_eq!(stored.len(), 1);
    assert!(stored[snapshot_id].png_base64.is_none());

    // 带 PNG：登记前端渲染结果（backend 不生成）。
    drop(stored);
    let with_png = call(
        &hub,
        "study-board.snapshot",
        json!({"board_id": "b-1", "png_base64": "iVBORw0KGgo="}),
    );
    assert!(with_png.ok);
    assert_eq!(with_png.data["has_png"], true);
    let stored = hub.store.snapshots.lock();
    assert_eq!(stored.len(), 2);
}

#[test]
fn snapshot_for_missing_board_is_controlled_failure() {
    let hub = hub();
    let result = call(
        &hub,
        "study-board.snapshot",
        json!({"board_id": "b-missing"}),
    );
    assert!(!result.ok);
    assert!(result.error.unwrap().contains("不存在"));
}

#[test]
fn snapshot_storage_failure_is_controlled() {
    let hub = hub();
    hub.store.seed(board("b-1", "t", 1));
    hub.store.inject_failure("snapshot_save_failed");
    let error = call_err(&hub, "study-board.snapshot", json!({"board_id": "b-1"}));
    assert!(
        error.to_string().contains("snapshot_save_failed"),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// ContextProvider（§13/§83）
// ---------------------------------------------------------------------------

#[test]
fn context_provider_returns_current_board_for_study_board_module() {
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));
    let provider = StudyBoardProviderOwned {
        tools: Arc::new(StudyBoardTools::new(hub.store.clone())),
    };

    let bundle = provider
        .build_context(&board_context("b-1"), &ContextBudget::default())
        .expect("context");
    assert_eq!(bundle.module, "study-board");
    assert!(bundle.headline.contains("遵义会议"), "{}", bundle.headline);
    assert_eq!(bundle.summary["title"], "遵义会议");
    assert_eq!(bundle.summary["entity"]["id"], "b-1");
    assert_eq!(bundle.summary["page"], "board-detail");
    // 上下文不含笔迹坐标（§13）。
    let rendered = serde_json::to_string(&bundle.summary).unwrap();
    assert!(!rendered.contains("points"));
    assert!(!rendered.contains("9, 9"));

    // 未打开具体板 → 总览摘要，不报错。
    let overview = provider
        .build_context(&AppContext::default(), &ContextBudget::default())
        .expect("overview context");
    assert_eq!(overview.module, "study-board");
    assert!(
        overview.summary["note"]
            .as_str()
            .unwrap()
            .contains("study-board.list")
    );

    // 上下文里的板不存在 → 摘要提示去 list，不 panic。
    let unknown = provider
        .build_context(&board_context("b-missing"), &ContextBudget::default())
        .expect("unknown board context");
    assert!(unknown.headline.contains("未找到"), "{}", unknown.headline);
}

#[test]
fn context_provider_ignores_other_module_entities() {
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));
    let provider = StudyBoardProviderOwned {
        tools: Arc::new(StudyBoardTools::new(hub.store.clone())),
    };
    // 其他模块 / 其他实体类型：不当作学习板解析（返回 None 语义由注册表保证，
    // 这里验证 provider 不会把 history 实体误判为板）。
    let history = AppContext {
        module: Some("history".into()),
        entity: Some(EntityRef {
            kind: "person".into(),
            id: "b-1".into(),
            label: None,
        }),
        ..AppContext::default()
    };
    let bundle = provider
        .build_context(&history, &ContextBudget::default())
        .expect("context");
    assert_eq!(bundle.summary["title"], serde_json::Value::Null);
    assert!(bundle.headline == "学习板");

    // study-board 模块但没有 entity / entity 不是 board。
    let wrong_kind = AppContext {
        module: Some(STUDY_BOARD_MODULE_ID.into()),
        entity: Some(EntityRef {
            kind: "stroke".into(),
            id: "b-1".into(),
            label: None,
        }),
        ..AppContext::default()
    };
    let bundle = provider
        .build_context(&wrong_kind, &ContextBudget::default())
        .expect("context");
    assert!(bundle.summary["title"].is_null(), "非 board 实体不算板");
}

#[test]
fn context_provider_does_not_leak_private_data_for_other_modules() {
    // 其他模块的 AppContext 经过 study-board provider 时不得回传笔迹正文。
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));
    let provider = StudyBoardProviderOwned {
        tools: Arc::new(StudyBoardTools::new(hub.store.clone())),
    };
    let other = AppContext {
        module: Some("travel".into()),
        page: Some("trip".into()),
        ..AppContext::default()
    };
    let bundle = provider
        .build_context(&other, &ContextBudget::default())
        .expect("context");
    let rendered = serde_json::to_string(&bundle.summary).unwrap();
    assert!(!rendered.contains("points"));
    assert!(!rendered.contains("遵义会议"));
}

// ---------------------------------------------------------------------------
// 隐私铁律（§113/§114）
// ---------------------------------------------------------------------------

#[test]
fn tools_never_write_to_personal_memory() {
    // 端口面不含任何 memory 能力：模块只依赖 StudyBoardStorePort。
    let hub = hub();
    hub.store.seed(board("b-1", "遵义会议", 1_000));
    for (tool, arguments) in [
        ("study-board.list", json!({})),
        ("study-board.get", json!({"board_id": "b-1"})),
        ("study-board.save", json!({"board_id": "b-1", "title": "x"})),
        ("study-board.snapshot", json!({"board_id": "b-1"})),
    ] {
        let result = call(&hub, tool, arguments);
        assert!(result.ok, "{tool}");
        let rendered = serde_json::to_string(&result).unwrap();
        assert!(
            !rendered.contains("memory"),
            "{tool} 结果不得涉及 memory 语义"
        );
        assert!(!rendered.contains("points"), "{tool} 不得回传笔迹坐标");
    }
}

#[test]
fn application_error_variant_for_store_failure_is_reused() {
    // 复用既有最接近的变体（不新增 ApplicationError 变体）：Infrastructure。
    let error = crate::personal_ai::args::tool_error(ApplicationError::Infrastructure {
        path: std::path::PathBuf::from("study_board"),
        message: "board_read_failed: boom".into(),
    });
    assert!(error.to_string().contains("board_read_failed"), "{error}");
}
