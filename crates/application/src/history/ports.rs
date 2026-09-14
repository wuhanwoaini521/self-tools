//! History 只读查询端口。
//!
//! 该 trait 是 History 全部现有 Use Case 实际需要的查询面：
//! 从 7 个命令（home / period / story / event / person / work / search）的调用链反推，
//! 不包含任何写入、连接管理或路径能力。DuckDB 行映射结果类型由
//! `devtoolbox_core::history_records` 提供（应用层与基础设施层共用，不感知 SQL / DuckDB），
//! 错误类型是应用端口自有的 `HistoryQueryError`，由平台适配层把基础设施错误转换进来。

use std::fmt;

use devtoolbox_core::history_records::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
    PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
    PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
    WorkResult,
};

/// History 查询端口的错误（适配层负责把基础设施错误转换为可显示文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPortError(pub String);

impl fmt::Display for HistoryPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HistoryPortError {}

/// History 知识库（只读查询面）的端口契约。
/// `Send + Sync`：用例服务会被跨线程共享（桌面 Tauri / HTTP 服务 State）。
pub trait HistoryQueryPort: Send + Sync {
    fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError>;
    fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError>;
    fn get_regimes_by_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<RegimeResult>, HistoryPortError>;
    fn get_events_for_period(
        &self,
        period_id: &str,
    ) -> Result<Vec<PeriodEventItem>, HistoryPortError>;
    fn get_people_for_period(
        &self,
        period_id: &str,
        limit: i64,
    ) -> Result<Vec<PeriodPersonItem>, HistoryPortError>;
    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError>;
    fn get_stories_for_period(
        &self,
        period_id: Option<&str>,
    ) -> Result<Vec<StoryResult>, HistoryPortError>;
    fn get_story(&self, story_id: &str) -> Result<Option<StoryResult>, HistoryPortError>;
    fn get_story_events(
        &self,
        story_id: &str,
    ) -> Result<Vec<StoryEventResult>, HistoryPortError>;
    fn get_story_people(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventPersonResult>, HistoryPortError>;
    fn get_story_places(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventPlaceResult>, HistoryPortError>;
    fn get_story_texts(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError>;
    fn get_story_evidences(
        &self,
        story_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError>;
    fn get_event(&self, event_id: &str) -> Result<Option<EventResult>, HistoryPortError>;
    fn get_event_people(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventPersonResult>, HistoryPortError>;
    fn get_event_places(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventPlaceResult>, HistoryPortError>;
    fn get_event_relations(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError>;
    fn get_event_texts(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError>;
    fn get_event_evidences(
        &self,
        event_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError>;
    fn get_person(&self, person_id: &str) -> Result<Option<PersonResult>, HistoryPortError>;
    fn get_person_relations(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonRelationResult>, HistoryPortError>;
    fn get_person_places(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonPlaceResult>, HistoryPortError>;
    fn get_person_events(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonEventResult>, HistoryPortError>;
    fn get_person_stories(
        &self,
        person_id: &str,
    ) -> Result<Vec<PersonStoryResult>, HistoryPortError>;
    fn get_work_by_id(&self, work_id: &str) -> Result<Option<WorkResult>, HistoryPortError>;
    fn get_work(&self, title: &str, limit: i64) -> Result<Vec<WorkResult>, HistoryPortError>;
    fn get_historical_texts(
        &self,
        work: Option<&str>,
        limit: i64,
    ) -> Result<Vec<HistoricalTextResult>, HistoryPortError>;
    fn search_people(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<PersonResult>, HistoryPortError>;
    fn search_events(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<EventResult>, HistoryPortError>;
    fn get_sources_for_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<SourceResult>, HistoryPortError>;
}