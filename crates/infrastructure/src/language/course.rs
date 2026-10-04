//! 英语课程学习子域的 SQLite 存储（`language.db` 内的 course/dict 表组）。
//!
//! 与词典表同库、但表组完全独立：教材内容（courses/books/lessons/sentences/vocab）
//! 可整体重导入而不触碰 `language_lesson_progress` / `language_word_occurrences` /
//! `language_plan` 三张用户状态表。单词的 SRS 排期在平台 `learning.db`，本模块只记
//! 「在哪见过这个词」（occurrences）——排期单点真相不破。

use rusqlite::{Connection, OptionalExtension, params};

use devtoolbox_core::language::{
    BookSummary, Course, CourseBook, CourseLesson, LearningPlan, LessonListEntry, LessonProgress,
    LessonSentence, LessonStage, LessonStatus, LessonVocab, WordEntry, WordOccurrence,
};

use crate::error::InfrastructureError;

use super::store::LanguageStore;

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

impl LanguageStore {
    /// 课程子域 schema（幂等；随 `open` 调用）。
    pub(crate) fn ensure_course_schema(&self) -> Result<(), InfrastructureError> {
        self.conn()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS language_courses (
                    id TEXT PRIMARY KEY, language TEXT NOT NULL, code TEXT NOT NULL,
                    title TEXT NOT NULL, description TEXT, source_type TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS language_books (
                    id TEXT PRIMARY KEY, course_id TEXT NOT NULL, book_no INTEGER NOT NULL,
                    title TEXT NOT NULL, subtitle TEXT, total_lessons INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS language_lessons (
                    id TEXT PRIMARY KEY, book_id TEXT NOT NULL, lesson_no INTEGER NOT NULL,
                    title TEXT NOT NULL, audio_path TEXT, duration_ms INTEGER,
                    sentence_count INTEGER NOT NULL DEFAULT 0,
                    vocab_count INTEGER NOT NULL DEFAULT 0, imported_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_course_lessons_book
                    ON language_lessons(book_id, lesson_no);
                CREATE TABLE IF NOT EXISTS language_sentences (
                    id TEXT PRIMARY KEY, lesson_id TEXT NOT NULL, sequence INTEGER NOT NULL,
                    start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL,
                    english TEXT NOT NULL, chinese TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_course_sentences_lesson
                    ON language_sentences(lesson_id, sequence);
                CREATE TABLE IF NOT EXISTS language_lesson_vocab (
                    lesson_id TEXT NOT NULL, word TEXT NOT NULL, surface TEXT,
                    sentence_id TEXT, context TEXT, phonetic TEXT, pos TEXT,
                    translation_zh TEXT, definition_en TEXT,
                    frequency INTEGER NOT NULL DEFAULT 0, tags TEXT NOT NULL DEFAULT '',
                    importance INTEGER NOT NULL DEFAULT 0,
                    -- 用户在本课的自评（know / fuzzy / unknown）。**必须持久化**：
                    -- 否则重进课程时又变成全新的词表，用户不知道上次标过哪些。
                    user_mark TEXT,
                    PRIMARY KEY (lesson_id, word)
                );
                CREATE TABLE IF NOT EXISTS language_lesson_progress (
                    lesson_id TEXT PRIMARY KEY, stage TEXT NOT NULL DEFAULT 'vocabulary',
                    position_ms INTEGER NOT NULL DEFAULT 0,
                    sentence_seq INTEGER NOT NULL DEFAULT 0,
                    vocab_index INTEGER NOT NULL DEFAULT 0,
                    shadow_seq INTEGER NOT NULL DEFAULT 0,
                    quiz_score INTEGER, completed_at INTEGER,
                    study_seconds INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS language_word_occurrences (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, word TEXT NOT NULL,
                    source_type TEXT NOT NULL, source_id TEXT NOT NULL,
                    sentence TEXT, occurred_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_occurrences_word
                    ON language_word_occurrences(word, occurred_at DESC);
                CREATE TABLE IF NOT EXISTS language_plan (
                    language TEXT PRIMARY KEY, course_id TEXT, book_id TEXT,
                    daily_minutes INTEGER NOT NULL DEFAULT 30,
                    new_words_per_day INTEGER NOT NULL DEFAULT 10,
                    nce_source_dir TEXT,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS dict_entries (
                    word TEXT PRIMARY KEY COLLATE NOCASE, phonetic TEXT,
                    definition_en TEXT, translation_zh TEXT, pos TEXT,
                    collins INTEGER NOT NULL DEFAULT 0, oxford INTEGER NOT NULL DEFAULT 0,
                    tag TEXT NOT NULL DEFAULT '', bnc INTEGER NOT NULL DEFAULT 0,
                    frq INTEGER NOT NULL DEFAULT 0, exchange TEXT NOT NULL DEFAULT ''
                );
                CREATE INDEX IF NOT EXISTS idx_dict_frq ON dict_entries(frq) WHERE frq > 0;",
            )
            .map_err(sqlite)?;
        self.migrate_plan_source_dir()?;
        self.migrate_lesson_vocab_user_mark()
    }

    /// 旧库 `language_lesson_vocab` 没有 `user_mark` 列：补上（幂等）。
    fn migrate_lesson_vocab_user_mark(&self) -> Result<(), InfrastructureError> {
        let has_column = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('language_lesson_vocab') WHERE name = 'user_mark'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sqlite)?;
        if has_column == 0 {
            self.conn()
                .execute(
                    "ALTER TABLE language_lesson_vocab ADD COLUMN user_mark TEXT",
                    [],
                )
                .map_err(sqlite)?;
        }
        Ok(())
    }

    /// 本课每个词的用户自评（word -> mark）。
    pub fn lesson_vocab_marks(
        &self,
        lesson_id: &str,
    ) -> Result<std::collections::HashMap<String, Option<String>>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare("SELECT word, user_mark FROM language_lesson_vocab WHERE lesson_id = ?1")
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![lesson_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(sqlite)?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (word, mark) = row.map_err(sqlite)?;
            map.insert(word, mark);
        }
        Ok(map)
    }

    /// 记录用户在本课的自评（know / fuzzy / unknown）。
    ///
    /// 与 SRS 并存但语义不同：SRS 排的是**什么时候复习**，
    /// 这里记的是**用户当时怎么说的**——课前预习要靠它把「认识」的词移出队列。
    pub fn set_lesson_vocab_mark(
        &self,
        lesson_id: &str,
        word: &str,
        mark: Option<&str>,
    ) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "UPDATE language_lesson_vocab SET user_mark = ?3 WHERE lesson_id = ?1 AND word = ?2",
                params![lesson_id, word, mark],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 旧版 `language_plan` 没有 `nce_source_dir` 列：补上（幂等）。
    fn migrate_plan_source_dir(&self) -> Result<(), InfrastructureError> {
        let has_column = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('language_plan') WHERE name = 'nce_source_dir'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sqlite)?;
        if has_column == 0 {
            self.conn()
                .execute(
                    "ALTER TABLE language_plan ADD COLUMN nce_source_dir TEXT",
                    [],
                )
                .map_err(sqlite)?;
        }
        Ok(())
    }

    // ====================================================================
    // 课程结构写入（导入器用；upsert 幂等）
    // ====================================================================

    pub fn upsert_course(&self, course: &Course) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO language_courses
                    (id, language, code, title, description, source_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title, description = excluded.description,
                    source_type = excluded.source_type",
                params![
                    course.id,
                    course.language.code(),
                    course.code,
                    course.title,
                    course.description,
                    course.source_type,
                    course.created_at
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn upsert_book(&self, book: &CourseBook) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO language_books (id, course_id, book_no, title, subtitle, total_lessons)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    book_no = excluded.book_no, title = excluded.title,
                    subtitle = excluded.subtitle, total_lessons = excluded.total_lessons",
                params![
                    book.id,
                    book.course_id,
                    book.book_no,
                    book.title,
                    book.subtitle,
                    book.total_lessons
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 写入一课及其全部句子/生词（同一事务；重导入 = 替换内容，不动进度）。
    pub fn replace_lesson_content(
        &self,
        lesson: &CourseLesson,
        sentences: &[LessonSentence],
        vocab: &[LessonVocab],
    ) -> Result<(), InfrastructureError> {
        let tx = self.conn().unchecked_transaction().map_err(sqlite)?;
        tx.execute(
            "INSERT INTO language_lessons
                (id, book_id, lesson_no, title, audio_path, duration_ms,
                 sentence_count, vocab_count, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                book_id = excluded.book_id, lesson_no = excluded.lesson_no,
                title = excluded.title, audio_path = excluded.audio_path,
                duration_ms = excluded.duration_ms,
                sentence_count = excluded.sentence_count,
                vocab_count = excluded.vocab_count, imported_at = excluded.imported_at",
            params![
                lesson.id,
                lesson.book_id,
                lesson.lesson_no,
                lesson.title,
                lesson.audio_path,
                lesson.duration_ms,
                sentences.len() as i64,
                vocab.len() as i64,
                crate::now_unix()
            ],
        )
        .map_err(sqlite)?;
        tx.execute(
            "DELETE FROM language_sentences WHERE lesson_id = ?1",
            params![lesson.id],
        )
        .map_err(sqlite)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO language_sentences
                        (id, lesson_id, sequence, start_ms, end_ms, english, chinese)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(sqlite)?;
            for sentence in sentences {
                stmt.execute(params![
                    sentence.id,
                    sentence.lesson_id,
                    sentence.sequence,
                    sentence.start_ms,
                    sentence.end_ms,
                    sentence.english,
                    sentence.chinese
                ])
                .map_err(sqlite)?;
            }
        }
        tx.execute(
            "DELETE FROM language_lesson_vocab WHERE lesson_id = ?1",
            params![lesson.id],
        )
        .map_err(sqlite)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO language_lesson_vocab
                        (lesson_id, word, surface, sentence_id, context, phonetic, pos,
                         translation_zh, definition_en, frequency, tags, importance)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )
                .map_err(sqlite)?;
            for word in vocab {
                stmt.execute(params![
                    word.lesson_id,
                    word.word,
                    word.surface,
                    word.sentence_id,
                    word.context,
                    word.phonetic,
                    word.pos,
                    word.translation_zh,
                    word.definition_en,
                    word.frequency,
                    word.tags.join(","),
                    word.importance
                ])
                .map_err(sqlite)?;
            }
        }
        tx.commit().map_err(sqlite)
    }

    // ====================================================================
    // 课程结构查询
    // ====================================================================

    pub fn courses(&self) -> Result<Vec<Course>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, language, code, title, description, source_type, created_at
                 FROM language_courses ORDER BY code",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Course {
                    id: row.get(0)?,
                    language: devtoolbox_core::language::LanguageCode::from_code(
                        &row.get::<_, String>(1)?,
                    )
                    .unwrap_or(devtoolbox_core::language::LanguageCode::Eng),
                    code: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    source_type: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    pub fn course_books(&self, course_id: &str) -> Result<Vec<CourseBook>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, course_id, book_no, title, subtitle, total_lessons
                 FROM language_books WHERE course_id = ?1 ORDER BY book_no",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![course_id], |row| {
                Ok(CourseBook {
                    id: row.get(0)?,
                    course_id: row.get(1)?,
                    book_no: row.get(2)?,
                    title: row.get(3)?,
                    subtitle: row.get(4)?,
                    total_lessons: row.get(5)?,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    pub fn book(&self, book_id: &str) -> Result<Option<CourseBook>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT id, course_id, book_no, title, subtitle, total_lessons
                 FROM language_books WHERE id = ?1",
                params![book_id],
                |row| {
                    Ok(CourseBook {
                        id: row.get(0)?,
                        course_id: row.get(1)?,
                        book_no: row.get(2)?,
                        title: row.get(3)?,
                        subtitle: row.get(4)?,
                        total_lessons: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite)
    }

    /// 一书全部课时 + 用户状态（一次查询，避免前端 N+1）。
    pub fn book_lessons(&self, book_id: &str) -> Result<Vec<LessonListEntry>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT l.id, l.book_id, l.lesson_no, l.title, l.audio_path, l.duration_ms,
                        l.sentence_count, l.vocab_count,
                        p.stage, p.position_ms, p.study_seconds, p.completed_at
                 FROM language_lessons l
                 LEFT JOIN language_lesson_progress p ON p.lesson_id = l.id
                 WHERE l.book_id = ?1
                 ORDER BY l.lesson_no",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![book_id], |row| {
                let stage =
                    LessonStage::parse(&row.get::<_, Option<String>>(8)?.unwrap_or_default());
                let position_ms = row.get::<_, Option<i64>>(9)?.unwrap_or(0);
                let study_seconds = row.get::<_, Option<i64>>(10)?.unwrap_or(0);
                let completed_at = row.get::<_, Option<i64>>(11)?;
                let status = if completed_at.is_some() {
                    LessonStatus::Completed
                } else if study_seconds > 0 || stage != LessonStage::Vocabulary || position_ms > 0 {
                    LessonStatus::Learning
                } else {
                    LessonStatus::NotStarted
                };
                let percent = if completed_at.is_some() {
                    100
                } else {
                    stage.order() * 100 / LessonStage::Done.order()
                };
                Ok(LessonListEntry {
                    lesson: CourseLesson {
                        id: row.get(0)?,
                        book_id: row.get(1)?,
                        lesson_no: row.get(2)?,
                        title: row.get(3)?,
                        audio_path: row.get(4)?,
                        duration_ms: row.get(5)?,
                        sentence_count: row.get(6)?,
                        vocab_count: row.get(7)?,
                    },
                    status,
                    percent,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    pub fn course_lesson(
        &self,
        lesson_id: &str,
    ) -> Result<Option<CourseLesson>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT id, book_id, lesson_no, title, audio_path, duration_ms,
                        sentence_count, vocab_count
                 FROM language_lessons WHERE id = ?1",
                params![lesson_id],
                |row| {
                    Ok(CourseLesson {
                        id: row.get(0)?,
                        book_id: row.get(1)?,
                        lesson_no: row.get(2)?,
                        title: row.get(3)?,
                        audio_path: row.get(4)?,
                        duration_ms: row.get(5)?,
                        sentence_count: row.get(6)?,
                        vocab_count: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn lesson_sentences(
        &self,
        lesson_id: &str,
    ) -> Result<Vec<LessonSentence>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, lesson_id, sequence, start_ms, end_ms, english, chinese
                 FROM language_sentences WHERE lesson_id = ?1 ORDER BY sequence",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![lesson_id], |row| {
                Ok(LessonSentence {
                    id: row.get(0)?,
                    lesson_id: row.get(1)?,
                    sequence: row.get(2)?,
                    start_ms: row.get(3)?,
                    end_ms: row.get(4)?,
                    english: row.get(5)?,
                    chinese: row.get(6)?,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    /// 课时生词（按学习价值排序：importance 高 → 词频低 → 字母序）。
    pub fn lesson_vocab(&self, lesson_id: &str) -> Result<Vec<LessonVocab>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT lesson_id, word, surface, sentence_id, context, phonetic, pos,
                        translation_zh, definition_en, frequency, tags, importance
                 FROM language_lesson_vocab WHERE lesson_id = ?1
                 ORDER BY importance DESC, CASE WHEN frequency = 0 THEN 999999 ELSE frequency END DESC, word",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![lesson_id], |row| {
                let tags_raw: String = row.get(10)?;
                Ok(LessonVocab {
                    lesson_id: row.get(0)?,
                    word: row.get(1)?,
                    surface: row.get(2)?,
                    sentence_id: row.get(3)?,
                    context: row.get(4)?,
                    phonetic: row.get(5)?,
                    pos: row.get(6)?,
                    translation_zh: row.get(7)?,
                    definition_en: row.get(8)?,
                    frequency: row.get(9)?,
                    tags: tags_raw
                        .split(',')
                        .filter(|tag| !tag.is_empty())
                        .map(str::to_string)
                        .collect(),
                    importance: row.get(11)?,
                    mark: None,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    /// 书本汇总统计（Book 页头）。
    pub fn book_summary(&self, book_id: &str) -> Result<BookSummary, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN p.completed_at IS NOT NULL THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN p.completed_at IS NULL
                              AND (p.study_seconds > 0 OR p.position_ms > 0
                                   OR p.stage <> 'vocabulary') THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(p.study_seconds), 0),
                        COALESCE(SUM(l.vocab_count), 0)
                 FROM language_lessons l
                 LEFT JOIN language_lesson_progress p ON p.lesson_id = l.id
                 WHERE l.book_id = ?1",
                params![book_id],
                |row| {
                    Ok(BookSummary {
                        total_lessons: row.get(0)?,
                        completed_lessons: row.get(1)?,
                        learning_lessons: row.get(2)?,
                        study_seconds: row.get(3)?,
                        vocab_total: row.get(4)?,
                    })
                },
            )
            .map_err(sqlite)
    }

    /// 最近学习过的课时（Recent Learning 列表）。
    pub fn recent_lesson_entries(
        &self,
        limit: usize,
    ) -> Result<Vec<LessonListEntry>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT l.id, l.book_id, l.lesson_no, l.title, l.audio_path, l.duration_ms,
                        l.sentence_count, l.vocab_count,
                        p.stage, p.position_ms, p.study_seconds, p.completed_at
                 FROM language_lesson_progress p
                 JOIN language_lessons l ON l.id = p.lesson_id
                 ORDER BY p.updated_at DESC LIMIT ?1",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let stage = LessonStage::parse(&row.get::<_, String>(8)?);
                let completed_at = row.get::<_, Option<i64>>(11)?;
                let status = if completed_at.is_some() {
                    LessonStatus::Completed
                } else {
                    LessonStatus::Learning
                };
                let percent = if completed_at.is_some() {
                    100
                } else {
                    stage.order() * 100 / LessonStage::Done.order()
                };
                Ok(LessonListEntry {
                    lesson: CourseLesson {
                        id: row.get(0)?,
                        book_id: row.get(1)?,
                        lesson_no: row.get(2)?,
                        title: row.get(3)?,
                        audio_path: row.get(4)?,
                        duration_ms: row.get(5)?,
                        sentence_count: row.get(6)?,
                        vocab_count: row.get(7)?,
                    },
                    status,
                    percent,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    /// 今日学习秒数（当天更新过进度的课时之和；粗略但诚实）。
    pub fn study_seconds_since(&self, day_start: i64) -> Result<i64, InfrastructureError> {
        // 注意：study_seconds 是累计值，updated_at 当天才计入。用户补学会高估当天，
        // 低估隔天——作为「今日学习」指标可接受，且比心跳表简单。
        self.conn()
            .query_row(
                "SELECT COALESCE(SUM(study_seconds), 0) FROM language_lesson_progress
                 WHERE updated_at >= ?1",
                params![day_start],
                |row| row.get(0),
            )
            .map_err(sqlite)
    }

    /// 连续学习天数（有进度更新的不同日期数，从今天往前连）。
    pub fn study_streak_days(&self, now: i64) -> Result<u32, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT DISTINCT (updated_at / 86400) AS day FROM language_lesson_progress
                 ORDER BY day DESC",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(sqlite)?;
        let mut days = Vec::new();
        for row in rows {
            days.push(row.map_err(sqlite)?);
        }
        let today = now / 86400;
        let mut streak = 0u32;
        for (index, day) in days.iter().enumerate() {
            // 允许今天还没学：连续从昨天算起。
            let expected =
                today - index as i64 - i64::from(days.first().is_some_and(|d| *d < today));
            if *day == expected {
                streak += 1;
            } else {
                break;
            }
        }
        Ok(streak)
    }

    // ====================================================================
    // 课时进度
    // ====================================================================

    pub fn lesson_progress(
        &self,
        lesson_id: &str,
    ) -> Result<Option<LessonProgress>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT lesson_id, stage, position_ms, sentence_seq, vocab_index, shadow_seq,
                        quiz_score, completed_at, study_seconds, updated_at
                 FROM language_lesson_progress WHERE lesson_id = ?1",
                params![lesson_id],
                |row| {
                    Ok(LessonProgress {
                        lesson_id: row.get(0)?,
                        stage: LessonStage::parse(&row.get::<_, String>(1)?),
                        position_ms: row.get(2)?,
                        sentence_seq: row.get(3)?,
                        vocab_index: row.get(4)?,
                        shadow_seq: row.get(5)?,
                        quiz_score: row.get(6)?,
                        completed_at: row.get(7)?,
                        study_seconds: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn save_lesson_progress(
        &self,
        progress: &LessonProgress,
    ) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO language_lesson_progress
                    (lesson_id, stage, position_ms, sentence_seq, vocab_index, shadow_seq,
                     quiz_score, completed_at, study_seconds, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(lesson_id) DO UPDATE SET
                    stage = excluded.stage, position_ms = excluded.position_ms,
                    sentence_seq = excluded.sentence_seq, vocab_index = excluded.vocab_index,
                    shadow_seq = excluded.shadow_seq, quiz_score = excluded.quiz_score,
                    completed_at = excluded.completed_at,
                    study_seconds = excluded.study_seconds, updated_at = excluded.updated_at",
                params![
                    progress.lesson_id,
                    progress.stage.as_str(),
                    progress.position_ms,
                    progress.sentence_seq,
                    progress.vocab_index,
                    progress.shadow_seq,
                    progress.quiz_score,
                    progress.completed_at,
                    progress.study_seconds,
                    progress.updated_at
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 当前学习锚点：最近更新过进度的课时（Continue 卡片用）。
    pub fn latest_learning_lesson(&self) -> Result<Option<LessonListEntry>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT l.id, l.book_id, l.lesson_no, l.title, l.audio_path, l.duration_ms,
                        l.sentence_count, l.vocab_count,
                        p.stage, p.position_ms, p.study_seconds, p.completed_at
                 FROM language_lesson_progress p
                 JOIN language_lessons l ON l.id = p.lesson_id
                 ORDER BY p.updated_at DESC LIMIT 1",
            )
            .map_err(sqlite)?;
        let mut rows = stmt
            .query_map([], |row| {
                let stage = LessonStage::parse(&row.get::<_, String>(8)?);
                let completed_at = row.get::<_, Option<i64>>(11)?;
                let status = if completed_at.is_some() {
                    LessonStatus::Completed
                } else {
                    LessonStatus::Learning
                };
                let percent = if completed_at.is_some() {
                    100
                } else {
                    stage.order() * 100 / LessonStage::Done.order()
                };
                Ok(LessonListEntry {
                    lesson: CourseLesson {
                        id: row.get(0)?,
                        book_id: row.get(1)?,
                        lesson_no: row.get(2)?,
                        title: row.get(3)?,
                        audio_path: row.get(4)?,
                        duration_ms: row.get(5)?,
                        sentence_count: row.get(6)?,
                        vocab_count: row.get(7)?,
                    },
                    status,
                    percent,
                })
            })
            .map_err(sqlite)?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(sqlite)?)),
            None => Ok(None),
        }
    }

    // ====================================================================
    // 学习计划
    // ====================================================================

    pub fn learning_plan(
        &self,
        language: &str,
    ) -> Result<Option<LearningPlan>, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT language, course_id, book_id, daily_minutes, new_words_per_day,
                        nce_source_dir, updated_at
                 FROM language_plan WHERE language = ?1",
                params![language],
                |row| {
                    Ok(LearningPlan {
                        language: devtoolbox_core::language::LanguageCode::from_code(
                            &row.get::<_, String>(0)?,
                        )
                        .unwrap_or(devtoolbox_core::language::LanguageCode::Eng),
                        course_id: row.get(1)?,
                        book_id: row.get(2)?,
                        daily_minutes: row.get(3)?,
                        new_words_per_day: row.get(4)?,
                        nce_source_dir: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn save_learning_plan(&self, plan: &LearningPlan) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO language_plan
                    (language, course_id, book_id, daily_minutes, new_words_per_day,
                     nce_source_dir, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(language) DO UPDATE SET
                    course_id = excluded.course_id, book_id = excluded.book_id,
                    daily_minutes = excluded.daily_minutes,
                    new_words_per_day = excluded.new_words_per_day,
                    nce_source_dir = excluded.nce_source_dir,
                    updated_at = excluded.updated_at",
                params![
                    plan.language.code(),
                    plan.course_id,
                    plan.book_id,
                    plan.daily_minutes,
                    plan.new_words_per_day,
                    plan.nce_source_dir,
                    plan.updated_at
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    // ====================================================================
    // 单词遇见记录
    // ====================================================================

    pub fn record_word_occurrence(
        &self,
        occurrence: &WordOccurrence,
    ) -> Result<(), InfrastructureError> {
        self.conn()
            .execute(
                "INSERT INTO language_word_occurrences
                    (word, source_type, source_id, sentence, occurred_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    occurrence.word,
                    occurrence.source_type,
                    occurrence.source_id,
                    occurrence.sentence,
                    occurrence.occurred_at
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn word_occurrences(
        &self,
        word: &str,
        limit: usize,
    ) -> Result<Vec<WordOccurrence>, InfrastructureError> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT word, source_type, source_id, sentence, occurred_at
                 FROM language_word_occurrences WHERE word = ?1
                 ORDER BY occurred_at DESC LIMIT ?2",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![word, limit as i64], |row| {
                Ok(WordOccurrence {
                    word: row.get(0)?,
                    source_type: row.get(1)?,
                    source_id: row.get(2)?,
                    sentence: row.get(3)?,
                    occurred_at: row.get(4)?,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    pub fn word_occurrence_count(&self, word: &str) -> Result<i64, InfrastructureError> {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM language_word_occurrences WHERE word = ?1",
                params![word],
                |row| row.get(0),
            )
            .map_err(sqlite)
    }

    // ====================================================================
    // ECDICT 词典
    // ====================================================================

    /// 批量写入词典条目（导入器事务内调用）。
    pub(crate) fn insert_dict_batch(
        conn: &Connection,
        batch: &[WordEntry],
    ) -> Result<(), InfrastructureError> {
        let mut stmt = conn
            .prepare(
                "INSERT OR REPLACE INTO dict_entries
                    (word, phonetic, definition_en, translation_zh, pos, collins, oxford,
                     tag, bnc, frq, exchange)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .map_err(sqlite)?;
        for entry in batch {
            let exchange = entry
                .forms
                .iter()
                .map(|(kind, form)| format!("{kind}:{form}"))
                .collect::<Vec<_>>()
                .join("/");
            stmt.execute(params![
                entry.word,
                entry.phonetic,
                entry.definition_en,
                entry.translation_zh,
                entry.pos,
                entry.collins,
                0, // oxford 标记当前未用，保留列
                entry.tags.join(" "),
                entry.bnc,
                entry.frequency,
                exchange
            ])
            .map_err(sqlite)?;
        }
        Ok(())
    }

    pub fn dict_count(&self) -> Result<i64, InfrastructureError> {
        self.conn()
            .query_row("SELECT COUNT(*) FROM dict_entries", [], |row| row.get(0))
            .map_err(sqlite)
    }

    /// 词典搜索（前缀优先，子串次之；English 首页全局搜索用）。
    pub fn dict_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<WordEntry>, InfrastructureError> {
        let normalized = query.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT word, phonetic, definition_en, translation_zh, pos, collins,
                        tag, bnc, frq, exchange
                 FROM dict_entries
                 WHERE word >= ?1 COLLATE NOCASE AND word < (?1 || x'ff') COLLATE NOCASE
                 ORDER BY word COLLATE NOCASE LIMIT ?2",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(params![normalized, limit as i64], |row| {
                let exchange: String = row.get(9)?;
                let word: String = row.get(0)?;
                let forms = parse_exchange(&exchange);
                let lemma = forms
                    .iter()
                    .find(|(kind, _)| kind == "0")
                    .map(|(_, form)| form.clone())
                    .unwrap_or_else(|| word.clone());
                Ok(WordEntry {
                    word: word.to_ascii_lowercase(),
                    lemma: lemma.to_ascii_lowercase(),
                    phonetic: row.get::<_, Option<String>>(1)?.filter(|v| !v.is_empty()),
                    definition_en: row.get::<_, Option<String>>(2)?.filter(|v| !v.is_empty()),
                    translation_zh: row.get::<_, Option<String>>(3)?.filter(|v| !v.is_empty()),
                    pos: row.get::<_, Option<String>>(4)?.filter(|v| !v.is_empty()),
                    collins: row.get::<_, i64>(5)?.max(0) as u32,
                    tags: row
                        .get::<_, String>(6)?
                        .split_whitespace()
                        .map(str::to_string)
                        .collect(),
                    bnc: row.get::<_, i64>(7)?.max(0) as u32,
                    frequency: row.get::<_, i64>(8)?.max(0) as u32,
                    forms,
                })
            })
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            let entry = row.map_err(sqlite)?;
            if entry.is_meaningful() {
                out.push(entry);
            }
        }
        Ok(out)
    }

    /// 课时标题搜索（「Lesson 17」/「Always Young」均可命中）。
    pub fn lesson_title_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CourseLesson>, InfrastructureError> {
        let normalized = query.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        // 支持 "lesson 17" / "17" 这类按编号找课。
        let no_match = normalized
            .trim_start_matches("lesson")
            .trim()
            .parse::<u32>()
            .ok();
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, book_id, lesson_no, title, audio_path, duration_ms,
                        sentence_count, vocab_count
                 FROM language_lessons
                 WHERE lower(title) LIKE '%' || ?1 || '%' OR lesson_no = ?2
                 ORDER BY book_id, lesson_no LIMIT ?3",
            )
            .map_err(sqlite)?;
        let rows = stmt
            .query_map(
                params![normalized, no_match.map(|n| n as i64), limit as i64],
                |row| {
                    Ok(CourseLesson {
                        id: row.get(0)?,
                        book_id: row.get(1)?,
                        lesson_no: row.get(2)?,
                        title: row.get(3)?,
                        audio_path: row.get(4)?,
                        duration_ms: row.get(5)?,
                        sentence_count: row.get(6)?,
                        vocab_count: row.get(7)?,
                    })
                },
            )
            .map_err(sqlite)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sqlite)?);
        }
        Ok(out)
    }

    /// 词典查询（大小写不敏感；lemma 经 exchange 的 `0:` 还原）。
    pub fn dict_lookup(&self, word: &str) -> Result<Option<WordEntry>, InfrastructureError> {
        let normalized = word.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Ok(None);
        }
        let row = self
            .conn()
            .query_row(
                "SELECT word, phonetic, definition_en, translation_zh, pos, collins,
                        tag, bnc, frq, exchange
                 FROM dict_entries WHERE word = ?1 COLLATE NOCASE",
                params![normalized],
                |row| {
                    let exchange: String = row.get(9)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        exchange,
                    ))
                },
            )
            .optional()
            .map_err(sqlite)?;
        let Some((
            word,
            phonetic,
            definition_en,
            translation_zh,
            pos,
            collins,
            tag,
            bnc,
            frq,
            exchange,
        )) = row
        else {
            return Ok(None);
        };
        let forms = parse_exchange(&exchange);
        let lemma = forms
            .iter()
            .find(|(kind, _)| kind == "0")
            .map(|(_, form)| form.clone())
            .unwrap_or_else(|| word.clone());
        let entry = WordEntry {
            word: word.to_ascii_lowercase(),
            lemma: lemma.to_ascii_lowercase(),
            phonetic: phonetic.filter(|value| !value.is_empty()),
            pos: pos.filter(|value| !value.is_empty()),
            translation_zh: translation_zh.filter(|value| !value.is_empty()),
            definition_en: definition_en.filter(|value| !value.is_empty()),
            frequency: frq.max(0) as u32,
            bnc: bnc.max(0) as u32,
            tags: tag.split_whitespace().map(str::to_string).collect(),
            collins: collins.max(0) as u32,
            forms,
        };
        Ok(if entry.is_meaningful() {
            Some(entry)
        } else {
            None
        })
    }
}

/// 解析 ECDICT `exchange` 字段（`p:played/d:played/i:playing/3:plays/s:plays/0:play/1:plays`）。
fn parse_exchange(exchange: &str) -> Vec<(String, String)> {
    exchange
        .split('/')
        .filter_map(|part| {
            let (kind, form) = part.split_once(':')?;
            if form.is_empty() {
                return None;
            }
            Some((kind.to_string(), form.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::language::LanguageCode;

    fn test_store() -> LanguageStore {
        let directory = tempfile::tempdir().expect("tempdir");
        // 用 keep() 让文件活到测试结束（tempdir 删除不影响已打开连接，但路径要有效）。
        let path = directory.keep().join("language.db");
        LanguageStore::open(path).expect("open")
    }

    fn sample_lesson() -> (CourseLesson, Vec<LessonSentence>, Vec<LessonVocab>) {
        let lesson = CourseLesson {
            id: "nce:1:1".into(),
            book_id: "nce:1".into(),
            lesson_no: 1,
            title: "Excuse Me".into(),
            audio_path: Some("/data/nce/NCE1/001.mp3".into()),
            duration_ms: Some(31_000),
            sentence_count: 2,
            vocab_count: 1,
        };
        let sentences = vec![
            LessonSentence {
                id: "nce:1:1#0".into(),
                lesson_id: "nce:1:1".into(),
                sequence: 0,
                start_ms: 610,
                end_ms: 2710,
                english: "Lesson 1".into(),
                chinese: Some("第1课".into()),
            },
            LessonSentence {
                id: "nce:1:1#1".into(),
                lesson_id: "nce:1:1".into(),
                sequence: 1,
                start_ms: 2710,
                end_ms: 5610,
                english: "Excuse me!".into(),
                chinese: Some("打扰一下！".into()),
            },
        ];
        let vocab = vec![LessonVocab {
            lesson_id: "nce:1:1".into(),
            word: "excuse".into(),
            surface: Some("Excuse".into()),
            sentence_id: Some("nce:1:1#1".into()),
            context: Some("Excuse me!".into()),
            phonetic: Some("ɪkˈskjuːs".into()),
            pos: Some("v.".into()),
            translation_zh: Some("原谅".into()),
            definition_en: None,
            frequency: 4200,
            tags: vec!["cet4".into()],
            importance: 65,
            mark: None,
        }];
        (lesson, sentences, vocab)
    }

    #[test]
    fn course_round_trip_and_idempotent_reimport() {
        let store = test_store();
        store
            .upsert_course(&Course {
                id: "nce".into(),
                language: LanguageCode::Eng,
                code: "nce".into(),
                title: "New Concept English".into(),
                description: None,
                source_type: "nce".into(),
                created_at: 100,
            })
            .expect("course");
        store
            .upsert_book(&CourseBook {
                id: "nce:1".into(),
                course_id: "nce".into(),
                book_no: 1,
                title: "NCE1".into(),
                subtitle: None,
                total_lessons: 1,
            })
            .expect("book");
        let (lesson, sentences, vocab) = sample_lesson();
        store
            .replace_lesson_content(&lesson, &sentences, &vocab)
            .expect("lesson");
        // 重导入同一份内容：行数不变（幂等）。
        store
            .replace_lesson_content(&lesson, &sentences, &vocab)
            .expect("reimport");

        assert_eq!(store.courses().expect("courses").len(), 1);
        assert_eq!(store.course_books("nce").expect("books").len(), 1);
        let lessons = store.book_lessons("nce:1").expect("lessons");
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].status, LessonStatus::NotStarted);
        assert_eq!(
            store.lesson_sentences("nce:1:1").expect("sentences").len(),
            2
        );
        let vocab_rows = store.lesson_vocab("nce:1:1").expect("vocab");
        assert_eq!(vocab_rows.len(), 1);
        assert_eq!(vocab_rows[0].tags, vec!["cet4".to_string()]);
    }

    #[test]
    fn progress_survives_content_reimport_and_drives_status() {
        let store = test_store();
        let (lesson, sentences, vocab) = sample_lesson();
        store
            .replace_lesson_content(&lesson, &sentences, &vocab)
            .expect("lesson");
        let mut progress = LessonProgress::new("nce:1:1", 1000);
        progress.stage = LessonStage::Listen;
        progress.position_ms = 12_000;
        store
            .save_lesson_progress(&progress)
            .expect("save progress");
        // 重导入教材内容后进度仍在。
        store
            .replace_lesson_content(&lesson, &sentences, &vocab)
            .expect("reimport");
        let lessons = store.book_lessons("nce:1").expect("lessons");
        assert_eq!(lessons[0].status, LessonStatus::Learning);
        assert!(lessons[0].percent > 0);
        let loaded = store
            .lesson_progress("nce:1:1")
            .expect("progress")
            .expect("some");
        assert_eq!(loaded.stage, LessonStage::Listen);
        assert_eq!(loaded.position_ms, 12_000);
    }

    #[test]
    fn plan_round_trip() {
        let store = test_store();
        assert!(store.learning_plan("eng").expect("plan").is_none());
        store
            .save_learning_plan(&LearningPlan {
                language: LanguageCode::Eng,
                course_id: Some("nce".into()),
                book_id: Some("nce:2".into()),
                daily_minutes: 45,
                new_words_per_day: 15,
                nce_source_dir: None,
                updated_at: 42,
            })
            .expect("save");
        let plan = store.learning_plan("eng").expect("plan").expect("some");
        assert_eq!(plan.book_id.as_deref(), Some("nce:2"));
        assert_eq!(plan.daily_minutes, 45);
    }

    #[test]
    fn occurrences_record_and_count() {
        let store = test_store();
        store
            .record_word_occurrence(&WordOccurrence {
                word: "hesitate".into(),
                source_type: "lesson".into(),
                source_id: "nce:2:17".into(),
                sentence: Some("Don't hesitate to ask.".into()),
                occurred_at: 100,
            })
            .expect("record");
        store
            .record_word_occurrence(&WordOccurrence {
                word: "hesitate".into(),
                source_type: "reading".into(),
                source_id: "doc:1".into(),
                sentence: None,
                occurred_at: 200,
            })
            .expect("record 2");
        assert_eq!(store.word_occurrence_count("hesitate").expect("count"), 2);
        let list = store.word_occurrences("hesitate", 10).expect("list");
        assert_eq!(list[0].occurred_at, 200); // 最新在前
    }

    #[test]
    fn dict_lookup_resolves_lemma_and_forms() {
        let store = test_store();
        store
            .conn()
            .execute(
                "INSERT INTO dict_entries
                    (word, phonetic, definition_en, translation_zh, pos, collins, oxford,
                     tag, bnc, frq, exchange)
                 VALUES ('hesitated', 'hɪˈzɪteɪtɪd', '', '犹豫', 'v.', 0, 0,
                         'cet4', 0, 9000, 'p:hesitated/0:hesitate')",
                [],
            )
            .expect("insert");
        let entry = store
            .dict_lookup("Hesitated")
            .expect("lookup")
            .expect("found");
        assert_eq!(entry.lemma, "hesitate");
        assert_eq!(entry.translation_zh.as_deref(), Some("犹豫"));
        assert!(
            entry
                .forms
                .contains(&("p".to_string(), "hesitated".to_string()))
        );
        assert!(
            store
                .dict_lookup("nonexistentword")
                .expect("miss")
                .is_none()
        );
    }

    #[test]
    fn exchange_parser_handles_real_ecdict_shape() {
        let forms = parse_exchange("p:played/d:played/i:playing/3:plays/s:plays/0:play/1:plays");
        assert_eq!(forms.len(), 7);
        assert_eq!(forms[5], ("0".to_string(), "play".to_string()));
        assert_eq!(parse_exchange(""), Vec::<(String, String)>::new());
        assert_eq!(parse_exchange("0:"), Vec::<(String, String)>::new());
    }
}
