//! Travel 研究会话状态机与注册表（应用层拥有）。
//!
//! Gate 7（Application Boundary）：会话生命周期属于 application，不属于 Tauri。
//! 会话保持 in-memory（进程内 `Arc<Mutex<…>>`），不引入 Redis / DB 会话 /
//! 事件溯源 / 分布式会话。Runtime（desktop/server）只负责：
//! - 通过注册表登记/查询会话；
//! - 调度后台任务，把进度事件写入会话状态。
//!
//! 缓存决策是结构化字段（`finish(guide, from_cache)`），不再依赖事件文案匹配。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use devtoolbox_core::travel::{CityGuide, TravelResearchEvent};

/// 共享会话句柄（进程内引用计数）。
pub type SharedResearchSession = Arc<Mutex<TravelResearchSession>>;

/// 一次旅行研究的后台会话状态。
///
/// 只允许通过 `push_event` / `finish` / `fail` 变更；查询一律走 `view()` 快照，
/// 保证轮询线程不会与后台任务交错写读。
pub struct TravelResearchSession {
    events: Vec<TravelResearchEvent>,
    done: bool,
    error: Option<String>,
    guide: Option<CityGuide>,
    from_cache: bool,
}

/// 轮询快照（应用层视图；序列化 DTO 由 runtime 层保持，命令契约不变）。
#[derive(Clone, Debug)]
pub struct TravelSessionView {
    pub done: bool,
    pub error: Option<String>,
    pub from_cache: bool,
    pub events: Vec<TravelResearchEvent>,
    pub guide: Option<CityGuide>,
}

impl TravelResearchSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            done: false,
            error: None,
            guide: None,
            from_cache: false,
        }
    }

    /// 追加一条研究进度事件。
    pub fn push_event(&mut self, event: TravelResearchEvent) {
        self.events.push(event);
    }

    /// 成功结束：记录攻略与结构化缓存决策。
    /// `from_cache` 由 `research_city` 的 `ResearchOutcome` 直接给出，
    /// 不依赖事件文案（如「命中缓存攻略」）做字符串匹配。
    pub fn finish(&mut self, guide: CityGuide, from_cache: bool) {
        self.guide = Some(guide);
        self.from_cache = from_cache;
        self.done = true;
    }

    /// 失败结束。
    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
        self.done = true;
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// 只读快照。
    #[must_use]
    pub fn view(&self) -> TravelSessionView {
        TravelSessionView {
            done: self.done,
            error: self.error.clone(),
            from_cache: self.from_cache,
            events: self.events.clone(),
            guide: self.guide.clone(),
        }
    }
}

impl Default for TravelResearchSession {
    fn default() -> Self {
        Self::new()
    }
}

/// 应用层会话注册表：分配 session_id 并按 id 查询。
///
/// In-memory 生命周期与进程一致；符合 Gate 7 的「session 生命周期不属于
/// Tauri」——Tauri 命令只做 register/get，自身不持有会话容器。
#[derive(Clone, Default)]
pub struct TravelSessionRegistry {
    sessions: Arc<Mutex<HashMap<String, SharedResearchSession>>>,
    next_id: Arc<AtomicU64>,
}

impl TravelSessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个新会话，返回 `(session_id, 会话句柄)`。id 形如 `t1, t2, …`。
    pub fn register(&self) -> (String, SharedResearchSession) {
        let id = format!("t{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let session = Arc::new(Mutex::new(TravelResearchSession::new()));
        self.sessions
            .lock()
            .expect("travel registry poisoned")
            .insert(id.clone(), Arc::clone(&session));
        (id, session)
    }

    /// 按 id 查询会话；未知 id 返回 `None`。
    pub fn get(&self, session_id: &str) -> Option<SharedResearchSession> {
        self.sessions
            .lock()
            .expect("travel registry poisoned")
            .get(session_id)
            .cloned()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions
            .lock()
            .expect("travel registry poisoned")
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::travel::{ResearchPhase, StepStatus};

    fn phase_event(message: &str) -> TravelResearchEvent {
        TravelResearchEvent {
            phase: ResearchPhase::IdentifyCity,
            status: StepStatus::InProgress,
            message: message.to_string(),
            seq: 1,
        }
    }

    #[test]
    fn registry_assigns_unique_ids_and_retrieves() {
        let registry = TravelSessionRegistry::new();
        let (first, _) = registry.register();
        let (second, _) = registry.register();
        assert_ne!(first, second);
        assert_eq!(registry.len(), 2);
        assert!(registry.get(&first).is_some());
        assert!(registry.get(&second).is_some());
        assert!(registry.get("t999").is_none());
    }

    #[test]
    fn new_registry_starts_empty() {
        let registry = TravelSessionRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn session_tracks_progress_and_success() {
        let (_, session) = TravelSessionRegistry::new().register();
        let mut session = session.lock().expect("poisoned");
        assert!(!session.is_done());

        session.push_event(phase_event("识别城市"));
        assert_eq!(session.view().events.len(), 1);

        session.finish(CityGuide::default(), true);
        assert!(session.is_done());
        let view = session.view();
        assert!(view.done);
        assert!(view.from_cache);
        assert!(view.guide.is_some());
        assert!(view.error.is_none());
        assert_eq!(view.events.len(), 1);
    }

    #[test]
    fn session_records_failure() {
        let (_, session) = TravelSessionRegistry::new().register();
        let mut session = session.lock().expect("poisoned");
        session.fail("all providers failed".to_string());
        assert!(session.is_done());
        let view = session.view();
        assert!(view.done);
        assert!(!view.from_cache);
        assert!(view.guide.is_none());
        assert_eq!(view.error.as_deref(), Some("all providers failed"));
    }

    #[test]
    fn finish_with_fresh_research_reports_miss() {
        let mut session = TravelResearchSession::new();
        session.finish(CityGuide::default(), false);
        let view = session.view();
        assert!(!view.from_cache);
        assert!(view.guide.is_some());
    }
}
