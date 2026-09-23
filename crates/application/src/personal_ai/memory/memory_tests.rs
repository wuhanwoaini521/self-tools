//! Memory 模块测试（V6 §95/§113）：工具面 + agent 路由 + 写入 gate。
//!
//! 用内存 Fake 存储（与 `crate::memory::tests` 同款语义），不触真实 SQLite。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::memory::{MemoryCategory, MemoryItem, MemoryQuery, MemoryStatus};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::{ActionKind, ToolRisk, UiBlockKind};
use serde_json::json;

use crate::memory::ports::{MemoryStoreError, MemoryStorePort};
use crate::memory::service::MemoryService;
use crate::personal_ai::context::ContextBudget;
use crate::personal_ai::memory::{memory_tool_names, register_memory};
use crate::personal_ai::registry::{ModuleRegistry, ToolRegistry};

#[derive(Default)]
struct FakeStore {
    items: Mutex<HashMap<String, MemoryItem>>,
}

impl MemoryStorePort for FakeStore {
    fn upsert(&self, item: &MemoryItem) -> Result<(), MemoryStoreError> {
        self.items.lock().insert(item.id.clone(), item.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<MemoryItem>, MemoryStoreError> {
        Ok(self.items.lock().get(id).cloned())
    }

    fn query(&self, spec: &MemoryQuery) -> Result<Vec<MemoryItem>, MemoryStoreError> {
        let tokens: Vec<String> = spec
            .query
            .split_whitespace()
            .map(|token| token.to_lowercase())
            .collect();
        let mut items: Vec<MemoryItem> = self
            .items
            .lock()
            .values()
            .filter(|item| spec.include_sensitive || item.sensitivity.is_model_visible())
            .filter(|item| {
                spec.category
                    .is_none_or(|category| item.category == category)
            })
            .filter(|item| spec.status.is_none_or(|status| item.status == status))
            .filter(|item| {
                tokens.is_empty()
                    || tokens
                        .iter()
                        .any(|token| item.content.to_lowercase().contains(token.as_str()))
            })
            .cloned()
            .collect();
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then(left.id.cmp(&right.id))
        });
        if spec.limit > 0 {
            items.truncate(spec.limit);
        }
        Ok(items)
    }

    fn touch_used(&self, ids: &[String], now: i64) -> Result<(), MemoryStoreError> {
        let mut items = self.items.lock();
        for id in ids {
            if let Some(item) = items.get_mut(id) {
                item.last_used_at = Some(now);
            }
        }
        Ok(())
    }

    fn count_by_status(&self) -> Result<Vec<(MemoryStatus, usize)>, MemoryStoreError> {
        Ok(MemoryStatus::ALL
            .into_iter()
            .filter_map(|status| {
                let count = self
                    .items
                    .lock()
                    .values()
                    .filter(|item| item.status == status)
                    .count();
                (count > 0).then_some((status, count))
            })
            .collect())
    }

    fn count_by_category(&self) -> Result<Vec<(MemoryCategory, usize)>, MemoryStoreError> {
        Ok(MemoryCategory::ALL
            .into_iter()
            .filter_map(|category| {
                let count = self
                    .items
                    .lock()
                    .values()
                    .filter(|item| item.category == category)
                    .count();
                (count > 0).then_some((category, count))
            })
            .collect())
    }
}

fn hub() -> (Arc<MemoryService>, ToolRegistry, ModuleRegistry) {
    let service = Arc::new(MemoryService::new(Arc::new(FakeStore::default())));
    let mut modules = ModuleRegistry::new();
    let mut tools = ToolRegistry::new();
    register_memory(&mut modules, &mut tools, Arc::clone(&service))
        .expect("register memory module");
    (service, tools, modules)
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

#[test]
fn registers_descriptor_and_five_tools_with_expected_risks() {
    let (_service, tools, modules) = hub();
    let descriptors = modules.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id, "memory");
    assert_eq!(descriptors[0].tools.len(), 5);

    let mut names: Vec<String> = tools.specs().iter().map(|spec| spec.name.clone()).collect();
    names.sort();
    let mut expected: Vec<String> = memory_tool_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    expected.sort();
    assert_eq!(names, expected);

    for spec in tools.specs() {
        let expected = if spec.name == "memory.save" || spec.name == "memory.archive" {
            ToolRisk::SafeWrite
        } else {
            ToolRisk::Read
        };
        assert_eq!(spec.risk, expected, "{}", spec.name);
        assert_eq!(spec.module, "memory");
        assert!(!spec.description.is_empty());
    }
    assert!(modules.context_provider("memory").is_some());
}

#[test]
fn search_returns_only_active_and_emits_memory_list_block() {
    let (service, tools, _modules) = hub();
    let active = service
        .save_confirmed(devtoolbox_core::memory::MemoryDraft::new(
            MemoryCategory::Environment,
            "Docker 数据目录是 /Volumes/Data/docker",
        ))
        .unwrap();
    service
        .propose(devtoolbox_core::memory::MemoryDraft::new(
            MemoryCategory::Preference,
            "今天晚上想吃寿司",
        ))
        .unwrap();

    // Search 不应该写任何东西，也不应命中候选。
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "memory.search".into(),
        arguments: json!({"query": "docker"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["count"], 1);
    assert_eq!(result.data["items"][0]["id"], active.id);
    assert_eq!(result.data["items"][0]["category_label"], "环境");

    let blocks = result.metadata["ui_hint"]["ui_blocks"]
        .as_array()
        .expect("ui_hint blocks");
    assert_eq!(blocks[0]["kind"], "memory_list");
    let _ = UiBlockKind::MemoryList;
}

#[test]
fn save_tool_only_creates_candidate_and_returns_confirm_action() {
    let (service, tools, _modules) = hub();
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c1".into(),
        name: "memory.save".into(),
        arguments: json!({"content": "家中服务器是 macOS", "category": "environment"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["memory"]["status"], "candidate");
    assert_eq!(result.data["needs_confirmation"], true);

    let actions = result.metadata["ui_hint"]["actions"]
        .as_array()
        .expect("confirm action");
    assert_eq!(actions[0]["type"], "confirm_memory");
    assert_eq!(actions[0]["module"], "memory");
    assert_eq!(actions[0]["target"]["category"], "environment");
    let id = actions[0]["target"]["memory_id"]
        .as_str()
        .unwrap()
        .to_string();

    // 关键不变量：模型路径写出来的仍是 Candidate（§113）。
    assert_eq!(
        service.get(&id, true).unwrap().status,
        MemoryStatus::Candidate
    );
    let _ = ActionKind::ConfirmMemory;

    // 用户确认后才是 ACTIVE。
    let confirmed = service.confirm(&id).unwrap();
    assert_eq!(confirmed.status, MemoryStatus::Active);
}

#[test]
fn save_tool_rejects_secrets_and_bad_category() {
    let (_service, tools, _modules) = hub();
    let secret = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.save".into(),
        arguments: json!({"content": "密码: hunter2xyz", "category": "personal_fact"}),
    }))
    .unwrap_err();
    assert_eq!(secret.code(), "personal_ai_tool_execution_failed");
    assert!(secret.message.contains("credential store"));

    let bad_category = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.save".into(),
        arguments: json!({"content": "x", "category": "not_a_category"}),
    }))
    .unwrap_err();
    assert_eq!(bad_category.code(), "personal_ai_tool_invalid_argument");

    // schema 校验：content 缺失不会进执行器。
    let missing = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.save".into(),
        arguments: json!({"category": "preference"}),
    }))
    .unwrap_err();
    assert_eq!(missing.code(), "personal_ai_tool_invalid_argument");
}

#[test]
fn get_and_list_never_expose_sensitive_items() {
    let (service, tools, _modules) = hub();
    let draft =
        devtoolbox_core::memory::MemoryDraft::new(MemoryCategory::PersonalFact, "体检记录在协和")
            .with_sensitivity(devtoolbox_core::memory::MemorySensitivity::Sensitive);
    let sensitive = service.save_confirmed(draft).unwrap();

    let blocked = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.get".into(),
        arguments: json!({"id": sensitive.id}),
    }))
    .unwrap_err();
    assert_eq!(blocked.code(), "personal_ai_tool_execution_failed");
    assert!(blocked.message.contains("敏感"));

    let listed = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.list".into(),
        arguments: json!({}),
    }))
    .unwrap();
    assert_eq!(listed.data["count"], 0);
}

#[test]
fn archive_tool_is_reversible_and_removes_from_search() {
    let (service, tools, _modules) = hub();
    let item = service
        .save_confirmed(devtoolbox_core::memory::MemoryDraft::new(
            MemoryCategory::Preference,
            "喜欢历史旅行",
        ))
        .unwrap();
    let archived = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.archive".into(),
        arguments: json!({"id": item.id}),
    }))
    .unwrap();
    assert!(archived.ok);
    assert_eq!(archived.data["memory"]["status"], "archived");

    let searched = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.search".into(),
        arguments: json!({"query": "历史"}),
    }))
    .unwrap();
    assert_eq!(searched.data["count"], 0);
    // 数据仍在（可恢复）。
    assert_eq!(
        service.get(&item.id, true).unwrap().status,
        MemoryStatus::Archived
    );
}

#[test]
fn empty_search_reports_no_result_without_ui_hint() {
    let (_service, tools, _modules) = hub();
    let result = block_on(tools.execute(&devtoolbox_core::ToolCallRequest {
        id: "c".into(),
        name: "memory.search".into(),
        arguments: json!({"query": "量子计算"}),
    }))
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.data["count"], 0);
    assert!(result.data["note"].as_str().unwrap().contains("没有找到"));
    assert!(result.metadata["ui_hint"].is_null());
}

#[test]
fn context_provider_reports_overview_and_entity() {
    let (service, _tools, modules) = hub();
    let item = service
        .save_confirmed(devtoolbox_core::memory::MemoryDraft::new(
            MemoryCategory::Environment,
            "Docker 数据目录是 /Volumes/Data/docker",
        ))
        .unwrap();
    let provider = modules.context_provider("memory").unwrap();

    let overview = provider
        .build_context(
            &AppContext {
                module: Some("memory".into()),
                ..AppContext::default()
            },
            &ContextBudget::default(),
        )
        .unwrap();
    assert_eq!(overview.module, "memory");
    assert_eq!(overview.summary["stats"]["active"], 1);
    assert_eq!(overview.summary["recent"][0]["id"], item.id);

    let entity = provider
        .build_context(
            &AppContext {
                module: Some("memory".into()),
                entity: Some(devtoolbox_core::personal_ai::EntityRef {
                    kind: "memory".into(),
                    id: item.id.clone(),
                    label: None,
                }),
                ..AppContext::default()
            },
            &ContextBudget::default(),
        )
        .unwrap();
    assert!(entity.headline.contains("环境"));
    assert_eq!(entity.summary["memory"]["content"], item.content);

    // 不存在的实体 → 受控 context 错误。
    let error = provider
        .build_context(
            &AppContext {
                module: Some("memory".into()),
                entity: Some(devtoolbox_core::personal_ai::EntityRef {
                    kind: "memory".into(),
                    id: "missing".into(),
                    label: None,
                }),
                ..AppContext::default()
            },
            &ContextBudget::default(),
        )
        .unwrap_err();
    assert_eq!(error.code(), "personal_ai_context_error");
}
