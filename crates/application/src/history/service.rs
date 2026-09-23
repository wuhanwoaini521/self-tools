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

/// Period Detail 的历史阶段（data-driven 分段）。
///
/// 由本时期事件的 `importance == "critical"` 锚点驱动：每个阶段以关键事件
/// 开启，延伸到下一个锚点年前一年（末段延伸到时期结束）。锚点年份相距 ≤2 年时
/// 合并为同一章。标题与叙事链完全由真实事件名组成，不引入硬编码历史叙述。
#[derive(Debug, Serialize)]
pub struct HistoryPeriodStage {
    pub index: i64,
    pub start_year: i32,
    pub end_year: i32,
    pub opening_event_id: String,
    pub opening_event_name: String,
    pub opening_event_type: Option<String>,
    pub event_count: i64,
}

#[derive(Debug, Serialize)]
pub struct HistorySemanticPeriodDetail {
    pub period: PeriodResult,
    pub regimes: Vec<RegimeResult>,
    pub stories: Vec<StoryResult>,
    pub events: Vec<PeriodEventItem>,
    pub people: Vec<PeriodPersonItem>,
    /// data-driven 历史阶段（空 = 该时期缺少足够锚点，前端隐藏阶段模块）。
    pub stages: Vec<HistoryPeriodStage>,
    /// 时期关系池：至少一端属于本时期的事件关系（含事件名）。
    pub relations: Vec<EventRelationResult>,
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

/// 关键锚点：`importance == "critical"` 且有确定年份的事件，按年份升序。
fn critical_anchors(events: &[PeriodEventItem]) -> Vec<&PeriodEventItem> {
    let mut anchors = events
        .iter()
        .filter(|event| event.importance.as_deref() == Some("critical"))
        .filter_map(|event| event.start_year.map(|year| (year, event)))
        .collect::<Vec<_>>();
    anchors.sort_by_key(|item| item.0);
    anchors.into_iter().map(|item| item.1).collect()
}

/// data-driven 阶段分段（详见 `HistoryPeriodStage` 注释）：
/// 1) 只取 critical 锚点；2) 年份相距 ≤2 年的锚点合并为一章（取最早者）；
/// 3) 每章从锚点年延伸到下一锚点年前一年（末章到时期结束）；
/// 4) 首个锚点前的空档（如有真实事件）作为单独「开端章」。
///
/// 锚点不足（<2）时返回空 —— 前端据此隐藏阶段模块。
fn derive_stages(
    events: &[PeriodEventItem],
    period_start: Option<i32>,
    period_end: Option<i32>,
) -> Vec<HistoryPeriodStage> {
    let mut anchors = critical_anchors(events);
    if anchors.len() < 2 {
        return Vec::new();
    }
    // 合并同年/相邻年锚点：窗口内保留最早者。
    let mut merged: Vec<&PeriodEventItem> = Vec::new();
    for anchor in anchors.drain(..) {
        let Some(year) = anchor.start_year else {
            continue;
        };
        if let Some(last) = merged.last()
            && year - last.start_year.unwrap_or(i32::MIN) <= 2
        {
            continue;
        }
        merged.push(anchor);
    }

    let span = |from: i32, to: i32| -> i64 {
        events
            .iter()
            .filter(|event| {
                event
                    .start_year
                    .is_some_and(|year| (from..=to).contains(&year))
            })
            .count() as i64
    };
    let mut stages = Vec::new();
    let mut index = 0i64;
    let Some(first_year) = merged[0].start_year else {
        return Vec::new();
    };
    // 开端章：时期开始到首个锚点前，且确有事件存在。
    if let Some(start) = period_start
        && start < first_year
        && span(start, first_year - 1) > 0
    {
        index += 1;
        stages.push(HistoryPeriodStage {
            index,
            start_year: start,
            end_year: first_year - 1,
            opening_event_id: String::new(),
            opening_event_name: String::new(),
            opening_event_type: None,
            event_count: span(start, first_year - 1),
        });
    }
    for (position, anchor) in merged.iter().enumerate() {
        let Some(start) = anchor.start_year else {
            continue;
        };
        let end = match merged.get(position + 1) {
            Some(next) => next
                .start_year
                .map(|year| (year - 1).max(start))
                .unwrap_or(start),
            None => period_end.filter(|end| *end >= start).unwrap_or(start),
        };
        index += 1;
        stages.push(HistoryPeriodStage {
            index,
            start_year: start,
            end_year: end,
            opening_event_id: anchor.id.clone(),
            opening_event_name: anchor.name_zh_cn.clone(),
            opening_event_type: anchor.event_type.clone(),
            event_count: span(start, end),
        });
    }
    stages
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
            stats: self
                .port
                .get_dataset_stats()
                .map_err(ApplicationError::History)?,
        })
    }

    /// 时期详情：按 id 解析时期，再装配朝代、故事、事件、核心人物，
    /// 以及 data-driven 历史阶段与时期关系池。
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
        let events = self
            .port
            .get_events_for_period(period_id)
            .map_err(ApplicationError::History)?;
        let stages = derive_stages(&events, period.start_year, period.end_year);
        let relations = self
            .port
            .get_relations_for_period(period_id)
            .map_err(ApplicationError::History)?;
        Ok(Some(HistorySemanticPeriodDetail {
            regimes: self
                .port
                .get_regimes_by_period(period_id)
                .map_err(ApplicationError::History)?,
            stories: self
                .port
                .get_stories_for_period(Some(period_id))
                .map_err(ApplicationError::History)?,
            events,
            people: self
                .port
                .get_people_for_period(period_id, PERIOD_PEOPLE_LIMIT)
                .map_err(ApplicationError::History)?,
            period,
            stages,
            relations,
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
