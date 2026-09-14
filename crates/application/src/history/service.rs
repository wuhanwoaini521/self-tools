//! History V2 只读知识库的用例编排服务。
//!
//! 视图模型与序列化字段名是前端 JSON 契约（HistoryPage 的 `semanticTypes.ts`），
//! **不得改名或增删字段**；本文件只移动逻辑，不改变行为。

use std::collections::BTreeSet;

use devtoolbox_core::history_records::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
    PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
    PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
    WorkResult,
};
use serde::Serialize;

use super::ports::HistoryQueryPort;
use crate::ApplicationError;

// ---------- 视图模型（Tauri command 直接序列化回前端） ----------

#[derive(Debug, Serialize)]
pub struct HistorySemanticHome {
    pub periods: Vec<PeriodResult>,
    pub stories: Vec<StoryResult>,
    pub stats: DatasetStats,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticPeriodDetail {
    pub period: PeriodResult,
    pub regimes: Vec<RegimeResult>,
    pub stories: Vec<StoryResult>,
    pub events: Vec<PeriodEventItem>,
    pub people: Vec<PeriodPersonItem>,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticStoryDetail {
    pub story: StoryResult,
    pub events: Vec<StoryEventResult>,
    pub people: Vec<EventPersonResult>,
    pub places: Vec<EventPlaceResult>,
    pub historical_texts: Vec<EventHistoricalTextResult>,
    pub evidences: Vec<EventEvidenceResult>,
    pub sources: Vec<SourceResult>,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticEventDetail {
    pub event: EventResult,
    pub people: Vec<EventPersonResult>,
    pub places: Vec<EventPlaceResult>,
    pub relations: Vec<EventRelationResult>,
    pub historical_texts: Vec<EventHistoricalTextResult>,
    pub evidences: Vec<EventEvidenceResult>,
    pub sources: Vec<SourceResult>,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticPersonDetail {
    pub person: PersonResult,
    pub relations: Vec<PersonRelationResult>,
    pub places: Vec<PersonPlaceResult>,
    pub events: Vec<PersonEventResult>,
    pub stories: Vec<PersonStoryResult>,
    pub sources: Vec<SourceResult>,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticWorkDetail {
    pub work: WorkResult,
    pub texts: Vec<HistoricalTextResult>,
    pub sources: Vec<SourceResult>,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticSearchHit {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub start_year: Option<i32>,
    pub end_year: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticSearchGroup {
    pub kind: String,
    pub items: Vec<HistorySemanticSearchHit>,
}

// ---------- 用例编排 ----------

/// 解析行内 JSON 数组（`"[\"id1\",\"id2\"]"`）；解析失败视为空。
fn semantic_ids(value: &Option<String>) -> Vec<String> {
    value
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default()
}

/// 从 story 详情页的各关联数据中合并全部来源 ID（去重、排序）。
fn semantic_source_ids(
    story: Option<&StoryResult>,
    events: &[StoryEventResult],
    people: &[EventPersonResult],
    places: &[EventPlaceResult],
    texts: &[EventHistoricalTextResult],
    evidences: &[EventEvidenceResult],
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    if let Some(story) = story {
        ids.extend(semantic_ids(&story.source_ids));
    }
    for event in events {
        ids.extend(semantic_ids(&event.source_ids));
    }
    ids.extend(people.iter().filter_map(|item| item.source_id.clone()));
    ids.extend(places.iter().filter_map(|item| item.source_id.clone()));
    ids.extend(texts.iter().filter_map(|item| item.source_id.clone()));
    ids.extend(evidences.iter().filter_map(|item| item.source_id.clone()));
    ids.into_iter().collect()
}

pub struct HistoryService {
    port: Box<dyn HistoryQueryPort>,
}

impl HistoryService {
    pub fn new(port: Box<dyn HistoryQueryPort>) -> Self {
        Self { port }
    }

    /// 首页：时期、故事与数据集统计。
    pub fn home(&self) -> Result<HistorySemanticHome, ApplicationError> {
        Ok(HistorySemanticHome {
            periods: self.port.get_periods().map_err(ApplicationError::History)?,
            stories: self.port.get_stories().map_err(ApplicationError::History)?,
            stats: self.port.get_dataset_stats().map_err(ApplicationError::History)?,
        })
    }

    /// 时期详情：按 id 解析时期，再装配朝代、故事、事件与核心人物。
    pub fn period_detail(
        &self,
        period_id: &str,
    ) -> Result<Option<HistorySemanticPeriodDetail>, ApplicationError> {
        let period = self
            .port
            .get_periods()
            .map_err(ApplicationError::History)?
            .into_iter()
            .find(|item| item.id == period_id);
        let Some(period) = period else {
            return Ok(None);
        };
        Ok(Some(HistorySemanticPeriodDetail {
            regimes: self
                .port
                .get_regimes_by_period(period_id)
                .map_err(ApplicationError::History)?,
            stories: self
                .port
                .get_stories_for_period(Some(period_id))
                .map_err(ApplicationError::History)?,
            events: self
                .port
                .get_events_for_period(period_id)
                .map_err(ApplicationError::History)?,
            people: self
                .port
                .get_people_for_period(period_id, PERIOD_PEOPLE_LIMIT)
                .map_err(ApplicationError::History)?,
            period,
        }))
    }

    /// 故事详情：故事 + 事件 + 人物 + 地点 + 原文节选 + 证据，并合并全部来源。
    pub fn story_detail(
        &self,
        story_id: &str,
    ) -> Result<Option<HistorySemanticStoryDetail>, ApplicationError> {
        let Some(story) = self
            .port
            .get_story(story_id)
            .map_err(ApplicationError::History)?
        else {
            return Ok(None);
        };
        let events = self
            .port
            .get_story_events(&story.id)
            .map_err(ApplicationError::History)?;
        let people = self
            .port
            .get_story_people(&story.id)
            .map_err(ApplicationError::History)?;
        let places = self
            .port
            .get_story_places(&story.id)
            .map_err(ApplicationError::History)?;
        let historical_texts = self
            .port
            .get_story_texts(&story.id)
            .map_err(ApplicationError::History)?;
        let evidences = self
            .port
            .get_story_evidences(&story.id)
            .map_err(ApplicationError::History)?;
        let ids = semantic_source_ids(
            Some(&story),
            &events,
            &people,
            &places,
            &historical_texts,
            &evidences,
        );
        let sources = self
            .port
            .get_sources_for_ids(&ids)
            .map_err(ApplicationError::History)?;
        Ok(Some(HistorySemanticStoryDetail {
            story,
            events,
            people,
            places,
            historical_texts,
            evidences,
            sources,
        }))
    }

    /// 事件详情：事件 + 人物 + 地点 + 关系 + 原文节选 + 证据，并合并全部来源 ID。
    pub fn event_detail(
        &self,
        event_id: &str,
    ) -> Result<Option<HistorySemanticEventDetail>, ApplicationError> {
        let Some(event) = self
            .port
            .get_event(event_id)
            .map_err(ApplicationError::History)?
        else {
            return Ok(None);
        };
        let people = self
            .port
            .get_event_people(&event.id)
            .map_err(ApplicationError::History)?;
        let places = self
            .port
            .get_event_places(&event.id)
            .map_err(ApplicationError::History)?;
        let relations = self
            .port
            .get_event_relations(&event.id)
            .map_err(ApplicationError::History)?;
        let historical_texts = self
            .port
            .get_event_texts(&event.id)
            .map_err(ApplicationError::History)?;
        let evidences = self
            .port
            .get_event_evidences(&event.id)
            .map_err(ApplicationError::History)?;
        let mut ids = BTreeSet::new();
        ids.extend(semantic_ids(&event.source_ids));
        ids.extend(people.iter().filter_map(|item| item.source_id.clone()));
        ids.extend(places.iter().filter_map(|item| item.source_id.clone()));
        ids.extend(relations.iter().filter_map(|item| item.source_id.clone()));
        ids.extend(
            historical_texts
                .iter()
                .filter_map(|item| item.source_id.clone()),
        );
        ids.extend(evidences.iter().filter_map(|item| item.source_id.clone()));
        let sources = self
            .port
            .get_sources_for_ids(&ids.into_iter().collect::<Vec<_>>())
            .map_err(ApplicationError::History)?;
        Ok(Some(HistorySemanticEventDetail {
            event,
            people,
            places,
            relations,
            historical_texts,
            evidences,
            sources,
        }))
    }

    /// 人物详情：人物 + 关系 + 地点 + 事件 + 故事，并合并全部来源 ID。
    pub fn person_detail(
        &self,
        person_id: &str,
    ) -> Result<Option<HistorySemanticPersonDetail>, ApplicationError> {
        let Some(person) = self
            .port
            .get_person(person_id)
            .map_err(ApplicationError::History)?
        else {
            return Ok(None);
        };
        let relations = self
            .port
            .get_person_relations(&person.id)
            .map_err(ApplicationError::History)?;
        let places = self
            .port
            .get_person_places(&person.id)
            .map_err(ApplicationError::History)?;
        let events = self
            .port
            .get_person_events(&person.id)
            .map_err(ApplicationError::History)?;
        let stories = self
            .port
            .get_person_stories(&person.id)
            .map_err(ApplicationError::History)?;
        let mut ids = BTreeSet::new();
        if let Some(id) = person.created_from_source.clone() {
            ids.insert(id);
        }
        ids.extend(
            relations
                .iter()
                .flat_map(|item| semantic_ids(&item.source_ids)),
        );
        ids.extend(places.iter().map(|item| item.source_id.clone()));
        let sources = self
            .port
            .get_sources_for_ids(&ids.into_iter().collect::<Vec<_>>())
            .map_err(ApplicationError::History)?;
        Ok(Some(HistorySemanticPersonDetail {
            person,
            relations,
            places,
            events,
            stories,
            sources,
        }))
    }

    /// 作品详情：作品 + 原文节选（按作品标题查询，固定上限），并合并来源 ID。
    pub fn work_detail(
        &self,
        work_id: &str,
    ) -> Result<Option<HistorySemanticWorkDetail>, ApplicationError> {
        let Some(work) = self
            .port
            .get_work_by_id(work_id)
            .map_err(ApplicationError::History)?
        else {
            return Ok(None);
        };
        let texts = self
            .port
            .get_historical_texts(Some(&work.title), WORK_TEXTS_LIMIT)
            .map_err(ApplicationError::History)?;
        let mut ids = BTreeSet::new();
        if let Some(id) = work.source_id.clone() {
            ids.insert(id);
        }
        ids.extend(texts.iter().filter_map(|item| item.source_id.clone()));
        let sources = self
            .port
            .get_sources_for_ids(&ids.into_iter().collect::<Vec<_>>())
            .map_err(ApplicationError::History)?;
        Ok(Some(HistorySemanticWorkDetail {
            work,
            texts,
            sources,
        }))
    }

    /// 语义搜索：按实体类型分组（人物 / 故事 / 事件 / 作品），每组最多 8 条。
    /// 空查询直接返回空列表，不触碰数据源。
    pub fn search(&self, query: &str) -> Result<Vec<HistorySemanticSearchGroup>, ApplicationError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let people = self
            .port
            .search_people(query, SEARCH_LIMIT)
            .map_err(ApplicationError::History)?;
        let stories = self
            .port
            .get_stories()
            .map_err(ApplicationError::History)?
            .into_iter()
            .filter(|item| {
                item.title_zh_cn.contains(query)
                    || item
                        .summary_zh_cn
                        .as_deref()
                        .is_some_and(|text| text.contains(query))
            })
            .take(SEARCH_LIMIT as usize)
            .collect::<Vec<_>>();
        let events = self
            .port
            .search_events(query, SEARCH_LIMIT)
            .map_err(ApplicationError::History)?;
        let works = self
            .port
            .get_work(query, SEARCH_LIMIT)
            .map_err(ApplicationError::History)?;
        let mut groups = Vec::new();
        if !people.is_empty() {
            groups.push(HistorySemanticSearchGroup {
                kind: "person".into(),
                items: people
                    .into_iter()
                    .map(|item| HistorySemanticSearchHit {
                        id: item.id,
                        kind: "person".into(),
                        title: item.canonical_name_zh_cn,
                        subtitle: item.intro_zh_cn,
                        start_year: item.birth_year,
                        end_year: item.death_year,
                    })
                    .collect(),
            });
        }
        if !stories.is_empty() {
            groups.push(HistorySemanticSearchGroup {
                kind: "story".into(),
                items: stories
                    .into_iter()
                    .map(|item| HistorySemanticSearchHit {
                        id: item.id,
                        kind: "story".into(),
                        title: item.title_zh_cn,
                        subtitle: item.summary_zh_cn,
                        start_year: item.start_year,
                        end_year: item.end_year,
                    })
                    .collect(),
            });
        }
        if !events.is_empty() {
            groups.push(HistorySemanticSearchGroup {
                kind: "event".into(),
                items: events
                    .into_iter()
                    .map(|item| HistorySemanticSearchHit {
                        id: item.id,
                        kind: "event".into(),
                        title: item.name_zh_cn,
                        subtitle: item.summary_zh_cn,
                        start_year: item.start_year,
                        end_year: item.end_year,
                    })
                    .collect(),
            });
        }
        if !works.is_empty() {
            groups.push(HistorySemanticSearchGroup {
                kind: "work".into(),
                items: works
                    .into_iter()
                    .map(|item| HistorySemanticSearchHit {
                        id: item.id,
                        kind: "work".into(),
                        title: item.title_zh_cn.unwrap_or(item.title),
                        subtitle: None,
                        start_year: None,
                        end_year: None,
                    })
                    .collect(),
            });
        }
        Ok(groups)
    }
}

// 与 Cutover 前行为一致的固定上限。
const SEARCH_LIMIT: i64 = 8;
const PERIOD_PEOPLE_LIMIT: i64 = 24;
const WORK_TEXTS_LIMIT: i64 = 200;