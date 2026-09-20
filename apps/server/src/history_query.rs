//! History 只读查询端的 HTTP 适配层。
//!
//! 端口的实现必须放在平台层（infrastructure 不能反向依赖 application，
//! Gate 2 显式例外），桌面端与 HTTP 服务各自持有一份薄代理；这里与
//! `apps/desktop/src/history_query.rs` 相同：原样转发，仅做错误文本转换。

use std::sync::Arc;

use devtoolbox_application::history::{HistoryPortError, HistoryQueryPort};
use devtoolbox_core::history_records::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
    PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
    PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
    WorkResult,
};
use devtoolbox_infrastructure::{HistoryDuckDbRepository, InfrastructureError};

fn map_err(error: InfrastructureError) -> HistoryPortError {
    HistoryPortError(error.to_string())
}

/// 把 `HistoryDuckDbRepository`（infra，只读 DuckDB）绑定到 `HistoryQueryPort`。
#[derive(Clone)]
pub struct HistoryQueryAdapter {
    repository: Arc<HistoryDuckDbRepository>,
}

impl HistoryQueryAdapter {
    #[must_use]
    pub fn new(repository: Arc<HistoryDuckDbRepository>) -> Self {
        Self { repository }
    }
}

impl HistoryQueryPort for HistoryQueryAdapter {
    fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError> {
        self.repository.get_dataset_stats().map_err(map_err)
    }

    fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError> {
        self.repository.get_periods().map_err(map_err)
    }

    fn get_regimes_by_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<RegimeResult>, HistoryPortError> {
        self.repository
            .get_regimes_by_period(period_id)
            .map_err(map_err)
    }

    fn get_events_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
        self.repository
            .get_events_for_period(period_id)
            .map_err(map_err)
    }

    fn get_people_for_period(
        &self,
        period_id: &str,
        limit: i64,
    ) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
        self.repository
            .get_people_for_period(period_id, limit)
            .map_err(map_err)
    }

    fn get_relations_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        self.repository
            .get_relations_for_period(period_id)
            .map_err(map_err)
    }

    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
        self.repository.get_stories().map_err(map_err)
    }

    fn get_stories_for_period(
        &self,
        period_id: Option<&str>,
    ) -> Result<Vec<StoryResult>, HistoryPortError> {
        self.repository
            .get_stories_for_period(period_id)
            .map_err(map_err)
    }

    fn get_story(&self, story_id: &str) -> Result<Option<StoryResult>, HistoryPortError> {
        self.repository.get_story(story_id).map_err(map_err)
    }

    fn get_story_events(&self, story_id: &str) -> Result<Vec<StoryEventResult>, HistoryPortError> {
        self.repository.get_story_events(story_id).map_err(map_err)
    }

    fn get_story_people(&self, story_id: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        self.repository.get_story_people(story_id).map_err(map_err)
    }

    fn get_story_places(&self, story_id: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        self.repository.get_story_places(story_id).map_err(map_err)
    }

    fn get_story_texts(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        self.repository.get_story_texts(story_id).map_err(map_err)
    }

    fn get_story_evidences(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        self.repository
            .get_story_evidences(story_id)
            .map_err(map_err)
    }

    fn get_event(&self, event_id: &str) -> Result<Option<EventResult>, HistoryPortError> {
        self.repository.get_event(event_id).map_err(map_err)
    }

    fn get_event_people(&self, event_id: &str) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        self.repository.get_event_people(event_id).map_err(map_err)
    }

    fn get_event_places(&self, event_id: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        self.repository.get_event_places(event_id).map_err(map_err)
    }

    fn get_event_relations(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        self.repository
            .get_event_relations(event_id)
            .map_err(map_err)
    }

    fn get_event_texts(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        self.repository.get_event_texts(event_id).map_err(map_err)
    }

    fn get_event_evidences(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        self.repository
            .get_event_evidences(event_id)
            .map_err(map_err)
    }

    fn get_person(&self, person_id: &str) -> Result<Option<PersonResult>, HistoryPortError> {
        self.repository.get_person(person_id).map_err(map_err)
    }

    fn get_person_relations(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
        self.repository
            .get_person_relations(person_id)
            .map_err(map_err)
    }

    fn get_person_places(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
        self.repository
            .get_person_places(person_id)
            .map_err(map_err)
    }

    fn get_person_events(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonEventResult>, HistoryPortError> {
        self.repository
            .get_person_events(person_id)
            .map_err(map_err)
    }

    fn get_person_stories(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
        self.repository
            .get_person_stories(person_id)
            .map_err(map_err)
    }

    fn get_work_by_id(&self, work_id: &str) -> Result<Option<WorkResult>, HistoryPortError> {
        self.repository.get_work_by_id(work_id).map_err(map_err)
    }

    fn get_work(&self, title: &str, limit: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
        self.repository.get_work(title, limit).map_err(map_err)
    }

    fn get_historical_texts(
        &self,
        work: Option<&str>,
        limit: i64,
    ) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
        self.repository
            .get_historical_texts(work, limit)
            .map_err(map_err)
    }

    fn search_people(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<PersonResult>, HistoryPortError> {
        self.repository.search_people(query, limit).map_err(map_err)
    }

    fn search_events(&self, query: &str, limit: i64) -> Result<Vec<EventResult>, HistoryPortError> {
        self.repository.search_events(query, limit).map_err(map_err)
    }

    fn get_sources_for_ids(&self, ids: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
        self.repository.get_sources_for_ids(ids).map_err(map_err)
    }
}
