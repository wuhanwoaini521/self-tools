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
    Collection, CollectionItem, CollectionItemRef, ContinueItem, LearningAction, LearningEvent,
    LearningProgress, LearningStatus, MasteryCalculator, ReviewCardType, ReviewQueueItem,
    ReviewQueueStats, ReviewRating, ReviewScheduleOutcome, SpacedRepetitionScheduler,
    UniversalReviewCard,
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
                    source_title TEXT,
                    user_note TEXT,
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
                CREATE TABLE IF NOT EXISTS review_submission_receipts (
                    request_id TEXT PRIMARY KEY,
                    card_id TEXT NOT NULL,
                    rating TEXT NOT NULL,
                    outcome_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );

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
        self.ensure_review_card_columns()?;
        self.purge_non_learning_progress()?;
        Ok(())
    }

    fn ensure_review_card_columns(&self) -> Result<(), InfrastructureError> {
        for (column, statement) in [
            (
                "source_title",
                "ALTER TABLE review_cards ADD COLUMN source_title TEXT",
            ),
            (
                "user_note",
                "ALTER TABLE review_cards ADD COLUMN user_note TEXT",
            ),
        ] {
            let exists: i64 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('review_cards') WHERE name = ?1",
                    params![column],
                    |row| row.get(0),
                )
                .map_err(sqlite_err)?;
            if exists == 0 {
                self.connection
                    .execute_batch(statement)
                    .map_err(sqlite_err)?;
            }
        }
        Ok(())
    }

    /// 一次性清理：把 news/rss 写进 `learning_progress` 的条目删掉。
    ///
    /// ## 为什么需要
    ///
    /// `learning_progress` 是**学习进度**表，但新闻/RSS 条目也会被写进来
    /// （曾经由自动化遍历批量写入）。后果不只是首页被新闻霸占，还会污染
    /// 所有聚合指标：平均掌握度、今日已学数量、连续学习天数。
    ///
    /// ## 为什么可以安全删
    ///
    /// - 判定依据很窄：仅 `module IN ('news','rss')`；
    /// - 这些条目**没有真实学习语义**：`review_count = 0`、没有复习卡、
    ///   掌握度是打开时算出来的默认值，不是答题结果；
    /// - 新闻本身的正文存在 `news.db`，删的是学习进度，**不影响文章数据**。
    ///
    /// 幂等：重复执行无副作用。
    fn purge_non_learning_progress(&self) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "DELETE FROM learning_progress WHERE module IN ('news', 'rss')",
                [],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    // ========================================================================
    // 1. Learning Events & Progress
    // ========================================================================

    /// 记录学习事件，并原子更新进度与掌握度。
    pub fn record_event(
        &self,
        event: &LearningEvent,
    ) -> Result<LearningProgress, InfrastructureError> {
        let metadata_str =
            serde_json::to_string(&event.metadata).unwrap_or_else(|_| "{}".to_string());

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
                event
                    .entity_title
                    .clone()
                    .unwrap_or_else(|| event.entity_id.clone()),
                event.timestamp,
            )
        });

        // 3. 更新统计
        match &event.action {
            LearningAction::Study
            | LearningAction::View
            | LearningAction::Complete
            | LearningAction::Note
            | LearningAction::Bookmark => {
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
        if let Some(title) = event.entity_title.as_ref().filter(|t| !t.is_empty()) {
            progress.entity_title = title.clone();
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
    pub fn get_progress(
        &self,
        entity_key: &str,
    ) -> Result<Option<LearningProgress>, InfrastructureError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT entity_key, module, entity_type, entity_id, entity_title, status,
                    study_count, review_count, correct_count, incorrect_count, mastery_score,
                    last_studied_at, next_review_at, interval_days, ease, custom_tags_json
             FROM learning_progress WHERE entity_key = ?1",
            )
            .map_err(sqlite_err)?;

        let row = stmt
            .query_row(params![entity_key], |row| {
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
            })
            .optional()
            .map_err(sqlite_err)?;

        Ok(row)
    }

    /// 保存或更新 Progress。
    pub fn save_progress(&self, progress: &LearningProgress) -> Result<(), InfrastructureError> {
        let tags_str =
            serde_json::to_string(&progress.custom_tags).unwrap_or_else(|_| "[]".to_string());

        self.connection
            .execute(
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
            )
            .map_err(sqlite_err)?;

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
        let mut rows = stmt
            .query(rusqlite::named_params! {
                ":module": module_filter,
                ":status": status_filter.map(|s| s.as_str()),
                ":limit": limit as i64,
            })
            .map_err(sqlite_err)?;

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
    pub fn upsert_review_card(
        &self,
        card: &UniversalReviewCard,
    ) -> Result<(), InfrastructureError> {
        let options_str = card
            .options
            .as_ref()
            .map(|opts| serde_json::to_string(opts).unwrap_or_default());
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
                hint, context, source_title, user_note, due_at, interval_days, ease, mastery_score,
                repetition_count, lapses, last_reviewed_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            ON CONFLICT(id) DO UPDATE SET
                prompt = excluded.prompt,
                answer = excluded.answer,
                options_json = excluded.options_json,
                hint = excluded.hint,
                context = excluded.context,
                source_title = excluded.source_title,
                user_note = excluded.user_note,
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
                card.source_title,
                card.user_note,
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

    /// Save Knowledge-owned text without replacing source identity or SRS progress.
    pub fn save_knowledge_review_card(
        &self,
        card: &UniversalReviewCard,
    ) -> Result<UniversalReviewCard, InfrastructureError> {
        if !card.id.starts_with("knowledge:")
            || !matches!(card.module.as_str(), "history" | "geography")
            || card.card_type != ReviewCardType::Qa
        {
            return Err(InfrastructureError::Sqlite(
                "invalid Knowledge review card identity or type".to_string(),
            ));
        }

        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(sqlite_err)?;
        let existing: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT module, entity_type, entity_id FROM review_cards WHERE id = ?1",
                params![card.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sqlite_err)?;

        if let Some((module, entity_type, entity_id)) = existing {
            if (module.as_str(), entity_type.as_str(), entity_id.as_str())
                != (
                    card.module.as_str(),
                    card.entity_type.as_str(),
                    card.entity_id.as_str(),
                )
            {
                return Err(InfrastructureError::Sqlite(
                    "Knowledge review card source identity cannot be changed".to_string(),
                ));
            }
            transaction
                .execute(
                    "UPDATE review_cards SET source_title = ?1, prompt = ?2, answer = ?3, user_note = ?4
                     WHERE id = ?5",
                    params![card.source_title, card.prompt, card.answer, card.user_note, card.id],
                )
                .map_err(sqlite_err)?;
        } else {
            transaction
                .execute(
                    "INSERT INTO review_cards (
                        id, module, entity_id, entity_type, card_type, prompt, answer, options_json,
                        hint, context, source_title, user_note, due_at, interval_days, ease,
                        mastery_score, repetition_count, lapses, last_reviewed_at, created_at
                    ) VALUES (?1, ?2, ?3, ?4, 'qa', ?5, ?6, NULL, NULL, NULL, ?7, ?8, ?9, 0.0, 2.5, 0.0, 0, 0, NULL, ?9)",
                    params![
                        card.id,
                        card.module,
                        card.entity_id,
                        card.entity_type,
                        card.prompt,
                        card.answer,
                        card.source_title,
                        card.user_note,
                        card.created_at,
                    ],
                )
                .map_err(sqlite_err)?;
        }
        transaction.commit().map_err(sqlite_err)?;
        self.get_review_card(&card.id)?.ok_or_else(|| {
            InfrastructureError::Sqlite("saved review card was not found".to_string())
        })
    }

    /// Remove only a Knowledge-created card; learning progress and review events are retained.
    pub fn delete_knowledge_review_card(&self, card_id: &str) -> Result<bool, InfrastructureError> {
        if !card_id.starts_with("knowledge:") {
            return Err(InfrastructureError::Sqlite(
                "only Knowledge-created review cards can be deleted here".to_string(),
            ));
        }
        let deleted = self
            .connection
            .execute(
                "DELETE FROM review_cards WHERE id = ?1 AND module IN ('history', 'geography')",
                params![card_id],
            )
            .map_err(sqlite_err)?;
        Ok(deleted > 0)
    }

    /// 获取指定卡片。
    pub fn get_review_card(
        &self,
        card_id: &str,
    ) -> Result<Option<UniversalReviewCard>, InfrastructureError> {
        let mut stmt = self.connection.prepare(
            "SELECT id, module, entity_id, entity_type, card_type, prompt, answer, options_json,
                    hint, context, source_title, user_note, due_at, interval_days, ease, mastery_score,
                    repetition_count, lapses, last_reviewed_at, created_at
             FROM review_cards WHERE id = ?1",
        ).map_err(sqlite_err)?;

        let row = stmt
            .query_row(params![card_id], |row| {
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
                    source_title: row.get(10)?,
                    user_note: row.get(11)?,
                    due_at: row.get(12)?,
                    interval_days: row.get(13)?,
                    ease: row.get(14)?,
                    mastery_score: row.get(15)?,
                    repetition_count: row.get(16)?,
                    lapses: row.get(17)?,
                    last_reviewed_at: row.get(18)?,
                    created_at: row.get(19)?,
                })
            })
            .optional()
            .map_err(sqlite_err)?;

        Ok(row)
    }

    /// 查询待复习队列（按到期时间排序）。
    pub fn list_due_reviews(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, InfrastructureError> {
        let query =
            "SELECT id, module, entity_id, entity_type, card_type, prompt, answer, options_json,
                            hint, context, source_title, user_note, due_at, interval_days, ease,
                            mastery_score, repetition_count, lapses, last_reviewed_at, created_at
                     FROM review_cards
                     WHERE due_at <= :now
                       AND (:module IS NULL OR :module = 'all' OR module = :module)
                     ORDER BY due_at ASC, mastery_score ASC LIMIT :limit";

        let mut stmt = self.connection.prepare(query).map_err(sqlite_err)?;
        let mut rows = stmt
            .query(rusqlite::named_params! {
                ":now": now,
                ":module": module_filter,
                ":limit": limit as i64,
            })
            .map_err(sqlite_err)?;

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
            let due_at: i64 = row.get(12).map_err(sqlite_err)?;
            let mastery_score: f64 = row.get(15).map_err(sqlite_err)?;

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
                source_title: row.get(10).map_err(sqlite_err)?,
                user_note: row.get(11).map_err(sqlite_err)?,
                due_at,
                interval_days: row.get(13).map_err(sqlite_err)?,
                ease: row.get(14).map_err(sqlite_err)?,
                mastery_score,
                repetition_count: row.get(16).map_err(sqlite_err)?,
                lapses: row.get(17).map_err(sqlite_err)?,
                last_reviewed_at: row.get(18).map_err(sqlite_err)?,
                created_at: row.get(19).map_err(sqlite_err)?,
            };

            let is_overdue = due_at < now.saturating_sub(86400);
            let urgency =
                (100.0 - mastery_score) + ((now - due_at) as f64 / 86400.0).max(0.0) * 5.0;

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
        let total_due: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM review_cards WHERE due_at <= ?1",
                params![now],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let total_cards: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM review_cards", [], |r| r.get(0))
            .unwrap_or(0);

        let mastered_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM learning_progress WHERE status = 'mastered'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let learning_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM learning_progress WHERE status = 'learning'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let overdue_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM review_cards WHERE due_at <= ?1",
                params![now.saturating_sub(86400)],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let upcoming_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM review_cards WHERE due_at > ?1 AND due_at <= ?2",
                params![now, now + 7 * 86400],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut stmt = self
            .connection
            .prepare("SELECT module, COUNT(*) FROM review_cards WHERE due_at <= ?1 GROUP BY module")
            .map_err(sqlite_err)?;

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

    /// Compatibility path: same card + timestamp is treated as one submission.
    pub fn record_review_outcome(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, InfrastructureError> {
        self.record_review_outcome_once(card_id, rating, &format!("legacy:{card_id}:{now}"), now)
    }

    /// Atomically advance SRS, persist learning progress/event, and record an idempotency receipt.
    pub fn record_review_outcome_once(
        &self,
        card_id: &str,
        rating: ReviewRating,
        request_id: &str,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, InfrastructureError> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(sqlite_err)?;
        let rating_key = match rating {
            ReviewRating::Again => "again",
            ReviewRating::Hard => "hard",
            ReviewRating::Good => "good",
            ReviewRating::Easy => "easy",
        };
        let receipt: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT card_id, rating, outcome_json FROM review_submission_receipts WHERE request_id = ?1",
                params![request_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sqlite_err)?;
        if let Some((saved_card_id, saved_rating, outcome_json)) = receipt {
            if saved_card_id != card_id || saved_rating != rating_key {
                return Err(InfrastructureError::Sqlite(
                    "review request ID was already used for a different submission".to_string(),
                ));
            }
            let outcome = serde_json::from_str(&outcome_json)
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
            transaction.commit().map_err(sqlite_err)?;
            return Ok(outcome);
        }

        let card = self.get_review_card(card_id)?.ok_or_else(|| {
            InfrastructureError::Sqlite(format!("review card {card_id} not found"))
        })?;
        let outcome = SpacedRepetitionScheduler::schedule(
            card.interval_days,
            card.ease,
            card.repetition_count,
            card.lapses,
            rating,
            now,
        );

        self.connection
            .execute(
                "UPDATE review_cards SET interval_days = ?1, ease = ?2, due_at = ?3,
                    repetition_count = ?4, lapses = ?5, last_reviewed_at = ?6 WHERE id = ?7",
                params![
                    outcome.interval_days,
                    outcome.ease,
                    outcome.due_at,
                    outcome.repetition_count,
                    outcome.lapses,
                    now,
                    card_id,
                ],
            )
            .map_err(sqlite_err)?;

        let entity_key = format!("{}:{}:{}", card.module, card.entity_type, card.entity_id);
        let mut progress = self.get_progress(&entity_key)?.unwrap_or_else(|| {
            LearningProgress::new(
                &card.module,
                &card.entity_type,
                &card.entity_id,
                card.source_title
                    .clone()
                    .unwrap_or_else(|| card.entity_id.clone()),
                now,
            )
        });
        progress.interval_days = outcome.interval_days;
        progress.ease = outcome.ease;
        progress.next_review_at = Some(outcome.due_at);
        self.save_progress(&progress)?;

        let event = LearningEvent {
            id: format!("review_submission:{request_id}"),
            module: card.module.clone(),
            entity_type: card.entity_type.clone(),
            entity_id: card.entity_id.clone(),
            entity_title: card
                .source_title
                .clone()
                .or_else(|| Some(card.prompt.clone())),
            action: if outcome.is_correct {
                LearningAction::Correct
            } else {
                LearningAction::Incorrect
            },
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

        let outcome_json = serde_json::to_string(&outcome)
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO review_submission_receipts (request_id, card_id, rating, outcome_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![request_id, card_id, rating_key, outcome_json, now],
            )
            .map_err(sqlite_err)?;
        transaction.commit().map_err(sqlite_err)?;
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
        entity: CollectionItemRef,
        now: i64,
    ) -> Result<CollectionItem, InfrastructureError> {
        let item_id = format!("coli_{now}_{}", entity.entity_id);
        self.connection.execute(
            "INSERT INTO collection_items (id, collection_id, module, entity_type, entity_id, title, note, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                item_id,
                collection_id,
                entity.module,
                entity.entity_type,
                entity.entity_id,
                entity.title,
                entity.note,
                now,
            ],
        ).map_err(sqlite_err)?;

        self.connection
            .execute(
                "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
                params![now, collection_id],
            )
            .map_err(sqlite_err)?;

        Ok(CollectionItem {
            id: item_id,
            collection_id: collection_id.to_string(),
            module: entity.module,
            entity_type: entity.entity_type,
            entity_id: entity.entity_id,
            title: entity.title,
            note: entity.note,
            added_at: now,
        })
    }

    pub fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItem>, InfrastructureError> {
        let mut stmt = self
            .connection
            .prepare(
                "SELECT id, collection_id, module, entity_type, entity_id, title, note, added_at
             FROM collection_items WHERE collection_id = ?1 ORDER BY added_at DESC",
            )
            .map_err(sqlite_err)?;

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

    pub fn update_collection_item_note(
        &mut self,
        item_id: &str,
        note: Option<&str>,
        now: i64,
    ) -> Result<Option<CollectionItem>, InfrastructureError> {
        let tx = self.connection.transaction().map_err(sqlite_err)?;
        let item = tx
            .query_row(
                "SELECT id, collection_id, module, entity_type, entity_id, title, note, added_at
                 FROM collection_items WHERE id = ?1",
                params![item_id],
                |row| {
                    Ok(CollectionItem {
                        id: row.get(0)?,
                        collection_id: row.get(1)?,
                        module: row.get(2)?,
                        entity_type: row.get(3)?,
                        entity_id: row.get(4)?,
                        title: row.get(5)?,
                        note: row.get(6)?,
                        added_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite_err)?;
        let Some(mut item) = item else {
            return Ok(None);
        };

        tx.execute(
            "UPDATE collection_items SET note = ?1 WHERE id = ?2",
            params![note, item_id],
        )
        .map_err(sqlite_err)?;
        tx.execute(
            "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
            params![now, item.collection_id],
        )
        .map_err(sqlite_err)?;
        item.note = note.map(str::to_string);
        tx.commit().map_err(sqlite_err)?;
        Ok(Some(item))
    }

    pub fn remove_collection_item(&self, item_id: &str) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "DELETE FROM collection_items WHERE id = ?1",
                params![item_id],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    pub fn delete_collection(&self, collection_id: &str) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "DELETE FROM collection_items WHERE collection_id = ?1",
                params![collection_id],
            )
            .map_err(sqlite_err)?;
        self.connection
            .execute(
                "DELETE FROM collections WHERE id = ?1",
                params![collection_id],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    // ========================================================================
    // 4. Continue & Recent Activities
    // ========================================================================

    /// 查询最近继续学习列表（去重聚合最新活动）。
    pub fn get_continue_items(
        &self,
        limit: usize,
    ) -> Result<Vec<ContinueItem>, InfrastructureError> {
        // 「继续学习」只列**真正的学习内容**。
        //
        // 排除 news/rss：打开一条新闻不等于「在学习」。此前新闻条目一旦进入
        // learning_progress 就会按 last_studied_at 排到最前，把首页最重要的
        // 位置占成随机新闻流（实测 100 条新闻霸屏，而真正的 history/language
        // 学习记录被挤掉）。RSS 同理——它只是信息源，不是学习对象。
        let mut stmt = self
            .connection
            .prepare(
                "SELECT module, entity_type, entity_id, entity_title,
                        MAX(last_studied_at) as recent_time, mastery_score
                 FROM learning_progress
                 WHERE module NOT IN ('news', 'rss')
                 GROUP BY module, entity_type, entity_id
                 ORDER BY recent_time DESC LIMIT ?1",
            )
            .map_err(sqlite_err)?;

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
    pub fn count_topics_studied_today(
        &self,
        day_start_ts: i64,
    ) -> Result<u32, InfrastructureError> {
        let day_end_ts = day_start_ts.saturating_add(86_400);
        let count: u32 = self.connection.query_row(
            "SELECT COUNT(DISTINCT entity_key) FROM learning_progress
             WHERE last_studied_at >= ?1 AND last_studied_at < ?2",
            params![day_start_ts, day_end_ts],
            |r| r.get(0),
        ).unwrap_or(0);
        Ok(count)
    }

    /// Query distinct active-learning days, projected into the supplied fixed UTC offset.
    pub fn list_study_activity_day_starts(
        &self,
        utc_offset_seconds: i64,
        now: i64,
    ) -> Result<Vec<i64>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT DISTINCT CAST(strftime('%s', date(timestamp + ?1, 'unixepoch')) AS INTEGER) - ?1 AS day_start
                 FROM learning_events
                 WHERE timestamp <= ?2
                   AND module NOT IN ('news', 'rss')
                   AND action IN ('study', 'complete', 'review', 'answer', 'correct', 'incorrect')
                 ORDER BY day_start DESC",
            )
            .map_err(sqlite_err)?;
        let rows = statement
            .query_map(params![utc_offset_seconds, now], |row| row.get(0))
            .map_err(sqlite_err)?;
        rows.collect::<Result<Vec<i64>, _>>().map_err(sqlite_err)
    }

    /// 查询平均掌握度。
    pub fn get_average_mastery(&self) -> Result<f64, InfrastructureError> {
        let avg: Option<f64> = self
            .connection
            .query_row(
                "SELECT AVG(mastery_score) FROM learning_progress WHERE mastery_score > 0",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);
        Ok(avg.unwrap_or(0.0))
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        store: &LearningStore,
        module: &str,
        entity_type: &str,
        entity_id: &str,
        title: &str,
        at: i64,
    ) {
        store
            .record_event(&LearningEvent {
                id: format!("evt_{module}_{entity_id}"),
                module: module.to_string(),
                entity_type: entity_type.to_string(),
                entity_id: entity_id.to_string(),
                entity_title: Some(title.to_string()),
                action: LearningAction::Study,
                timestamp: at,
                duration_ms: None,
                metadata: serde_json::json!({}),
                source: Some("test".to_string()),
            })
            .expect("record event");
    }

    fn knowledge_card(now: i64) -> UniversalReviewCard {
        UniversalReviewCard {
            id: "knowledge:history:event:event-anlu-luoyang".to_string(),
            module: "history".to_string(),
            entity_id: "event-anlu-luoyang".to_string(),
            entity_type: "event".to_string(),
            card_type: ReviewCardType::Qa,
            prompt: "叛军何时攻陷洛阳？".to_string(),
            answer: "公元 756 年。".to_string(),
            options: None,
            hint: None,
            context: None,
            source_title: Some("洛阳失守".to_string()),
            user_note: Some("个人记录".to_string()),
            due_at: now,
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        }
    }

    /// 回归：新闻/RSS **不是学习对象**，不得出现在「继续学习」里。
    ///
    /// 真实事故：新闻条目曾被批量写进 `learning_progress`，首页最重要的
    /// 「继续学习」位置被 100 条随机新闻霸占，真正的 history/language
    /// 学习记录被挤掉——而用户看到的是「侗族多耶节」这类与他学习无关的内容。
    #[test]
    fn continue_items_exclude_news_and_rss() {
        let store = LearningStore::open_in_memory().expect("open store");
        let now = 1_700_000_000;
        record(&store, "history", "event", "e1", "夏朝建立", now);
        record(&store, "language", "word", "w1", "食べる", now + 1);
        // 新闻更「新」——如果不过滤，它会排在最前面霸屏。
        record(&store, "news", "article", "n1", "侗族多耶节启幕", now + 2);
        record(&store, "rss", "feed", "r1", "某订阅", now + 3);

        let items = store.get_continue_items(10).expect("continue items");
        let modules: Vec<&str> = items.iter().map(|item| item.module.as_str()).collect();
        assert!(
            !modules.contains(&"news") && !modules.contains(&"rss"),
            "新闻/RSS 不应出现在继续学习：{modules:?}"
        );
        assert!(
            modules.contains(&"history") && modules.contains(&"language"),
            "真实学习内容必须保留：{modules:?}"
        );
    }

    /// 回归：news/rss 写入的进度会在打开库时被清理，不污染聚合指标。
    ///
    /// 依据很窄（仅这两个 module），且不影响 `news.db` 里的文章本体。
    #[test]
    fn news_and_rss_progress_rows_are_purged_on_open() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("learning.db");

        {
            let store = LearningStore::open(&path).expect("open");
            record(&store, "news", "article", "n1", "某新闻", 1_700_000_000);
            record(&store, "history", "event", "e1", "夏朝建立", 1_700_000_001);
            assert_eq!(
                store
                    .list_progress(Some("news"), None, 10)
                    .expect("list")
                    .len(),
                1,
                "写入后应存在（此时尚未清理）"
            );
        }

        // 重新打开 → 触发一次性清理。
        let reopened = LearningStore::open(&path).expect("reopen");
        assert_eq!(
            reopened
                .list_progress(Some("news"), None, 10)
                .expect("list")
                .len(),
            0,
            "重新打开后新闻进度应被清理"
        );
        assert_eq!(
            reopened
                .list_progress(Some("history"), None, 10)
                .expect("list")
                .len(),
            1,
            "真实学习进度不得被误删"
        );
    }

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
            source_title: None,
            user_note: None,
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
        let due_list = store
            .list_due_reviews(None, now, 10)
            .expect("list due reviews");
        assert_eq!(due_list.len(), 1);
        assert_eq!(due_list[0].card.prompt, "明治维新发生在何年？");

        // 4. 提交复习打分
        let outcome = store
            .record_review_outcome("card_1", ReviewRating::Good, now)
            .expect("submit review");
        assert_eq!(outcome.repetition_count, 1);
        assert_eq!(outcome.interval_days, 1.0);
        assert!(outcome.is_correct);

        // 5. 合集功能测试
        let col = store
            .create_collection(
                "日本近代史专题",
                Some("历史与地理综合探索"),
                &["历史".to_string(), "日本".to_string()],
                now,
            )
            .expect("create collection");
        store
            .add_collection_item(
                &col.id,
                CollectionItemRef {
                    module: "history".to_string(),
                    entity_type: "story".to_string(),
                    entity_id: "meiji_restoration".to_string(),
                    title: "明治维新".to_string(),
                    note: Some("核心起点".to_string()),
                },
                now,
            )
            .expect("add item");

        let cols = store.list_collections().expect("list collections");
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].item_count, 1);
    }

    #[test]
    fn updating_a_collection_note_preserves_its_source_and_updates_recent_order() {
        let mut store = LearningStore::open_in_memory().expect("open store");
        let collection = store
            .create_collection("洛阳笔记", None, &[], 1_000)
            .expect("create collection");
        let item = store
            .add_collection_item(
                &collection.id,
                CollectionItemRef {
                    module: "history".into(),
                    entity_type: "event".into(),
                    entity_id: "event-anlu-luoyang".into(),
                    title: "洛阳失守".into(),
                    note: Some("初始备注".into()),
                },
                1_001,
            )
            .expect("add collection item");

        let updated = store
            .update_collection_item_note(&item.id, Some("复核来源后补充"), 1_100)
            .expect("update note")
            .expect("existing item");
        assert_eq!(updated.id, item.id);
        assert_eq!(updated.entity_id, "event-anlu-luoyang");
        assert_eq!(updated.note.as_deref(), Some("复核来源后补充"));
        assert_eq!(
            store
                .list_collection_items(&collection.id)
                .expect("list items")[0]
                .note
                .as_deref(),
            Some("复核来源后补充")
        );
        assert_eq!(
            store.list_collections().expect("list collections")[0].updated_at,
            1_100
        );
        assert!(
            store
                .update_collection_item_note("missing-item", None, 1_101)
                .expect("missing item is not a storage error")
                .is_none()
        );
    }

    /// 复习结果必须真正推进 `learning_progress` 的 SRS 字段。
    ///
    /// 回归缺陷：`interval_days` / `ease` / `next_review_at` 此前从无写入点，
    /// 于是掌握度的「间隔因子」恒为 0，`Mastered`（要求 interval ≥ 14 天）
    /// 在生产中永远不可达。本用例锁定该链路。
    #[test]
    fn review_outcome_persists_srs_fields_into_progress() {
        let store = LearningStore::open_in_memory().expect("open store");
        let now = 1_700_000_000i64;
        let entity_id = "jmdict:1002990";

        // 首次学习 → 建立 progress 与复习卡
        store
            .record_event(&LearningEvent {
                id: "evt_1".to_string(),
                module: "language".to_string(),
                entity_type: "word".to_string(),
                entity_id: entity_id.to_string(),
                entity_title: Some("駅".to_string()),
                action: LearningAction::Study,
                timestamp: now,
                duration_ms: None,
                metadata: serde_json::json!({}),
                source: None,
            })
            .expect("record study");

        let card = UniversalReviewCard {
            id: "card_lang_1".to_string(),
            module: "language".to_string(),
            entity_id: entity_id.to_string(),
            entity_type: "word".to_string(),
            card_type: ReviewCardType::Recall,
            prompt: "駅 的含义？".to_string(),
            answer: "车站".to_string(),
            options: None,
            hint: None,
            context: None,
            source_title: None,
            user_note: None,
            due_at: now,
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        };
        store.upsert_review_card(&card).expect("upsert card");

        // 连续 Good 若干轮，间隔应逐轮增长并写回 progress
        let entity_key = format!("language:word:{entity_id}");
        let mut clock = now;
        let mut last_interval = 0.0;
        for round in 0..5 {
            let outcome = store
                .record_review_outcome("card_lang_1", ReviewRating::Good, clock)
                .expect("review");
            assert!(
                outcome.interval_days > last_interval,
                "round {round}: interval 应递增，实际 {} -> {}",
                last_interval,
                outcome.interval_days
            );
            last_interval = outcome.interval_days;

            let progress = store
                .get_progress(&entity_key)
                .expect("get progress")
                .expect("progress 存在");
            assert_eq!(progress.interval_days, outcome.interval_days);
            assert_eq!(progress.ease, outcome.ease);
            assert_eq!(progress.next_review_at, Some(outcome.due_at));
            assert!(progress.review_count > 0);

            clock = outcome.due_at;
        }

        // 间隔越过 14 天阈值后，Mastered 必须可达。
        let progress = store
            .get_progress(&entity_key)
            .expect("get progress")
            .expect("progress 存在");
        assert!(
            progress.interval_days >= 14.0,
            "多轮 Good 后间隔应达到 14 天阈值，实际 {}",
            progress.interval_days
        );
        assert_eq!(progress.status, LearningStatus::Mastered);
    }

    /// 答错必须记为 Incorrect，并让掌握度不因重复出错而虚高。
    #[test]
    fn incorrect_review_records_error_and_keeps_progress_honest() {
        let store = LearningStore::open_in_memory().expect("open store");
        let now = 1_700_000_000i64;
        let entity_key = "language:word:wrong_one";

        store
            .record_event(&LearningEvent {
                id: "evt_1".to_string(),
                module: "language".to_string(),
                entity_type: "word".to_string(),
                entity_id: "wrong_one".to_string(),
                entity_title: Some("切符".to_string()),
                action: LearningAction::Study,
                timestamp: now,
                duration_ms: None,
                metadata: serde_json::json!({}),
                source: None,
            })
            .expect("record study");

        let card = UniversalReviewCard {
            id: "card_wrong".to_string(),
            module: "language".to_string(),
            entity_id: "wrong_one".to_string(),
            entity_type: "word".to_string(),
            card_type: ReviewCardType::FillBlank,
            prompt: "切符".to_string(),
            answer: "ticket".to_string(),
            options: None,
            hint: None,
            context: None,
            source_title: None,
            user_note: None,
            due_at: now,
            interval_days: 0.0,
            ease: 2.5,
            mastery_score: 0.0,
            repetition_count: 0,
            lapses: 0,
            last_reviewed_at: None,
            created_at: now,
        };
        store.upsert_review_card(&card).expect("upsert card");

        let outcome = store
            .record_review_outcome("card_wrong", ReviewRating::Again, now)
            .expect("review");
        assert!(!outcome.is_correct);
        assert_eq!(outcome.lapses, 1);
        assert_eq!(outcome.interval_days, 0.0);

        let progress = store
            .get_progress(entity_key)
            .expect("get progress")
            .expect("progress 存在");
        assert_eq!(progress.incorrect_count, 1);
        assert_eq!(progress.correct_count, 0);
        assert_eq!(progress.status, LearningStatus::Learning);
        assert!(
            progress.mastery_score < 60.0,
            "只学过一次且答错，掌握度不应高，实际 {}",
            progress.mastery_score
        );
    }

    #[test]
    fn knowledge_card_content_edits_preserve_review_receipt_srs_and_progress() {
        let store = LearningStore::open_in_memory().expect("open store");
        let now = 1_700_000_000i64;
        let card = knowledge_card(now);
        let saved = store.save_knowledge_review_card(&card).expect("save card");
        assert_eq!(saved.source_title.as_deref(), Some("洛阳失守"));
        assert_eq!(saved.user_note.as_deref(), Some("个人记录"));

        let request_id = "review-test-123";
        let outcome = store
            .record_review_outcome_once(&card.id, ReviewRating::Good, request_id, now)
            .expect("submit once");
        assert_eq!(outcome.repetition_count, 1);
        assert_eq!(outcome.interval_days, 1.0);

        let retry = store
            .record_review_outcome_once(&card.id, ReviewRating::Good, request_id, now + 90)
            .expect("idempotent retry");
        assert_eq!(retry.due_at, outcome.due_at);
        assert_eq!(retry.repetition_count, outcome.repetition_count);
        assert!(
            store
                .record_review_outcome_once(&card.id, ReviewRating::Hard, request_id, now + 90)
                .is_err()
        );

        let progress = store
            .get_progress("history:event:event-anlu-luoyang")
            .expect("read progress")
            .expect("first review creates progress");
        assert_eq!(progress.review_count, 1);
        assert_eq!(progress.correct_count, 1);
        assert_eq!(progress.interval_days, outcome.interval_days);
        assert_eq!(progress.next_review_at, Some(outcome.due_at));
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM learning_events", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1,
            "a retry must not duplicate the learning event"
        );

        let mut edited = card.clone();
        edited.prompt = "洛阳失守发生在哪一年？".into();
        edited.answer = "公元 756 年。".into();
        edited.user_note = Some("根据个人理解重新表述".into());
        // Client-provided scheduling fields are intentionally stale and must be ignored.
        edited.due_at = now;
        edited.interval_days = 0.0;
        edited.repetition_count = 0;
        let updated = store
            .save_knowledge_review_card(&edited)
            .expect("edit card text");
        assert_eq!(updated.prompt, edited.prompt);
        assert_eq!(updated.answer, edited.answer);
        assert_eq!(updated.user_note, edited.user_note);
        assert_eq!(updated.due_at, outcome.due_at);
        assert_eq!(updated.interval_days, outcome.interval_days);
        assert_eq!(updated.repetition_count, outcome.repetition_count);
        assert_eq!(updated.last_reviewed_at, Some(now));

        assert!(
            store
                .delete_knowledge_review_card(&card.id)
                .expect("delete own card")
        );
        assert!(store.get_review_card(&card.id).unwrap().is_none());
        assert_eq!(
            store
                .get_progress("history:event:event-anlu-luoyang")
                .unwrap()
                .unwrap()
                .review_count,
            1,
            "deleting the card must retain review history/progress"
        );
        assert!(
            store
                .delete_knowledge_review_card("language:word:kept")
                .is_err()
        );
    }

    #[test]
    fn review_database_write_failure_rolls_back_schedule_progress_event_and_receipt() {
        let store = LearningStore::open_in_memory().expect("open store");
        let now = 1_700_000_000i64;
        let card = knowledge_card(now);
        store.save_knowledge_review_card(&card).expect("save card");
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_review_center_event BEFORE INSERT ON learning_events
             WHEN NEW.source = 'review_center'
             BEGIN SELECT RAISE(FAIL, 'simulated learning event write failure'); END;",
            )
            .expect("install failing trigger");

        assert!(
            store
                .record_review_outcome_once(&card.id, ReviewRating::Good, "atomic-failure", now)
                .is_err()
        );
        let unchanged = store.get_review_card(&card.id).unwrap().unwrap();
        assert_eq!(unchanged.due_at, now);
        assert_eq!(unchanged.repetition_count, 0);
        assert_eq!(unchanged.last_reviewed_at, None);
        assert!(
            store
                .get_progress("history:event:event-anlu-luoyang")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM learning_events", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM review_submission_receipts",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn old_review_card_schema_migrates_without_losing_existing_cards() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("legacy-learning.db");
        {
            let connection = Connection::open(&path).expect("open legacy db");
            connection.execute_batch(
                "CREATE TABLE review_cards (
                    id TEXT PRIMARY KEY, module TEXT NOT NULL, entity_id TEXT NOT NULL,
                    entity_type TEXT NOT NULL, card_type TEXT NOT NULL, prompt TEXT NOT NULL,
                    answer TEXT NOT NULL, options_json TEXT, hint TEXT, context TEXT,
                    due_at INTEGER NOT NULL, interval_days REAL NOT NULL DEFAULT 0.0,
                    ease REAL NOT NULL DEFAULT 2.5, mastery_score REAL NOT NULL DEFAULT 0.0,
                    repetition_count INTEGER NOT NULL DEFAULT 0, lapses INTEGER NOT NULL DEFAULT 0,
                    last_reviewed_at INTEGER, created_at INTEGER NOT NULL
                );
                INSERT INTO review_cards (id,module,entity_id,entity_type,card_type,prompt,answer,due_at,created_at)
                VALUES ('legacy-card','history','event-1','event','qa','问题','答案',1700000000,1699999999);",
            ).expect("seed legacy schema");
        }
        let store = LearningStore::open(&path).expect("open and migrate");
        let card = store.get_review_card("legacy-card").unwrap().unwrap();
        assert_eq!(card.prompt, "问题");
        assert_eq!(card.source_title, None);
        assert_eq!(card.user_note, None);
        store
            .ensure_review_card_columns()
            .expect("migration is idempotent");
    }

    #[test]
    fn knowledge_card_schedule_and_idempotency_receipt_survive_database_reopen() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("learning.db");
        let card_id = "knowledge:history:event:event-anlu-luoyang";
        let now = 1_700_000_000i64;
        let original = {
            let store = LearningStore::open(&path).expect("open store");
            let card = knowledge_card(now);
            store.save_knowledge_review_card(&card).expect("save card");
            store
                .record_review_outcome_once(card_id, ReviewRating::Easy, "restart-receipt", now)
                .expect("submit review");
            store.get_review_card(card_id).unwrap().unwrap()
        };

        let reopened = LearningStore::open(&path).expect("reopen database");
        let retry = reopened
            .record_review_outcome_once(card_id, ReviewRating::Easy, "restart-receipt", now + 500)
            .expect("same request returns persisted receipt");
        let persisted = reopened.get_review_card(card_id).unwrap().unwrap();
        assert_eq!(retry.due_at, original.due_at);
        assert_eq!(retry.repetition_count, original.repetition_count);
        assert_eq!(persisted.due_at, original.due_at);
        assert_eq!(persisted.last_reviewed_at, original.last_reviewed_at);
        assert_eq!(persisted.repetition_count, 1);
        assert_eq!(
            reopened
                .get_progress("history:event:event-anlu-luoyang")
                .unwrap()
                .unwrap()
                .review_count,
            1
        );
        assert_eq!(
            reopened
                .connection
                .query_row("SELECT COUNT(*) FROM learning_events", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn srs_due_timestamp_crosses_day_boundary_in_elapsed_seconds() {
        let day_start = 1_700_000_000i64.div_euclid(86_400) * 86_400;
        let just_before_midnight = day_start + 86_399;
        let outcome = SpacedRepetitionScheduler::schedule(
            0.0,
            2.5,
            0,
            0,
            ReviewRating::Good,
            just_before_midnight,
        );
        assert_eq!(outcome.due_at, just_before_midnight + 86_400);
        assert!(outcome.due_at / 86_400 > just_before_midnight / 86_400);
    }
}
