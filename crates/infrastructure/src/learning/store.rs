//! Learning OS SQLite 存储（`learning.db`）。
//!
//! 统一管理：
//! - `learning_events`
//! - `learning_progress`
//! - `review_cards`
//! - `collections` & `collection_items`
//! - `custom_relations`

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use devtoolbox_core::learning::{
    Collection, CollectionItem, ContinueItem, LearningAction, LearningEvent, LearningProgress,
    LearningStatus, MasteryCalculator, ReviewCardType, ReviewQueueItem, ReviewQueueStats,
    ReviewRating, ReviewScheduleOutcome, SpacedRepetitionScheduler, UniversalReviewCard,
};

use crate::error::InfrastructureError;
use crate::sqlite::{configure_sqlite, open_sqlite};

pub struct LearningStore {
    connection: Connection,
}

fn sqlite_err(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

impl LearningStore {
    /// 打开或创建 SQLite 学习数据库。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = open_sqlite(path).map_err(sqlite_err)?;
        let store = Self { connection };
        store.ensure_schema()?;
        Ok(store)
    }

    /// 在内存中打开（供测试用）。
    pub fn open_in_memory() -> Result<Self, InfrastructureError> {
        let connection = Connection::open_in_memory().map_err(sqlite_err)?;
        configure_sqlite(&connection).map_err(sqlite_err)?;
        let store = Self { connection };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS learning_events (
                    id TEXT PRIMARY KEY,
                    module TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    entity_title TEXT,
                    action TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    duration_ms INTEGER,
                    metadata_json TEXT,
                    source TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_events_time ON learning_events(timestamp DESC);
                CREATE INDEX IF NOT EXISTS idx_events_entity ON learning_events(module, entity_type, entity_id);

                CREATE TABLE IF NOT EXISTS learning_progress (
                    entity_key TEXT PRIMARY KEY,
                    module TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    entity_title TEXT NOT NULL,
                    status TEXT NOT NULL,
                    study_count INTEGER NOT NULL DEFAULT 0,
                    review_count INTEGER NOT NULL DEFAULT 0,
                    correct_count INTEGER NOT NULL DEFAULT 0,
                    incorrect_count INTEGER NOT NULL DEFAULT 0,
                    mastery_score REAL NOT NULL DEFAULT 0.0,
                    last_studied_at INTEGER NOT NULL,
                    next_review_at INTEGER,
                    interval_days REAL NOT NULL DEFAULT 0.0,
                    ease REAL NOT NULL DEFAULT 2.5,
                    custom_tags_json TEXT NOT NULL DEFAULT '[]'
                );
                CREATE INDEX IF NOT EXISTS idx_progress_module ON learning_progress(module, status);
                CREATE INDEX IF NOT EXISTS idx_progress_next_review ON learning_progress(next_review_at);
                CREATE INDEX IF NOT EXISTS idx_progress_mastery ON learning_progress(mastery_score DESC);

                CREATE TABLE IF NOT EXISTS review_cards (
                    id TEXT PRIMARY KEY,
                    module TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    card_type TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    answer TEXT NOT NULL,
                    options_json TEXT,
                    hint TEXT,
                    context TEXT,
                    due_at INTEGER NOT NULL,
                    interval_days REAL NOT NULL DEFAULT 0.0,
                    ease REAL NOT NULL DEFAULT 2.5,
                    mastery_score REAL NOT NULL DEFAULT 0.0,
                    repetition_count INTEGER NOT NULL DEFAULT 0,
                    lapses INTEGER NOT NULL DEFAULT 0,
                    last_reviewed_at INTEGER,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_review_cards_due ON review_cards(due_at ASC);
                CREATE INDEX IF NOT EXISTS idx_review_cards_module_due ON review_cards(module, due_at ASC);
                CREATE INDEX IF NOT EXISTS idx_review_cards_entity ON review_cards(module, entity_type, entity_id);

                CREATE TABLE IF NOT EXISTS collections (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    description TEXT,
                    tags_json TEXT NOT NULL DEFAULT '[]',
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS collection_items (
                    id TEXT PRIMARY KEY,
                    collection_id TEXT NOT NULL,
                    module TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    note TEXT,
                    added_at INTEGER NOT NULL,
                    FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_col_items ON collection_items(collection_id);

                CREATE TABLE IF NOT EXISTS custom_relations (
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    relation_kind TEXT NOT NULL,
                    label TEXT NOT NULL,
                    weight REAL NOT NULL DEFAULT 1.0,
                    source_module TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_rel_src ON custom_relations(source_id);
                CREATE INDEX IF NOT EXISTS idx_rel_tgt ON custom_relations(target_id);",
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    // ========================================================================
    // 1. Learning Events & Progress
    // ========================================================================

    /// 记录学习事件，并原子更新进度与掌握度。
    pub fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, InfrastructureError> {
        let metadata_str = serde_json::to_string(&event.metadata).unwrap_or_else(|_| "{}".to_string());

        // 1. 插入事件
        self.connection.execute(
            "INSERT INTO learning_events (
                id, module, entity_type, entity_id, entity_title, action, timestamp, duration_ms, metadata_json, source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                event.id,
                event.module,
                event.entity_type,
                event.entity_id,
                event.entity_title,
                event.action.as_str(),
                event.timestamp,
                event.duration_ms,
                metadata_str,
                event.source,
            ],
        ).map_err(sqlite_err)?;

        // 2. 读取或初始化 Progress
        let entity_key = format!("{}:{}:{}", event.module, event.entity_type, event.entity_id);
        let mut progress = self.get_progress(&entity_key)?.unwrap_or_else(|| {
            LearningProgress::new(
                &event.module,
                &event.entity_type,
                &event.entity_id,
                event.entity_title.clone().unwrap_or_else(|| event.entity_id.clone()),
                event.timestamp,
            )
        });

        // 3. 更新统计
        match &event.action {
            LearningAction::Study | LearningAction::View | LearningAction::Complete | LearningAction::Note | LearningAction::Bookmark => {
                progress.study_count += 1;
            }
            LearningAction::Review => {
                progress.review_count += 1;
            }
            LearningAction::Correct => {
                progress.review_count += 1;
                progress.correct_count += 1;
            }
            LearningAction::Incorrect => {
                progress.review_count += 1;
                progress.incorrect_count += 1;
            }
            LearningAction::Answer | LearningAction::AskAi | LearningAction::Custom(_) => {
                progress.study_count += 1;
            }
        }

        progress.last_studied_at = event.timestamp;
        if let Some(title) = &event.entity_title {
            if !title.is_empty() {
                progress.entity_title = title.clone();
            }
        }

        // 4. 计算掌握度
        let (new_score, new_status) = MasteryCalculator::calculate(
            progress.study_count,
            progress.correct_count,
            progress.incorrect_count,
            progress.interval_days,
            progress.last_studied_at,
            event.timestamp,
        );
        progress.mastery_score = new_score;
        progress.status = new_status;

        // 5. 保存 Progress
        self.save_progress(&progress)?;

        Ok(progress)
    }

    /// 获取实体进度。
    pub fn get_progress(&self, entity_key: &str) -> Result<Option<LearningProgress>, InfrastructureError> {
        let mut stmt = self.connection.prepare(
            "SELECT entity_key, module, entity_type, entity_id, entity_title, status,
                    study_count, review_count, correct_count, incorrect_count, mastery_score,
                    last_studied_at, next_review_at, interval_days, ease, custom_tags_json
             FROM learning_progress WHERE entity_key = ?1",
        ).map_err(sqlite_err)?;

        let row = stmt.query_row(params![entity_key], |row| {
            let status_str: String = row.get(5)?;
            let tags_str: String = row.get(15)?;
            let custom_tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

            Ok(LearningProgress {
                entity_key: row.get(0)?,
                module: row.get(1)?,
                entity_type: row.get(2)?,
                entity_id: row.get(3)?,
                entity_title: row.get(4)?,
                status: LearningStatus::parse(&status_str),
                study_count: row.get(6)?,
                review_count: row.get(7)?,
                correct_count: row.get(8)?,
                incorrect_count: row.get(9)?,
                mastery_score: row.get(10)?,
                last_studied_at: row.get(11)?,
                next_review_at: row.get(12)?,
                interval_days: row.get(13)?,
                ease: row.get(14)?,
                custom_tags,
            })
        }).optional().map_err(sqlite_err)?;

        Ok(row)
    }

    /// 保存或更新 Progress。
    pub fn save_progress(&self, progress: &LearningProgress) -> Result<(), InfrastructureError> {
        let tags_str = serde_json::to_string(&progress.custom_tags).unwrap_or_else(|_| "[]".to_string());

        self.connection.execute(
            "INSERT INTO learning_progress (
                entity_key, module, entity_type, entity_id, entity_title, status,
                study_count, review_count, correct_count, incorrect_count, mastery_score,
                last_studied_at, next_review_at, interval_days, ease, custom_tags_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(entity_key) DO UPDATE SET
                entity_title = excluded.entity_title,
                status = excluded.status,
                study_count = excluded.study_count,
                review_count = excluded.review_count,
                correct_count = excluded.correct_count,
                incorrect_count = excluded.incorrect_count,
                mastery_score = excluded.mastery_score,
                last_studied_at = excluded.last_studied_at,
                next_review_at = excluded.next_review_at,
                interval_days = excluded.interval_days,
                ease = excluded.ease,
                custom_tags_json = excluded.custom_tags_json",
            params![
                progress.entity_key,
                progress.module,
                progress.entity_type,
                progress.entity_id,
                progress.entity_title,
                progress.status.as_str(),
                progress.study_count,
                progress.review_count,
                progress.correct_count,
                progress.incorrect_count,
                progress.mastery_score,
                progress.last_studied_at,
                progress.next_review_at,
                progress.interval_days,
                progress.ease,
                tags_str,
            ],
        ).map_err(sqlite_err)?;

        Ok(())
    }

    /// 列出学习进度列表。
    pub fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, InfrastructureError> {
        let query = "SELECT entity_key, module, entity_type, entity_id, entity_title, status,
                            study_count, review_count, correct_count, incorrect_count, mastery_score,
                            last_studied_at, next_review_at, interval_days, ease, custom_tags_json
                     FROM learning_progress
                     WHERE (:module IS NULL OR module = :module)
                       AND (:status IS NULL OR status = :status)
                     ORDER BY last_studied_at DESC LIMIT :limit";

        let mut stmt = self.connection.prepare(query).map_err(sqlite_err)?;
        let mut rows = stmt.query(rusqlite::named_params! {
            ":module": module_filter,
            ":status": status_filter.map(|s| s.as_str()),
            ":limit": limit as i64,
        }).map_err(sqlite_err)?;

        let mut list = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            let status_str: String = row.get(5).map_err(sqlite_err)?;
            let tags_str: String = row.get(15).map_err(sqlite_err)?;
            list.push(LearningProgress {
                entity_key: row.get(0).map_err(sqlite_err)?,
                module: row.get(1).map_err(sqlite_err)?,
                entity_type: row.get(2).map_err(sqlite_err)?,
                entity_id: row.get(3).map_err(sqlite_err)?,
                entity_title: row.get(4).map_err(sqlite_err)?,
                status: LearningStatus::parse(&status_str),
                study_count: row.get(6).map_err(sqlite_err)?,
                review_count: row.get(7).map_err(sqlite_err)?,
                correct_count: row.get(8).map_err(sqlite_err)?,
                incorrect_count: row.get(9).map_err(sqlite_err)?,
                mastery_score: row.get(10).map_err(sqlite_err)?,
                last_studied_at: row.get(11).map_err(sqlite_err)?,
                next_review_at: row.get(12).map_err(sqlite_err)?,
                interval_days: row.get(13).map_err(sqlite_err)?,
                ease: row.get(14).map_err(sqlite_err)?,
                custom_tags: serde_json::from_str(&tags_str).unwrap_or_default(),
            });
        }

        Ok(list)
    }

    // ========================================================================
    // 2. Review Center & Spaced Repetition
    // ========================================================================

    /// 添加或更新通用复习卡片。
    pub fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), InfrastructureError> {
        let options_str = card.options.as_ref().map(|opts| serde_json::to_string(opts).unwrap_or_default());
        let card_type_str = match card.card_type {
            ReviewCardType::Recall => "recall",
            ReviewCardType::MultipleChoice => "multiple_choice",
            ReviewCardType::Qa => "qa",
            ReviewCardType::MapLocate => "map_locate",
            ReviewCardType::FillBlank => "fill_blank",
        };

        self.connection.execute(
            "INSERT INTO review_cards (
                id, module, entity_id, entity_type, card_type, prompt, answer, options_json,
                hint, context, due_at, interval_days, ease, mastery_score, repetition_count,
                lapses, last_reviewed_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(id) DO UPDATE SET
                prompt = excluded.prompt,
                answer = excluded.answer,
                options_json = excluded.options_json,
                hint = excluded.hint,
                context = excluded.context,
                due_at = excluded.due_at,
                interval_days = excluded.interval_days,
                ease = excluded.ease,
                mastery_score = excluded.mastery_score,
                repetition_count = excluded.repetition_count,
                lapses = excluded.lapses,
                last_reviewed_at = excluded.last_reviewed_at",
            params![
                card.id,
                card.module,
                card.entity_id,
                card.entity_type,
                card_type_str,
                card.prompt,
                card.answer,
                options_str,
                card.hint,
                card.context,
                card.due_at,
                card.interval_days,
                card.ease,
                card.mastery_score,
                card.repetition_count,
                card.lapses,
                card.last_reviewed_at,
                card.created_at,
            ],
        ).map_err(sqlite_err)?;

        Ok(())
    }

    /// 获取指定卡片。
    pub fn get_review_card(&self, card_id: &str) -> Result<Option<UniversalReviewCard>, InfrastructureError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, module, entity_id, entity_type, card_type, prompt, answer, options_json,
                    hint, context, due_at, interval_days, ease, mastery_score, repetition_count,
                    lapses, last_reviewed_at, created_at
             FROM review_cards WHERE id = ?1",
        ).map_err(sqlite_err)?;

        let row = stmt.query_row(params![card_id], |row| {
            let card_type_str: String = row.get(4)?;
            let card_type = match card_type_str.as_str() {
                "multiple_choice" => ReviewCardType::MultipleChoice,
                "qa" => ReviewCardType::Qa,
                "map_locate" => ReviewCardType::MapLocate,
                "fill_blank" => ReviewCardType::FillBlank,
                _ => ReviewCardType::Recall,
            };
            let opts_str: Option<String> = row.get(7)?;
            let options = opts_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(UniversalReviewCard {
                id: row.get(0)?,
                module: row.get(1)?,
                entity_id: row.get(2)?,
                entity_type: row.get(3)?,
                card_type,
                prompt: row.get(5)?,
                answer: row.get(6)?,
                options,
                hint: row.get(8)?,
                context: row.get(9)?,
                due_at: row.get(10)?,
                interval_days: row.get(11)?,
                ease: row.get(12)?,
                mastery_score: row.get(13)?,
                repetition_count: row.get(14)?,
                lapses: row.get(15)?,
                last_reviewed_at: row.get(16)?,
                created_at: row.get(17)?,
            })
        }).optional().map_err(sqlite_err)?;

        Ok(row)
    }

    /// 查询待复习队列（按到期时间排序）。
    pub fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, InfrastructureError> {
        let query = "SELECT id, module, entity_id, entity_type, card_type, prompt, answer, options_json,
                            hint, context, due_at, interval_days, ease, mastery_score, repetition_count,
                            lapses, last_reviewed_at, created_at
                     FROM review_cards
                     WHERE due_at <= :now
                       AND (:module IS NULL OR :module = 'all' OR module = :module)
                     ORDER BY due_at ASC, mastery_score ASC LIMIT :limit";

        let mut stmt = self.connection.prepare(query).map_err(sqlite_err)?;
        let mut rows = stmt.query(rusqlite::named_params! {
            ":now": now,
            ":module": module_filter,
            ":limit": limit as i64,
        }).map_err(sqlite_err)?;

        let mut items = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            let card_type_str: String = row.get(4).map_err(sqlite_err)?;
            let card_type = match card_type_str.as_str() {
                "multiple_choice" => ReviewCardType::MultipleChoice,
                "qa" => ReviewCardType::Qa,
                "map_locate" => ReviewCardType::MapLocate,
                "fill_blank" => ReviewCardType::FillBlank,
                _ => ReviewCardType::Recall,
            };
            let opts_str: Option<String> = row.get(7).map_err(sqlite_err)?;
            let options = opts_str.and_then(|s| serde_json::from_str(&s).ok());
            let due_at: i64 = row.get(10).map_err(sqlite_err)?;
            let mastery_score: f64 = row.get(13).map_err(sqlite_err)?;

            let card = UniversalReviewCard {
                id: row.get(0).map_err(sqlite_err)?,
                module: row.get(1).map_err(sqlite_err)?,
                entity_id: row.get(2).map_err(sqlite_err)?,
                entity_type: row.get(3).map_err(sqlite_err)?,
                card_type,
                prompt: row.get(5).map_err(sqlite_err)?,
                answer: row.get(6).map_err(sqlite_err)?,
                options,
                hint: row.get(8).map_err(sqlite_err)?,
                context: row.get(9).map_err(sqlite_err)?,
                due_at,
                interval_days: row.get(11).map_err(sqlite_err)?,
                ease: row.get(12).map_err(sqlite_err)?,
                mastery_score,
                repetition_count: row.get(14).map_err(sqlite_err)?,
                lapses: row.get(15).map_err(sqlite_err)?,
                last_reviewed_at: row.get(16).map_err(sqlite_err)?,
                created_at: row.get(17).map_err(sqlite_err)?,
            };

            let is_overdue = due_at < now.saturating_sub(86400);
            let urgency = (100.0 - mastery_score) + ((now - due_at) as f64 / 86400.0).max(0.0) * 5.0;

            items.push(ReviewQueueItem {
                card,
                is_overdue,
                urgency_score: urgency,
            });
        }

        Ok(items)
    }

    /// 获取复习中心统计指标。
    pub fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, InfrastructureError> {
        let total_due: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM review_cards WHERE due_at <= ?1",
            params![now],
            |r| r.get(0),
        ).unwrap_or(0);

        let total_cards: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM review_cards",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let mastered_count: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM learning_progress WHERE status = 'mastered'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let learning_count: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM learning_progress WHERE status = 'learning'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let overdue_count: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM review_cards WHERE due_at <= ?1",
            params![now.saturating_sub(86400)],
            |r| r.get(0),
        ).unwrap_or(0);

        let upcoming_count: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM review_cards WHERE due_at > ?1 AND due_at <= ?2",
            params![now, now + 7 * 86400],
            |r| r.get(0),
        ).unwrap_or(0);

        let mut stmt = self.connection.prepare(
            "SELECT module, COUNT(*) FROM review_cards WHERE due_at <= ?1 GROUP BY module",
        ).map_err(sqlite_err)?;

        let mut by_module = HashMap::new();
        let mut rows = stmt.query(params![now]).map_err(sqlite_err)?;
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            let m: String = row.get(0).map_err(sqlite_err)?;
            let c: u32 = row.get(1).map_err(sqlite_err)?;
            by_module.insert(m, c);
        }

        Ok(ReviewQueueStats {
            total_due,
            due_count: total_due,
            overdue_count,
            upcoming_count,
            by_module,
            mastered_count,
            learning_count,
            total_cards,
        })
    }

    /// 提交一次卡片复习评分并更新 SRS 计划与 Progress。
    pub fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, InfrastructureError> {
        let card = self.get_review_card(card_id)?
            .ok_or_else(|| InfrastructureError::Sqlite(format!("review card {card_id} not found")))?;

        let outcome = SpacedRepetitionScheduler::schedule(
            card.interval_days,
            card.ease,
            card.repetition_count,
            card.lapses,
            rating,
            now,
        );

        // 1. 更新卡片
        self.connection.execute(
            "UPDATE review_cards SET
                interval_days = ?1,
                ease = ?2,
                due_at = ?3,
                repetition_count = ?4,
                lapses = ?5,
                last_reviewed_at = ?6
             WHERE id = ?7",
            params![
                outcome.interval_days,
                outcome.ease,
                outcome.due_at,
                outcome.repetition_count,
                outcome.lapses,
                now,
                card_id,
            ],
        ).map_err(sqlite_err)?;

        // 2. 发送并记录 LearningEvent
        let event_action = if outcome.is_correct {
            LearningAction::Correct
        } else {
            LearningAction::Incorrect
        };

        let event = LearningEvent {
            id: format!("evt_{now}_{}", &card.entity_id),
            module: card.module.clone(),
            entity_type: card.entity_type.clone(),
            entity_id: card.entity_id.clone(),
            entity_title: Some(card.prompt.clone()),
            action: event_action,
            timestamp: now,
            duration_ms: None,
            metadata: serde_json::json!({
                "rating": rating.label(),
                "interval_days": outcome.interval_days,
                "repetition_count": outcome.repetition_count,
            }),
            source: Some("review_center".to_string()),
        };

        self.record_event(&event)?;

        Ok(outcome)
    }

    // ========================================================================
    // 3. Collections & Collection Items
    // ========================================================================

    pub fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        tags: &[String],
        now: i64,
    ) -> Result<Collection, InfrastructureError> {
        let id = format!("col_{now}_{}", title.chars().take(8).collect::<String>());
        let tags_str = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());

        self.connection.execute(
            "INSERT INTO collections (id, title, description, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, title, description, tags_str, now, now],
        ).map_err(sqlite_err)?;

        Ok(Collection {
            id,
            title: title.to_string(),
            description: description.map(str::to_string),
            tags: tags.to_vec(),
            item_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn list_collections(&self) -> Result<Vec<Collection>, InfrastructureError> {
        let mut stmt = self.connection.prepare(
            "SELECT c.id, c.title, c.description, c.tags_json, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM collection_items i WHERE i.collection_id = c.id) AS item_count
             FROM collections c ORDER BY c.updated_at DESC",
        ).map_err(sqlite_err)?;

        let mut rows = stmt.query([]).map_err(sqlite_err)?;
        let mut list = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            let tags_str: String = row.get(3).map_err(sqlite_err)?;
            list.push(Collection {
                id: row.get(0).map_err(sqlite_err)?,
                title: row.get(1).map_err(sqlite_err)?,
                description: row.get(2).map_err(sqlite_err)?,
                tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                created_at: row.get(4).map_err(sqlite_err)?,
                updated_at: row.get(5).map_err(sqlite_err)?,
                item_count: row.get(6).map_err(sqlite_err)?,
            });
        }
        Ok(list)
    }

    pub fn add_collection_item(
        &self,
        collection_id: &str,
        module: &str,
        entity_type: &str,
        entity_id: &str,
        title: &str,
        note: Option<&str>,
        now: i64,
    ) -> Result<CollectionItem, InfrastructureError> {
        let item_id = format!("coli_{now}_{entity_id}");
        self.connection.execute(
            "INSERT INTO collection_items (id, collection_id, module, entity_type, entity_id, title, note, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![item_id, collection_id, module, entity_type, entity_id, title, note, now],
        ).map_err(sqlite_err)?;

        self.connection.execute(
            "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
            params![now, collection_id],
        ).map_err(sqlite_err)?;

        Ok(CollectionItem {
            id: item_id,
            collection_id: collection_id.to_string(),
            module: module.to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            title: title.to_string(),
            note: note.map(str::to_string),
            added_at: now,
        })
    }

    pub fn list_collection_items(&self, collection_id: &str) -> Result<Vec<CollectionItem>, InfrastructureError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, collection_id, module, entity_type, entity_id, title, note, added_at
             FROM collection_items WHERE collection_id = ?1 ORDER BY added_at DESC",
        ).map_err(sqlite_err)?;

        let mut rows = stmt.query(params![collection_id]).map_err(sqlite_err)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            items.push(CollectionItem {
                id: row.get(0).map_err(sqlite_err)?,
                collection_id: row.get(1).map_err(sqlite_err)?,
                module: row.get(2).map_err(sqlite_err)?,
                entity_type: row.get(3).map_err(sqlite_err)?,
                entity_id: row.get(4).map_err(sqlite_err)?,
                title: row.get(5).map_err(sqlite_err)?,
                note: row.get(6).map_err(sqlite_err)?,
                added_at: row.get(7).map_err(sqlite_err)?,
            });
        }
        Ok(items)
    }

    pub fn remove_collection_item(&self, item_id: &str) -> Result<(), InfrastructureError> {
        self.connection.execute("DELETE FROM collection_items WHERE id = ?1", params![item_id]).map_err(sqlite_err)?;
        Ok(())
    }

    pub fn delete_collection(&self, collection_id: &str) -> Result<(), InfrastructureError> {
        self.connection.execute("DELETE FROM collection_items WHERE collection_id = ?1", params![collection_id]).map_err(sqlite_err)?;
        self.connection.execute("DELETE FROM collections WHERE id = ?1", params![collection_id]).map_err(sqlite_err)?;
        Ok(())
    }

    // ========================================================================
    // 4. Continue & Recent Activities
    // ========================================================================

    /// 查询最近继续学习列表（去重聚合最新活动）。
    pub fn get_continue_items(&self, limit: usize) -> Result<Vec<ContinueItem>, InfrastructureError> {
        let mut stmt = self.connection.prepare(
            "SELECT module, entity_type, entity_id, entity_title, MAX(last_studied_at) as recent_time, mastery_score
             FROM learning_progress
             GROUP BY module, entity_type, entity_id
             ORDER BY recent_time DESC LIMIT ?1",
        ).map_err(sqlite_err)?;

        let mut rows = stmt.query(params![limit as i64]).map_err(sqlite_err)?;
        let mut list = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            let module: String = row.get(0).map_err(sqlite_err)?;
            let entity_type: String = row.get(1).map_err(sqlite_err)?;
            let entity_id: String = row.get(2).map_err(sqlite_err)?;
            let title: String = row.get(3).map_err(sqlite_err)?;
            let last_studied_at: i64 = row.get(4).map_err(sqlite_err)?;
            let mastery: f64 = row.get(5).map_err(sqlite_err)?;

            let action_target = match (module.as_str(), entity_type.as_str()) {
                ("history", "story") => format!("#history?story={entity_id}"),
                ("history", "person") => format!("#history?person={entity_id}"),
                ("history", "event") => format!("#history?event={entity_id}"),
                _ => format!("#{module}?id={entity_id}"),
            };
            let subtitle = match module.as_str() {
                "history" => Some("历史故事 · 继续阅读".to_string()),
                "geography" => Some("地理百科 · 探索地貌".to_string()),
                "language" => Some("语言词汇 · 继续背诵".to_string()),
                "study" => Some("学习板 · 继续整理".to_string()),
                "news" => Some("新闻背景 · 深度理解".to_string()),
                _ => Some(format!("{module} · 继续学习")),
            };

            list.push(ContinueItem {
                module,
                entity_type,
                entity_id,
                title,
                subtitle,
                progress_percent: Some(mastery),
                last_studied_at,
                action_target,
            });
        }

        Ok(list)
    }

    /// 查询今日学习的主题数量。
    pub fn count_topics_studied_today(&self, day_start_ts: i64) -> Result<u32, InfrastructureError> {
        let count: u32 = self.connection.query_row(
            "SELECT COUNT(DISTINCT entity_key) FROM learning_progress WHERE last_studied_at >= ?1",
            params![day_start_ts],
            |r| r.get(0),
        ).unwrap_or(0);
        Ok(count)
    }

    /// 查询平均掌握度。
    pub fn get_average_mastery(&self) -> Result<f64, InfrastructureError> {
        let avg: Option<f64> = self.connection.query_row(
            "SELECT AVG(mastery_score) FROM learning_progress WHERE mastery_score > 0",
            [],
            |r| r.get(0),
        ).optional().unwrap_or(None);
        Ok(avg.unwrap_or(0.0))
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_store_in_memory_flow() {
        let store = LearningStore::open_in_memory().expect("open store");
        let now = 1_700_000_000;

        // 1. 记录事件
        let event = LearningEvent {
            id: "evt_1".to_string(),
            module: "history".to_string(),
            entity_type: "story".to_string(),
            entity_id: "meiji_restoration".to_string(),
            entity_title: Some("明治维新".to_string()),
            action: LearningAction::Study,
            timestamp: now,
            duration_ms: Some(15000),
            metadata: serde_json::json!({"chapter": 1}),
            source: Some("today_continue".to_string()),
        };

        let progress = store.record_event(&event).expect("record event");
        assert_eq!(progress.study_count, 1);
        assert_eq!(progress.status, LearningStatus::Learning);

        // 2. 添加复习卡
        let card = UniversalReviewCard {
            id: "card_1".to_string(),
            module: "history".to_string(),
            entity_id: "meiji_restoration".to_string(),
            entity_type: "event".to_string(),
            card_type: ReviewCardType::Qa,
            prompt: "明治维新发生在何年？".to_string(),
            answer: "1868 年".to_string(),
            options: None,
            hint: Some("19世纪中后期".to_string()),
            context: Some("日本历史近现代转型".to_string()),
            due_at: now - 100, // 已到期
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        };

        store.upsert_review_card(&card).expect("upsert review card");

        // 3. 查询待复习队列
        let due_list = store.list_due_reviews(None, now, 10).expect("list due reviews");
        assert_eq!(due_list.len(), 1);
        assert_eq!(due_list[0].card.prompt, "明治维新发生在何年？");

        // 4. 提交复习打分
        let outcome = store.record_review_outcome("card_1", ReviewRating::Good, now).expect("submit review");
        assert_eq!(outcome.repetition_count, 1);
        assert_eq!(outcome.interval_days, 1.0);
        assert!(outcome.is_correct);

        // 5. 合集功能测试
        let col = store.create_collection("日本近代史专题", Some("历史与地理综合探索"), &["历史".to_string(), "日本".to_string()], now).expect("create collection");
        store.add_collection_item(&col.id, "history", "story", "meiji_restoration", "明治维新", Some("核心起点"), now).expect("add item");

        let cols = store.list_collections().expect("list collections");
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].item_count, 1);
    }
}
