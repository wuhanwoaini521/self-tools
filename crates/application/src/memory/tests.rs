//! Memory 域测试（V6 §95 六个 Case + gate 边界）。
//!
//! 全部用内存 Fake 端口驱动，不触真实文件系统与 SQLite；SQLite 语义由
//! infrastructure 的 store 测试覆盖。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_core::memory::{
    MemoryCategory, MemoryDraft, MemoryItem, MemoryQuery, MemorySensitivity, MemorySourceType,
    MemoryStatus,
};

use super::ports::{MemoryStoreError, MemoryStorePort};
use super::service::MemoryService;

/// 内存 Fake：语义与 SQLite 实现一致（LIKE 粗筛 + category/status 过滤 + 倒序）。
#[derive(Default)]
struct FakeMemoryStore {
    items: Mutex<HashMap<String, MemoryItem>>,
    touch_calls: Mutex<Vec<(Vec<String>, i64)>>,
}

impl FakeMemoryStore {
    fn all(&self) -> Vec<MemoryItem> {
        self.items.lock().values().cloned().collect()
    }
}

impl MemoryStorePort for FakeMemoryStore {
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
            .all()
            .into_iter()
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
        self.touch_calls.lock().push((ids.to_vec(), now));
        Ok(())
    }

    fn count_by_status(&self) -> Result<Vec<(MemoryStatus, usize)>, MemoryStoreError> {
        let mut counts: Vec<(MemoryStatus, usize)> = Vec::new();
        for status in MemoryStatus::ALL {
            let count = self
                .all()
                .into_iter()
                .filter(|item| item.status == status)
                .count();
            if count > 0 {
                counts.push((status, count));
            }
        }
        Ok(counts)
    }

    fn count_by_category(&self) -> Result<Vec<(MemoryCategory, usize)>, MemoryStoreError> {
        let mut counts: Vec<(MemoryCategory, usize)> = Vec::new();
        for category in MemoryCategory::ALL {
            let count = self
                .all()
                .into_iter()
                .filter(|item| item.category == category)
                .count();
            if count > 0 {
                counts.push((category, count));
            }
        }
        Ok(counts)
    }
}

fn service() -> (MemoryService, Arc<FakeMemoryStore>) {
    let store = Arc::new(FakeMemoryStore::default());
    let service = MemoryService::new(store.clone());
    (service, store)
}

fn draft(content: &str) -> MemoryDraft {
    MemoryDraft::new(MemoryCategory::Preference, content)
}

// ---- Case 1：显式保存 → 确认后 ACTIVE ----

#[test]
fn case1_explicit_save_then_confirm_becomes_active() {
    let (service, store) = service();
    let candidate = service
        .propose_explicit(
            draft("我的 Docker 数据都放在 /Volumes/Data/docker")
                .with_source(MemorySourceType::ExplicitUser, Some("chat:1".into())),
        )
        .expect("propose explicit");
    assert_eq!(candidate.status, MemoryStatus::Candidate);
    assert_eq!(candidate.source_type, MemorySourceType::ExplicitUser);

    let active = service.confirm(&candidate.id).expect("confirm");
    assert_eq!(active.status, MemoryStatus::Active);
    assert_eq!(active.metadata["confirmed_by"], "ui");
    assert_eq!(
        store.get(&candidate.id).unwrap().unwrap().status,
        MemoryStatus::Active
    );

    // 再次确认 → 受控错误（不是 panic、不是静默成功）。
    let error = service.confirm(&candidate.id).unwrap_err();
    assert!(error.to_string().contains("只有候选状态"));
}

#[test]
fn ui_confirmed_save_is_active_immediately() {
    let (service, _) = service();
    let item = service
        .save_confirmed(draft("喜欢历史类的旅行"))
        .expect("save confirmed");
    assert_eq!(item.status, MemoryStatus::Active);
    assert_eq!(item.metadata["confirmed_by"], "ui");
}

// ---- Case 2：普通对话 → 不产生 ACTIVE ----

#[test]
fn case2_model_path_never_produces_active() {
    let (service, store) = service();
    let candidate = service.propose(draft("今天晚上想吃寿司")).expect("propose");
    assert_eq!(candidate.status, MemoryStatus::Candidate);
    assert_ne!(candidate.status, MemoryStatus::Active);

    // 检索（模型可见路径）看不到候选。
    let hits = service.search("寿司", None, 10).unwrap();
    assert!(hits.is_empty());
    assert_eq!(
        store.get(&candidate.id).unwrap().unwrap().status,
        MemoryStatus::Candidate
    );
}

// ---- Case 3：search 命中相关记忆 ----

#[test]
fn case3_search_returns_relevant_active_memory() {
    let (service, store) = service();
    let docker = service
        .save_confirmed(draft("Docker 数据目录是 /Volumes/Data/docker"))
        .unwrap();
    service
        .save_confirmed(draft("喜欢历史与摄影类型的旅行"))
        .unwrap();

    let hits = service.search("docker", None, 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, docker.id);
    // 命中后应记录使用时间（写回存储，不改变本次返回快照）。
    assert!(
        store
            .get(&docker.id)
            .unwrap()
            .unwrap()
            .last_used_at
            .is_some(),
        "命中后应记录使用时间"
    );

    // token 命中（多关键词）。
    let hits = service.search("摄影 旅行", None, 5).unwrap();
    assert_eq!(hits.len(), 1);

    // 类别过滤。
    assert!(
        service
            .search("docker", Some(MemoryCategory::Routine), 5)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service
            .search("docker", Some(MemoryCategory::Preference), 5)
            .unwrap()
            .len(),
        1
    );

    // 无结果 → 空（§67：不编造）。
    assert!(service.search("量子计算", None, 5).unwrap().is_empty());
}

// ---- Case 4：archive 后不参与默认检索 ----

#[test]
fn case4_archived_memory_is_not_retrieved() {
    let (service, store) = service();
    let item = service
        .save_confirmed(draft("Docker 数据目录是 /Volumes/Data/docker"))
        .unwrap();
    assert_eq!(service.search("docker", None, 5).unwrap().len(), 1);

    let archived = service.archive(&item.id).unwrap();
    assert_eq!(archived.status, MemoryStatus::Archived);
    assert!(service.search("docker", None, 5).unwrap().is_empty());
    // 归档是可恢复的：仍在库里，管理页可见。
    assert_eq!(
        store.get(&item.id).unwrap().unwrap().status,
        MemoryStatus::Archived
    );
    let listed = service
        .list(&MemoryQuery {
            status: Some(MemoryStatus::Archived),
            include_sensitive: true,
            ..MemoryQuery::default()
        })
        .unwrap();
    assert_eq!(listed.len(), 1);
}

// ---- Case 5：secret 类内容拒绝 ----

#[test]
fn case5_secret_like_content_is_rejected() {
    let (service, store) = service();
    for content in [
        "密码: hunter2xyz",
        "api key 是 sk-abcdefghijklmnopqrstuvwx",
        "-----BEGIN RSA PRIVATE KEY----- MIIE",
        "~/.ssh/id_rsa 在这里",
    ] {
        let error = service.propose(draft_with_fact(content)).unwrap_err();
        assert!(
            error.to_string().contains("credential store"),
            "应提示凭据存储: {error}"
        );
    }
    assert!(store.all().is_empty(), "拒绝的内容不得写入存储");

    // 编辑路径同样受 gate 保护。
    let ok = service
        .save_confirmed(draft("Docker 数据目录是 /Volumes/Data/docker"))
        .unwrap();
    let error = service
        .update(&ok.id, "密码: hunter2xyz", None, None)
        .unwrap_err();
    assert!(error.to_string().contains("credential store"));
    assert_eq!(
        store.get(&ok.id).unwrap().unwrap().content,
        "Docker 数据目录是 /Volumes/Data/docker"
    );
}

fn draft_with_fact(content: &str) -> MemoryDraft {
    MemoryDraft::new(MemoryCategory::ProjectFact, content)
}

// ---- Case 6：candidate → confirm → active（含敏感与过期边界） ----

#[test]
fn case6_candidate_confirm_flow_with_sensitivity_and_expiry() {
    let (service, _) = service();
    let candidate = service
        .propose(draft("家中 Personal Server 是 macOS"))
        .unwrap();
    assert_eq!(service.search("macOS", None, 5).unwrap().len(), 0);

    let active = service.confirm(&candidate.id).unwrap();
    assert_eq!(active.status, MemoryStatus::Active);
    assert_eq!(service.search("macOS", None, 5).unwrap().len(), 1);

    // 敏感项：工具路径不返回（§69）。
    let sensitive = service
        .save_confirmed(
            MemoryDraft::new(MemoryCategory::PersonalFact, "年度体检在协和")
                .with_sensitivity(MemorySensitivity::Sensitive),
        )
        .unwrap();
    assert!(service.search("体检", None, 5).unwrap().is_empty());
    assert!(service.get(&sensitive.id, false).is_err());
    let found = service.get(&sensitive.id, true).unwrap();
    assert_eq!(found.sensitivity, MemorySensitivity::Sensitive);
    let managed = service
        .list(&MemoryQuery {
            include_sensitive: true,
            ..MemoryQuery::default()
        })
        .unwrap();
    assert!(managed.iter().any(|item| item.id == sensitive.id));
    // 默认列表（模型可见路径）不含敏感项。
    let default_list = service.list(&MemoryQuery::default()).unwrap();
    assert!(!default_list.iter().any(|item| item.id == sensitive.id));
}

#[test]
fn expired_memories_are_excluded_and_marked() {
    let (service, store) = service();
    let mut draft = draft("临时偏好：这周只吃素");
    draft.expires_at = Some(1);
    let item = service.save_confirmed(draft).unwrap();
    assert_eq!(item.status, MemoryStatus::Active);

    let expired = service.expire_due().unwrap();
    assert_eq!(expired, 1);
    assert_eq!(
        store.get(&item.id).unwrap().unwrap().status,
        MemoryStatus::Expired
    );
    assert!(service.search("吃素", None, 5).unwrap().is_empty());
    assert_eq!(service.expire_due().unwrap(), 0, "重复清理必须幂等");
}

#[test]
fn reject_transitions_candidate_and_blocks_archive() {
    let (service, _) = service();
    let candidate = service.propose(draft("今天晚上想吃寿司")).unwrap();
    let rejected = service.reject(&candidate.id).unwrap();
    assert_eq!(rejected.status, MemoryStatus::Rejected);
    assert!(service.archive(&candidate.id).is_err());
    assert!(service.confirm(&candidate.id).is_err());
    assert!(service.search("寿司", None, 5).unwrap().is_empty());
}

#[test]
fn stats_and_category_counts_reflect_state() {
    let (service, _) = service();
    service
        .save_confirmed(draft("Docker 数据在 /Volumes/Data/docker"))
        .unwrap();
    let candidate = service.propose(draft("喜欢夜景摄影")).unwrap();
    service.archive(&candidate.id).unwrap();

    let stats = service.stats().unwrap();
    assert_eq!(stats.active, 1);
    assert_eq!(stats.archived, 1);
    assert_eq!(stats.total(), 2);

    let counts = service.category_counts().unwrap();
    assert!(
        counts
            .iter()
            .any(|(category, count)| *category == MemoryCategory::Preference && *count == 2)
    );
}

#[test]
fn validation_rejects_empty_long_and_low_confidence() {
    let (service, _) = service();
    assert!(service.propose(draft("   ")).is_err());
    assert!(service.propose(draft(&"x".repeat(500))).is_err());
    assert!(
        service
            .propose(draft("喜欢夜景摄影").with_confidence(0.01))
            .is_err()
    );
}

#[test]
fn knowledge_projection_carries_category_and_source() {
    let (service, _) = service();
    let item = service
        .save_confirmed(
            MemoryDraft::new(
                MemoryCategory::Environment,
                "Docker 数据目录是 /Volumes/Data/docker",
            )
            .with_source(MemorySourceType::ExplicitUser, Some("chat:9".into())),
        )
        .unwrap();
    let results = service.to_knowledge_results(&[item], "docker");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "环境 · environment");
    assert!(results[0].snippet.contains("docker"));
    assert!(results[0].score > 0.5);
    // provenance 保留（§66）。
    assert_eq!(results[0].provenance.source_id, results[0].source_id);
    assert_eq!(
        results[0].provenance.source_kind,
        devtoolbox_core::knowledge::KnowledgeSourceKind::Memory
    );
    assert!(!results[0].metadata.is_null());
}

#[test]
fn update_edits_content_and_category() {
    let (service, store) = service();
    let item = service.save_confirmed(draft("喜欢历史旅行")).unwrap();
    let updated = service
        .update(
            &item.id,
            "喜欢历史与摄影旅行",
            Some(MemoryCategory::Preference),
            None,
        )
        .unwrap();
    assert_eq!(updated.content, "喜欢历史与摄影旅行");
    assert!(updated.updated_at >= item.updated_at);
    assert_eq!(
        store.get(&item.id).unwrap().unwrap().content,
        "喜欢历史与摄影旅行"
    );
}
