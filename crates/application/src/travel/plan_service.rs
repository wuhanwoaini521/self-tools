//! Safe, version-checked TravelPlan proposals used by PersonalAgent.
//!
//! A model may create a short-lived preview ticket, never persist a plan. Confirmation
//! consumes that ticket once and performs a second compare-and-swap against the latest
//! stored revision. Tickets are process-local bearer values and contain no plan writes.

use std::collections::HashMap;
use std::sync::Arc;

use devtoolbox_core::personal_ai::{AgentProposedChange, AppContext};
use devtoolbox_core::travel::{TravelPlan, TravelPlanContext, TravelPlanError, TravelPlanPlace};
use parking_lot::Mutex;
use uuid::Uuid;

use super::ports::{
    GeographyPlace, GeographyPlaceLookup, TravelPlanPortError, TravelPlanStorePort,
};

const PROPOSAL_TTL_SECS: i64 = 5 * 60;
const MAX_PENDING_PROPOSALS: usize = 100;
const MAX_NOTE_CHARS: usize = 1_000;
const OPERATION_ADD_GEOGRAPHY_STOP: &str = "travel.add_geography_stop";

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum TravelPlanChangeError {
    #[error("只能在 Travel 行程页面提出修改。")]
    InvalidContext,
    #[error("请先打开并保存当前行程，再提出修改。")]
    PlanNotSaved,
    #[error("当前行程不存在或已删除，请重新打开。")]
    PlanNotFound,
    #[error("行程版本已变化，预览已过期；请重新载入并再次确认。")]
    Conflict,
    #[error("Geography stable ID 不存在；请先从 Geography 搜索确认地点。")]
    GeographyNotFound,
    #[error("该地点已在行程中。")]
    AlreadyIncluded,
    #[error("行程日期无效。")]
    InvalidDay,
    #[error("修改内容超出长度限制或包含非法字符。")]
    InvalidInput,
    #[error("确认预览不存在或已处理。")]
    InvalidProposal,
    #[error("确认预览已过期。")]
    Expired,
    #[error("行程校验失败：{0}")]
    InvalidPlan(String),
    #[error("行程存储失败：{0}")]
    Storage(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProposalState {
    Pending,
    Consumed,
    Cancelled,
    Expired,
}

#[derive(Clone)]
struct ProposalRecord {
    value: AgentProposedChange,
    context: TravelPlanContext,
    proposed_plan: TravelPlan,
    state: ProposalState,
}

pub struct TravelPlanChangeService {
    store: Arc<dyn TravelPlanStorePort>,
    geography: Arc<dyn GeographyPlaceLookup>,
    proposals: Mutex<HashMap<String, ProposalRecord>>,
}

impl TravelPlanChangeService {
    #[must_use]
    pub fn new(
        store: Arc<dyn TravelPlanStorePort>,
        geography: Arc<dyn GeographyPlaceLookup>,
    ) -> Self {
        Self {
            store,
            geography,
            proposals: Mutex::new(HashMap::new()),
        }
    }

    /// Build a preview from a persisted, current Travel plan and a canonical Geography ID.
    pub fn propose_add_geography_place(
        &self,
        app_context: &AppContext,
        geography_id: &str,
        day: u8,
        note: &str,
        now: i64,
    ) -> Result<AgentProposedChange, TravelPlanChangeError> {
        if app_context.module.as_deref() != Some("travel")
            || app_context
                .entity
                .as_ref()
                .is_none_or(|entity| entity.kind != "destination")
        {
            return Err(TravelPlanChangeError::InvalidContext);
        }
        let entity = app_context.entity.as_ref().expect("checked above");
        let view = app_context
            .view_state
            .as_object()
            .ok_or(TravelPlanChangeError::InvalidContext)?;
        let days = view
            .get("days")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or(TravelPlanChangeError::InvalidContext)?;
        let date_range = match view.get("date_range") {
            Some(value) if !value.is_null() => Some(
                serde_json::from_value(value.clone())
                    .map_err(|_| TravelPlanChangeError::InvalidContext)?,
            ),
            _ => None,
        };
        let expected_revision = view
            .get("plan_revision")
            .and_then(serde_json::Value::as_i64)
            .ok_or(TravelPlanChangeError::PlanNotSaved)?;
        if view
            .get("plan_persisted")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
            || expected_revision <= 0
        {
            return Err(TravelPlanChangeError::PlanNotSaved);
        }
        let context = TravelPlanContext {
            city: entity.id.clone(),
            days,
            date_range,
        }
        .validate()
        .map_err(map_plan_validation)?;
        let plan = self
            .store
            .get_plan(&context)
            .map_err(map_store_error)?
            .ok_or(TravelPlanChangeError::PlanNotFound)?;
        if plan.revision != expected_revision {
            return Err(TravelPlanChangeError::Conflict);
        }
        if !(1..=plan.context.days).contains(&day) {
            return Err(TravelPlanChangeError::InvalidDay);
        }
        let note = note.trim();
        if note.chars().count() > MAX_NOTE_CHARS || note.chars().any(char::is_control) {
            return Err(TravelPlanChangeError::InvalidInput);
        }
        let stable_id = geography_id.trim();
        if stable_id.is_empty()
            || stable_id.chars().count() > 512
            || stable_id.chars().any(char::is_control)
        {
            return Err(TravelPlanChangeError::GeographyNotFound);
        }
        let place = self
            .geography
            .find_place(stable_id)
            .map_err(TravelPlanChangeError::Storage)?
            .ok_or(TravelPlanChangeError::GeographyNotFound)?;
        let key = format!("custom:geography:{}", place.id);
        if plan.selected_keys.iter().any(|existing| existing == &key)
            || plan.custom_places.contains_key(&key)
        {
            return Err(TravelPlanChangeError::AlreadyIncluded);
        }

        let before_stops: Vec<_> = plan.selected_keys.iter().filter(|key| {
            plan.assigned_days.get(*key).copied().unwrap_or(1) == day
        }).map(|key| serde_json::json!({
            "id": key,
            "name": plan.custom_places.get(key).map(|place| place.name.as_str()).unwrap_or(key),
        })).collect();
        let mut proposed_plan = plan.clone();
        proposed_plan.custom_places.insert(
            key.clone(),
            TravelPlanPlace {
                name: place.name.clone(),
                note: note.to_string(),
            },
        );
        proposed_plan.selected_keys.push(key.clone());
        proposed_plan.assigned_days.insert(key.clone(), day);
        proposed_plan = proposed_plan.validate().map_err(map_plan_validation)?;

        let mut after_stops = before_stops.clone();
        after_stops.push(serde_json::json!({
            "id": key,
            "name": place.name,
            "note": note,
        }));
        let proposal = AgentProposedChange {
            id: Uuid::new_v4().to_string(),
            operation: OPERATION_ADD_GEOGRAPHY_STOP.to_string(),
            title: format!("将{}加入{} · 第{}天", place.name, context.city, day),
            expires_at: now.saturating_add(PROPOSAL_TTL_SECS),
            expected_revision: Some(expected_revision),
            before: serde_json::json!({"day": day, "stops": before_stops}),
            after: serde_json::json!({"day": day, "stops": after_stops}),
        };
        let record = ProposalRecord {
            value: proposal.clone(),
            context,
            proposed_plan,
            state: ProposalState::Pending,
        };
        let mut proposals = self.proposals.lock();
        proposals
            .retain(|_, item| item.state == ProposalState::Pending && item.value.expires_at > now);
        if proposals.len() >= MAX_PENDING_PROPOSALS {
            return Err(TravelPlanChangeError::Storage(
                "待确认预览数量已达上限".to_string(),
            ));
        }
        proposals.insert(proposal.id.clone(), record);
        Ok(proposal)
    }

    /// Consume and commit one preview. The DB revision is checked again immediately before save.
    pub fn confirm(
        &self,
        proposal_id: &str,
        now: i64,
    ) -> Result<TravelPlan, TravelPlanChangeError> {
        let record = {
            let mut proposals = self.proposals.lock();
            let record = proposals
                .get_mut(proposal_id)
                .ok_or(TravelPlanChangeError::InvalidProposal)?;
            if record.state != ProposalState::Pending {
                return Err(TravelPlanChangeError::InvalidProposal);
            }
            if now >= record.value.expires_at {
                record.state = ProposalState::Expired;
                return Err(TravelPlanChangeError::Expired);
            }
            record.state = ProposalState::Consumed;
            record.clone()
        };
        let expected_revision = record
            .value
            .expected_revision
            .ok_or(TravelPlanChangeError::InvalidProposal)?;
        let latest = self
            .store
            .get_plan(&record.context)
            .map_err(map_store_error)?
            .ok_or(TravelPlanChangeError::Conflict)?;
        if latest.revision != expected_revision {
            return Err(TravelPlanChangeError::Conflict);
        }
        self.store
            .save_plan(&record.proposed_plan, now)
            .map_err(map_store_error)
    }

    /// Cancel is terminal and has no effect on the persisted plan.
    pub fn cancel(&self, proposal_id: &str) -> Result<(), TravelPlanChangeError> {
        let mut proposals = self.proposals.lock();
        let record = proposals
            .get_mut(proposal_id)
            .ok_or(TravelPlanChangeError::InvalidProposal)?;
        if record.state != ProposalState::Pending {
            return Err(TravelPlanChangeError::InvalidProposal);
        }
        record.state = ProposalState::Cancelled;
        Ok(())
    }
}

impl GeographyPlaceLookup for crate::geography::GeographyService {
    fn find_place(&self, stable_id: &str) -> Result<Option<GeographyPlace>, String> {
        self.detail(stable_id)
            .map_err(|error| error.to_string())
            .map(|detail| {
                detail.map(|detail| GeographyPlace {
                    id: detail.entity.id,
                    name: detail.entity.name,
                })
            })
    }
}

fn map_store_error(error: TravelPlanPortError) -> TravelPlanChangeError {
    match error {
        TravelPlanPortError::Conflict => TravelPlanChangeError::Conflict,
        TravelPlanPortError::Storage(message) => TravelPlanChangeError::Storage(message),
    }
}

fn map_plan_validation(error: TravelPlanError) -> TravelPlanChangeError {
    TravelPlanChangeError::InvalidPlan(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::travel::ports::{GeographyPlaceLookup, TravelPlanPortError, TravelPlanStorePort};
    use devtoolbox_core::personal_ai::{AppContext, EntityRef};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct FakeStore {
        plan: Mutex<Option<TravelPlan>>,
        writes: Mutex<usize>,
    }

    impl TravelPlanStorePort for FakeStore {
        fn get_plan(
            &self,
            context: &TravelPlanContext,
        ) -> Result<Option<TravelPlan>, TravelPlanPortError> {
            Ok(self
                .plan
                .lock()
                .as_ref()
                .filter(|plan| &plan.context == context)
                .cloned())
        }

        fn save_plan(
            &self,
            plan: &TravelPlan,
            now: i64,
        ) -> Result<TravelPlan, TravelPlanPortError> {
            let mut state = self.plan.lock();
            let current_revision = state.as_ref().map_or(0, |current| current.revision);
            if plan.revision != current_revision {
                return Err(TravelPlanPortError::Conflict);
            }
            let mut saved = plan
                .clone()
                .validate()
                .map_err(|error| TravelPlanPortError::Storage(error.to_string()))?;
            saved.revision += 1;
            saved.updated_at = now;
            *state = Some(saved.clone());
            *self.writes.lock() += 1;
            Ok(saved)
        }
    }

    struct FakeGeography;
    impl GeographyPlaceLookup for FakeGeography {
        fn find_place(&self, id: &str) -> Result<Option<GeographyPlace>, String> {
            Ok((id == "luoyang").then(|| GeographyPlace {
                id: id.into(),
                name: "洛阳".into(),
            }))
        }
    }

    fn saved_plan(revision: i64) -> TravelPlan {
        TravelPlan {
            context: TravelPlanContext {
                city: "杭州".into(),
                days: 2,
                date_range: None,
            },
            title: "周末行程".into(),
            selected_keys: vec!["poi-west-lake".into()],
            assigned_days: BTreeMap::from([("poi-west-lake".into(), 1)]),
            revision,
            ..TravelPlan::default()
        }
    }

    fn app_context(revision: i64) -> AppContext {
        AppContext {
            module: Some("travel".into()),
            page: Some("guide".into()),
            entity: Some(EntityRef {
                kind: "destination".into(),
                id: "杭州".into(),
                label: Some("杭州".into()),
            }),
            view_state: serde_json::json!({
                "days": 2,
                "date_range": null,
                "plan_revision": revision,
                "plan_persisted": true
            }),
            ..AppContext::default()
        }
    }

    fn service() -> (TravelPlanChangeService, Arc<FakeStore>) {
        let store = Arc::new(FakeStore {
            plan: Mutex::new(Some(saved_plan(3))),
            writes: Mutex::new(0),
        });
        let service = TravelPlanChangeService::new(store.clone(), Arc::new(FakeGeography));
        (service, store)
    }

    #[test]
    fn propose_is_preview_only_and_confirm_saves_exact_diff_once() {
        let (service, store) = service();
        let proposal = service
            .propose_add_geography_place(&app_context(3), "luoyang", 2, "  看唐代遗址  ", 100)
            .unwrap();
        assert_eq!(*store.writes.lock(), 0);
        assert_eq!(proposal.expected_revision, Some(3));
        assert_eq!(proposal.before["day"], 2);
        assert_eq!(proposal.before["stops"], serde_json::json!([]));
        assert_eq!(proposal.after["stops"][0]["id"], "custom:geography:luoyang");
        assert_eq!(proposal.after["stops"][0]["note"], "看唐代遗址");

        let saved = service.confirm(&proposal.id, 200).unwrap();
        assert_eq!(saved.revision, 4);
        assert!(
            saved
                .selected_keys
                .contains(&"custom:geography:luoyang".to_string())
        );
        assert_eq!(saved.custom_places["custom:geography:luoyang"].name, "洛阳");
        assert_eq!(*store.writes.lock(), 1);
        assert!(
            service.confirm(&proposal.id, 201).is_err(),
            "confirmation tickets are one-shot"
        );
    }

    #[test]
    fn stale_revision_and_non_geography_ids_fail_closed() {
        let (service, store) = service();
        assert!(matches!(
            service.propose_add_geography_place(&app_context(2), "luoyang", 2, "", 100),
            Err(TravelPlanChangeError::Conflict)
        ));
        assert!(matches!(
            service.propose_add_geography_place(&app_context(3), "洛阳", 2, "", 100),
            Err(TravelPlanChangeError::GeographyNotFound)
        ));
        assert_eq!(*store.writes.lock(), 0);
    }

    #[test]
    fn compare_and_swap_conflict_after_preview_prevents_confirmation_write() {
        let (service, store) = service();
        let proposal = service
            .propose_add_geography_place(&app_context(3), "luoyang", 2, "", 100)
            .unwrap();
        *store.plan.lock() = Some(saved_plan(4));
        assert!(matches!(
            service.confirm(&proposal.id, 101),
            Err(TravelPlanChangeError::Conflict)
        ));
        assert_eq!(*store.writes.lock(), 0);
        assert!(
            !store
                .plan
                .lock()
                .as_ref()
                .unwrap()
                .selected_keys
                .contains(&"custom:geography:luoyang".to_string())
        );
    }

    #[test]
    fn cancelled_and_expired_preview_tickets_never_write() {
        let (service, store) = service();
        let cancelled = service
            .propose_add_geography_place(&app_context(3), "luoyang", 2, "", 100)
            .unwrap();
        service.cancel(&cancelled.id).unwrap();
        assert!(service.confirm(&cancelled.id, 101).is_err());

        let expired = service
            .propose_add_geography_place(&app_context(3), "luoyang", 2, "", 100)
            .unwrap();
        assert!(matches!(
            service.confirm(&expired.id, expired.expires_at),
            Err(TravelPlanChangeError::Expired)
        ));
        assert_eq!(*store.writes.lock(), 0);
    }
}
