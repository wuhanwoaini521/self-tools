//! HistoryQueryPort 的桌面端适配器：把用例层接口绑定到
//! `HistoryDuckDbRepository`（infra 的只读查询）。每个方法原样转发，
//! 不做任何业务决策 —— 查询都在 application 用例层。
//!
//! Port 及其错误类型属于 application crate；infrastructure 不能反向依赖
//! application（否则成环），所以适配器放在桌面端组合层，并把基础设施错误
//! 转换为 `HistoryPortError` 的可显示文本（消息文本保持不变）。

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

/// 把基础设施错误转换为端口错误（保留原始可显示文本）。
fn map_err(error: InfrastructureError) -> HistoryPortError {
    HistoryPortError(error.to_string())
}

/// 适配器保存在 Tauri State 中的仓库引用，每次命令调用时构造轻量代理。
///
/// 说明：trait 定义在 use-case 侧（application crate），而仓库类型在
/// infrastructure crate；infrastructure 不能反向依赖 application（否则成环），
/// 所以实现放在桌面端适配层 —— 这是 Gate 2 允许的显式例外。
#[derive(Clone)]
pub struct HistoryQueryAdapter {
    repository: Arc<HistoryDuckDbRepository>,
}

impl HistoryQueryAdapter {
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
    fn get_regimes_by_period(&self, period_id: &str) -> Result<Vec<RegimeResult>, HistoryPortError> {
        self.repository.get_regimes_by_period(period_id).map_err(map_err)
    }
    fn get_events_for_period(&self, period_id: &str) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
        self.repository.get_events_for_period(period_id).map_err(map_err)
    }
    fn get_people_for_period(&self, period_id: &str, limit: i64) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
        self.repository.get_people_for_period(period_id, limit).map_err(map_err)
    }
    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
        self.repository.get_stories().map_err(map_err)
    }
    fn get_stories_for_period(&self, period_id: Option<&str>) -> Result<Vec<StoryResult>, HistoryPortError> {
        self.repository.get_stories_for_period(period_id).map_err(map_err)
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
    fn get_story_texts(&self, story_id: &str) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        self.repository.get_story_texts(story_id).map_err(map_err)
    }
    fn get_story_evidences(&self, story_id: &str) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        self.repository.get_story_evidences(story_id).map_err(map_err)
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
    fn get_event_relations(&self, event_id: &str) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        self.repository.get_event_relations(event_id).map_err(map_err)
    }
    fn get_event_texts(&self, event_id: &str) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        self.repository.get_event_texts(event_id).map_err(map_err)
    }
    fn get_event_evidences(&self, event_id: &str) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        self.repository.get_event_evidences(event_id).map_err(map_err)
    }
    fn get_person(&self, person_id: &str) -> Result<Option<PersonResult>, HistoryPortError> {
        self.repository.get_person(person_id).map_err(map_err)
    }
    fn get_person_relations(&self, person_id: &str) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
        self.repository.get_person_relations(person_id).map_err(map_err)
    }
    fn get_person_places(&self, person_id: &str) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
        self.repository.get_person_places(person_id).map_err(map_err)
    }
    fn get_person_events(&self, person_id: &str) -> Result<Vec<PersonEventResult>, HistoryPortError> {
        self.repository.get_person_events(person_id).map_err(map_err)
    }
    fn get_person_stories(&self, person_id: &str) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
        self.repository.get_person_stories(person_id).map_err(map_err)
    }
    fn get_work_by_id(&self, work_id: &str) -> Result<Option<WorkResult>, HistoryPortError> {
        self.repository.get_work_by_id(work_id).map_err(map_err)
    }
    fn get_work(&self, title: &str, limit: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
        self.repository.get_work(title, limit).map_err(map_err)
    }
    fn get_historical_texts(&self, work: Option<&str>, limit: i64) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
        self.repository.get_historical_texts(work, limit).map_err(map_err)
    }
    fn search_people(&self, query: &str, limit: i64) -> Result<Vec<PersonResult>, HistoryPortError> {
        self.repository.search_people(query, limit).map_err(map_err)
    }
    fn search_events(&self, query: &str, limit: i64) -> Result<Vec<EventResult>, HistoryPortError> {
        self.repository.search_events(query, limit).map_err(map_err)
    }
    fn get_sources_for_ids(&self, ids: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
        self.repository.get_sources_for_ids(ids).map_err(map_err)
    }
}