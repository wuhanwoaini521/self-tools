//! Language Learning Hub 的 SQLite 存储（`language.db`）。
//!
//! 与 RSS/Travel/History 同款 rusqlite 方案：`Connection` 非 `Sync`，由上层 `Mutex` 串行。
//! 词典表与用户学习表严格分离（#59）：`import_items` 只动词典表，绝不触碰用户进度。
//! 搜索使用 **FTS5**（bundled SQLite 自带，已核对 libsqlite3-sys build flags）。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use devtoolbox_core::language::{
    Difficulty, LanguageCode, LanguageCount, LanguageItem, LanguageItemType, LanguageLearningItem,
    LanguageMetadata, LanguageRelation, LanguageRelationKind, LanguageSource, LearningItemType,
    Lesson, LessonPosition, LessonStep, LicenseKind, Meaning, Mistake, Pronunciation,
    PronunciationScheme, SentenceChunk, SentenceRecord, SentenceStudy, SourceLicense,
    normalize_roman,
};

use crate::error::InfrastructureError;

use super::import::{ImportReport, ImportedExample, ImportedItem, ImportedPronunciation};

/// 搜索结果命中（含匹配字段说明，供 UI 展示“为什么命中”）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub item: LanguageItem,
    pub matched: String,
}

/// 词详情所需的最小关联集合。
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemDetailRows {
    pub item: Option<LanguageItem>,
    pub meanings: Vec<Meaning>,
    pub pronunciations: Vec<Pronunciation>,
    pub relations: Vec<LanguageRelation>,
    pub related_items: Vec<LanguageItem>,
    pub examples: Vec<ImportedExample>,
    pub sentences: Vec<SentenceRecord>,
    /// item_extra JSON（kanji 元数据等）。
    pub extra: Option<serde_json::Value>,
}

pub struct LanguageStore {
    connection: Connection,
    /// 数据库文件路径。用于「重开库验证持久化」这类测试与备份。
    path: std::path::PathBuf,
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

impl LanguageStore {
    /// 打开（必要时创建）语言数据库并确保 Schema 存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(&path)
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite)?;
        let store = Self { connection, path };
        store.ensure_schema()?;
        store.seed_languages()?;
        Ok(store)
    }

    /// 数据库文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 暴露底层连接，仅供同 crate 的测试构造「旧版本」数据库。
    #[cfg(test)]
    pub(crate) fn connection_for_tests(&self) -> &Connection {
        &self.connection
    }

    fn ensure_schema(&self) -> Result<(), InfrastructureError> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS languages (
                code TEXT PRIMARY KEY, name TEXT NOT NULL, native_name TEXT NOT NULL, sort INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sources (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, homepage TEXT NOT NULL DEFAULT '',
                download_source TEXT NOT NULL DEFAULT '', dataset_version TEXT NOT NULL DEFAULT '',
                downloaded_at INTEGER, license_kind TEXT NOT NULL, license_url TEXT,
                attribution TEXT NOT NULL DEFAULT '',
                commercial_use INTEGER NOT NULL DEFAULT 0, redistribution INTEGER NOT NULL DEFAULT 0,
                share_alike INTEGER NOT NULL DEFAULT 0, attribution_required INTEGER NOT NULL DEFAULT 0,
                notes TEXT
            );
            CREATE TABLE IF NOT EXISTS dataset_manifests (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, language TEXT NOT NULL,
                version TEXT NOT NULL DEFAULT '', downloaded_at INTEGER, source_id TEXT NOT NULL,
                checksum TEXT, raw_file TEXT, record_count INTEGER NOT NULL DEFAULT 0,
                importer_version INTEGER NOT NULL DEFAULT 1, imported_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS language_items (
                id TEXT PRIMARY KEY, language TEXT NOT NULL, item_type TEXT NOT NULL,
                text TEXT NOT NULL, reading TEXT, romanization TEXT, meta_json TEXT,
                source TEXT NOT NULL, imported_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_items_language ON language_items(language, item_type);
            CREATE INDEX IF NOT EXISTS idx_items_text ON language_items(text);
            CREATE INDEX IF NOT EXISTS idx_items_roman ON language_items(romanization);
            CREATE TABLE IF NOT EXISTS meanings (
                id TEXT PRIMARY KEY, item_id TEXT NOT NULL, pos TEXT, gloss TEXT, raw TEXT,
                sense_key TEXT, lang TEXT, rank INTEGER NOT NULL DEFAULT 0, source TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_meanings_item ON meanings(item_id);
            CREATE TABLE IF NOT EXISTS pronunciations (
                id TEXT PRIMARY KEY, item_id TEXT NOT NULL, scheme TEXT NOT NULL,
                phonemes TEXT NOT NULL, tone INTEGER, variant TEXT, source TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pronunciations_item ON pronunciations(item_id);
            CREATE TABLE IF NOT EXISTS examples (
                id TEXT PRIMARY KEY, item_id TEXT NOT NULL, text TEXT NOT NULL,
                translation TEXT, source TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_examples_item ON examples(item_id);
            CREATE TABLE IF NOT EXISTS relations (
                id TEXT PRIMARY KEY, from_item_id TEXT NOT NULL, to_item_id TEXT NOT NULL,
                kind TEXT NOT NULL, note TEXT, source TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_item_id);
            CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_item_id);
            CREATE TABLE IF NOT EXISTS topics (
                id TEXT PRIMARY KEY, language TEXT NOT NULL, name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS item_topics (
                item_id TEXT NOT NULL, topic_id TEXT NOT NULL, PRIMARY KEY(item_id, topic_id)
            );
            CREATE TABLE IF NOT EXISTS item_extra (
                item_id TEXT PRIMARY KEY, json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS item_search_index (
                item_id TEXT NOT NULL, term TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'search',
                PRIMARY KEY(item_id, term, kind)
            );
            CREATE INDEX IF NOT EXISTS idx_search_index_term ON item_search_index(term);
            CREATE TABLE IF NOT EXISTS audio_assets (
                id TEXT PRIMARY KEY, item_id TEXT NOT NULL, language TEXT NOT NULL,
                text TEXT NOT NULL, voice TEXT, provider TEXT NOT NULL, audio_type TEXT NOT NULL,
                local_path TEXT, remote_source TEXT, generated_at INTEGER, source_license TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_audio_item ON audio_assets(item_id);
            -- ===== 学习内容（Lesson / 错题）=====
            --
            -- 只存「语言学习内容」：Lesson 的步骤**引用** language_items.id，不复制
            -- 词条正文（`steps_json` 里的 text 只是列表渲染用的快照）。
            -- 掌握度、复习排期、LearningEvent 一律归平台 `learning.db`
            -- （`learning_progress` / `review_cards` / `learning_events`），
            -- 本库**不再**持有第二份 SRS —— 旧的 `learning_states` / `review_logs` /
            -- `favorites` / `learning_sessions` 已在此前的双写中被删除（见 migrate）。
            CREATE TABLE IF NOT EXISTS lessons (
                id TEXT PRIMARY KEY, language TEXT NOT NULL, title TEXT NOT NULL,
                description TEXT, steps_json TEXT NOT NULL, created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_lessons_language ON lessons(language, updated_at DESC);
            CREATE TABLE IF NOT EXISTS lesson_positions (
                lesson_id TEXT PRIMARY KEY, step_index INTEGER NOT NULL, updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS mistakes (
                id TEXT PRIMARY KEY, item_id TEXT NOT NULL, item_type TEXT NOT NULL,
                language TEXT NOT NULL, content TEXT NOT NULL, question TEXT NOT NULL,
                user_answer TEXT NOT NULL, correct_answer TEXT NOT NULL,
                error_count INTEGER NOT NULL DEFAULT 1, last_missed_at INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_mistakes_dedup ON mistakes(item_id, id);
            CREATE INDEX IF NOT EXISTS idx_mistakes_recent ON mistakes(last_missed_at DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS item_fts USING fts5(
                search_key, meanings_key, item_id UNINDEXED, tokenize='unicode61'
            );
            CREATE VIEW IF NOT EXISTS sentences_view AS
            SELECT i.id, i.language, i.text,
                   json_extract(COALESCE(e.json, '{}'), '$.author') AS author,
                   json_extract(COALESCE(e.json, '{}'), '$.license') AS license,
                   i.source
            FROM language_items i LEFT JOIN item_extra e ON e.item_id = i.id
            WHERE i.item_type = 'SENTENCE';",
        ).map_err(sqlite)?;
        self.migrate_dropped_learning_tables()?;
        self.ensure_course_schema()
    }

    /// crate 内共享连接（course / nce / dict 子模块的 `impl LanguageStore` 用）。
    pub(crate) fn conn(&self) -> &Connection {
        &self.connection
    }

    /// 删除已被平台 `learning.db` 取代的重复学习表。
    ///
    /// 这四张表承载的是平台已有的能力（`learning_progress` / `review_cards` /
    /// `learning_events` / `collection_items`）。保留它们意味着同一份掌握度有两处
    /// 互不同步的副本——此前 `WordDetail` 一次点击就同时写两处。
    ///
    /// **不使用 `IF EXISTS` 判空 + 逐表 drop 的写法**：`DROP TABLE` 在表不存在时
    /// 已按 SQL 标准成功，因此这里逐条执行即可，重复运行（每次启动都会调用）是幂等的。
    fn migrate_dropped_learning_tables(&self) -> Result<(), InfrastructureError> {
        for table in [
            "learning_states",
            "review_logs",
            "favorites",
            "learning_sessions",
        ] {
            self.connection
                .execute_batch(&format!("DROP TABLE IF EXISTS {table};"))
                .map_err(sqlite)?;
        }
        Ok(())
    }

    fn seed_languages(&self) -> Result<(), InfrastructureError> {
        let languages = [
            ("eng", "English", "English", 1),
            ("jpn", "Japanese", "日本語", 2),
            ("cmn", "Mandarin", "普通话", 3),
            ("yue", "Cantonese", "廣東話", 4),
        ];
        for (code, name, native, sort) in languages {
            self.connection
                .execute(
                    "INSERT OR IGNORE INTO languages (code, name, native_name, sort) VALUES (?1, ?2, ?3, ?4)",
                    params![code, name, native, sort],
                )
                .map_err(sqlite)?;
        }
        Ok(())
    }

    // ---------------- 导入 ----------------

    /// 一次性导入一批 ImportedItem（事务内；替换同名 item 的词典数据，保留用户数据）。
    #[allow(clippy::too_many_lines)]
    pub fn import_items(
        &mut self,
        items: &[ImportedItem],
        source_id: &str,
        now: i64,
    ) -> Result<ImportReport, InfrastructureError> {
        let mut report = ImportReport::default();
        let transaction = self.connection.transaction().map_err(sqlite)?;
        for item in items {
            let existing: Option<i64> = transaction
                .query_row(
                    "SELECT 1 FROM language_items WHERE id = ?1",
                    [&item.id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(sqlite)?;
            if existing.is_some() {
                report.updated += 1;
            } else {
                report.inserted += 1;
            }
            // 先清旧词典子数据（用户表不触碰）
            transaction
                .execute("DELETE FROM meanings WHERE item_id = ?1", [&item.id])
                .map_err(sqlite)?;
            transaction
                .execute("DELETE FROM pronunciations WHERE item_id = ?1", [&item.id])
                .map_err(sqlite)?;
            transaction
                .execute("DELETE FROM examples WHERE item_id = ?1", [&item.id])
                .map_err(sqlite)?;
            transaction
                .execute(
                    "DELETE FROM relations WHERE from_item_id = ?1 OR to_item_id = ?1",
                    [&item.id],
                )
                .map_err(sqlite)?;
            transaction
                .execute("DELETE FROM item_topics WHERE item_id = ?1", [&item.id])
                .map_err(sqlite)?;
            transaction
                .execute("DELETE FROM item_extra WHERE item_id = ?1", [&item.id])
                .map_err(sqlite)?;
            transaction
                .execute(
                    "DELETE FROM item_search_index WHERE item_id = ?1",
                    [&item.id],
                )
                .map_err(sqlite)?;
            transaction
                .execute("DELETE FROM item_fts WHERE item_id = ?1", [&item.id])
                .map_err(sqlite)?;
            // 主行
            let meta_json = item
                .meta
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
            let reading = item.reading.as_deref();
            let romanization = item.romanization.as_deref();
            transaction
                .execute(
                    "INSERT OR REPLACE INTO language_items
                        (id, language, item_type, text, reading, romanization, meta_json, source, imported_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        item.id, item.language.code(), item_type_name(item.item_type), item.text,
                        reading, romanization, meta_json, source_id, now
                    ],
                )
                .map_err(sqlite)?;
            // 子数据
            for meaning in &item.meanings {
                let gloss = meaning.gloss.as_deref();
                let raw = meaning.raw.as_deref();
                let pos = meaning.pos.as_deref();
                let sense_key = meaning.sense_key.as_deref();
                let lang = meaning.lang.as_deref();
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO meanings
                            (id, item_id, pos, gloss, raw, sense_key, lang, rank, source)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![
                            meaning.id,
                            item.id,
                            pos,
                            gloss,
                            raw,
                            sense_key,
                            lang,
                            meaning.rank,
                            source_id
                        ],
                    )
                    .map_err(sqlite)?;
            }
            for pronunciation in &item.pronunciations {
                let variant = pronunciation.variant.as_deref();
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO pronunciations
                            (id, item_id, scheme, phonemes, tone, variant, source)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            pronunciation.id,
                            item.id,
                            scheme_name(pronunciation.scheme),
                            pronunciation.phonemes,
                            pronunciation.tone,
                            variant,
                            source_id
                        ],
                    )
                    .map_err(sqlite)?;
            }
            for relation in &item.relations {
                let note = relation.note.as_deref();
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO relations (id, from_item_id, to_item_id, kind, note, source)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            relation.id, relation.from_item_id, relation.to_item_id,
                            relation_kind_name(relation.kind), note, source_id
                        ],
                    )
                    .map_err(sqlite)?;
            }
            for example in &item.examples {
                let translation = example.translation.as_deref();
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO examples (id, item_id, text, translation, source)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![example.id, item.id, example.text, translation, source_id],
                    )
                    .map_err(sqlite)?;
            }
            if let Some(extra) = item.extra.as_ref() {
                let json = serde_json::to_string(extra)
                    .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
                transaction
                    .execute(
                        "INSERT OR REPLACE INTO item_extra (item_id, json) VALUES (?1, ?2)",
                        params![item.id, json],
                    )
                    .map_err(sqlite)?;
            }
            for term in &item.search_terms {
                transaction
                    .execute(
                        "INSERT OR IGNORE INTO item_search_index (item_id, term, kind)
                         VALUES (?1, ?2, 'search')",
                        params![item.id, term],
                    )
                    .map_err(sqlite)?;
            }
            // FTS5 行（含释义文本）
            let meanings_key = item
                .meanings
                .iter()
                .filter_map(|meaning| meaning.gloss.as_deref())
                .collect::<Vec<_>>()
                .join(" ");
            let search_key = build_search_key(item);
            transaction
                .execute(
                    "INSERT INTO item_fts (search_key, meanings_key, item_id) VALUES (?1, ?2, ?3)",
                    params![search_key, meanings_key, item.id],
                )
                .map_err(sqlite)?;
        }
        transaction.commit().map_err(sqlite)?;
        Ok(report)
    }

    /// 给已存在词条追加一条发音（CMUdict 对 OEWN 词条的 enrichment）。
    pub fn attach_pronunciation(
        &self,
        item_id: &str,
        pronunciation: &ImportedPronunciation,
        source_id: &str,
    ) -> Result<bool, InfrastructureError> {
        let exists: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM language_items WHERE id = ?1",
                [item_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(sqlite)?;
        if exists.is_none() {
            return Ok(false);
        }
        let variant = pronunciation.variant.as_deref();
        self.connection
            .execute(
                "INSERT OR REPLACE INTO pronunciations (id, item_id, scheme, phonemes, tone, variant, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    pronunciation.id, item_id, scheme_name(pronunciation.scheme),
                    pronunciation.phonemes, pronunciation.tone, variant, source_id
                ],
            )
            .map_err(sqlite)?;
        Ok(true)
    }

    /// 给已存在词条附加搜索词（words.hk English Index 等）。
    pub fn attach_search_terms(
        &self,
        pairs: &[(String, Vec<String>)],
    ) -> Result<usize, InfrastructureError> {
        let mut attached = 0usize;
        for (item_id, terms) in pairs {
            let exists: Option<i64> = self
                .connection
                .query_row(
                    "SELECT 1 FROM language_items WHERE id = ?1",
                    [item_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(sqlite)?;
            if exists.is_none() {
                continue;
            }
            for term in terms {
                self.connection
                    .execute(
                        "INSERT OR IGNORE INTO item_search_index (item_id, term, kind) VALUES (?1, ?2, 'search')",
                        params![item_id, term],
                    )
                    .map_err(sqlite)?;
                attached += 1;
            }
        }
        Ok(attached)
    }

    // ---------------- 来源 / 清单 ----------------

    pub fn upsert_source(&self, source: &LanguageSource) -> Result<(), InfrastructureError> {
        let license = &source.license;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO sources (
                    id, name, homepage, download_source, dataset_version, downloaded_at,
                    license_kind, license_url, attribution, commercial_use, redistribution,
                    share_alike, attribution_required, notes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    source.id,
                    source.name,
                    source.homepage,
                    source.download_source,
                    source.dataset_version,
                    source.downloaded_at,
                    license_kind_name(license.kind),
                    source.license_url,
                    source.attribution,
                    license.commercial_use_allowed,
                    license.redistribution_allowed,
                    license.share_alike_required,
                    license.attribution_required,
                    source.notes
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn sources(&self) -> Result<Vec<LanguageSource>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, homepage, download_source, dataset_version, downloaded_at,
                        license_kind, license_url, attribution, commercial_use, redistribution,
                        share_alike, attribution_required, notes
                 FROM sources ORDER BY id",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([], map_source)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    pub fn insert_manifest(
        &self,
        manifest: &devtoolbox_core::language::DatasetManifest,
    ) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO dataset_manifests (
                    id, name, language, version, downloaded_at, source_id, checksum, raw_file,
                    record_count, importer_version, imported_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    manifest.id,
                    manifest.name,
                    manifest.language,
                    manifest.version,
                    manifest.downloaded_at,
                    manifest.source_id,
                    manifest.checksum,
                    manifest.raw_file,
                    manifest.record_count,
                    manifest.importer_version,
                    manifest.imported_at
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn manifests(
        &self,
    ) -> Result<Vec<devtoolbox_core::language::DatasetManifest>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, language, version, downloaded_at, source_id, checksum, raw_file,
                        record_count, importer_version, imported_at
                 FROM dataset_manifests ORDER BY imported_at DESC",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([], |row| {
                Ok(devtoolbox_core::language::DatasetManifest {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    language: row.get(2)?,
                    version: row.get(3)?,
                    downloaded_at: row.get(4)?,
                    source_id: row.get(5)?,
                    checksum: row.get(6)?,
                    raw_file: row.get(7)?,
                    record_count: row.get(8)?,
                    importer_version: row.get(9)?,
                    imported_at: row.get(10)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    // ---------------- 搜索 ----------------

    /// 统一搜索：text / reading / romanization / meaning / 英语索引（#49）。
    /// 排名：精确 text > text 前缀 > reading > romanization(规范化) > meaning(FTS) > english index。
    #[allow(clippy::too_many_lines)]
    pub fn search(
        &self,
        language: Option<LanguageCode>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, InfrastructureError> {
        let raw = query.trim();
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let lower = raw.to_lowercase();
        let roman = normalize_roman(&lower);
        let lang_filter = language.map(|code| code.code().to_string());
        let limit = limit.clamp(1, 100) as i64;

        let mut ranked: Vec<(i64, String, LanguageItem)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // 1) text 精确 / 前缀（利用 idx_items_text 索引）
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, language, item_type, text, reading, romanization, meta_json, source
                 FROM language_items
                 WHERE (?1 IS NULL OR language = ?1) AND text = ?2 LIMIT ?3",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(params![lang_filter, raw, limit], map_item)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        for item in rows {
            push_ranked(&mut ranked, &mut seen, 0, "exact", item);
        }

        // 2) reading 精确（たべる 等）
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, language, item_type, text, reading, romanization, meta_json, source
                 FROM language_items
                 WHERE (?1 IS NULL OR language = ?1) AND reading = ?2 LIMIT ?3",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(params![lang_filter, raw, limit], map_item)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        for item in rows {
            push_ranked(&mut ranked, &mut seen, 1, "reading", item);
        }

        // 3) romanization 精确/前缀（taberu、sik6 faan6、lü3 xing2）
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, language, item_type, text, reading, romanization, meta_json, source
                 FROM language_items
                 WHERE (?1 IS NULL OR language = ?1) AND (lower(romanization) = ?2
                    OR lower(romanization) LIKE ?3) LIMIT ?4",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(
                params![lang_filter, roman, format!("{roman}%"), limit],
                map_item,
            )
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        for item in rows {
            push_ranked(&mut ranked, &mut seen, 2, "romanization", item);
        }

        // 4) FTS5：搜索键 + 释义键（前缀查询，兼容 CJK 整词）
        if !roman.is_empty() {
            let fts_query = build_fts_query(&roman);
            let mut statement = self
                .connection
                .prepare("SELECT item_id FROM item_fts WHERE item_fts MATCH ?1 LIMIT ?2")
                .map_err(sqlite)?;
            let rows = statement
                .query_map(params![fts_query, limit * 4], |row| row.get::<_, String>(0))
                .map_err(sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite)?;
            let matched: Vec<String> = rows;
            for item_id in matched {
                if let Some(item) = self.item(&item_id)?.filter(|item| {
                    lang_filter
                        .as_deref()
                        .is_none_or(|lang| lang == item.language.code())
                }) {
                    push_ranked(&mut ranked, &mut seen, 3, "meaning", item);
                }
            }
        }

        // 5) LIKE 兜底：CJK 子串 / 长词内嵌（徒步旅行 中的 旅行）
        if ranked.is_empty() || query.chars().any(is_cjk) {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT id, language, item_type, text, reading, romanization, meta_json, source
                     FROM language_items
                     WHERE (?1 IS NULL OR language = ?1) AND (text LIKE ('%' || ?2 || '%') OR reading LIKE ('%' || ?2 || '%'))
                     ORDER BY length(text) LIMIT ?3",
                )
                .map_err(sqlite)?;
            let rows = statement
                .query_map(params![lang_filter, raw, limit], map_item)
                .map_err(sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite)?;
            for item in rows {
                push_ranked(&mut ranked, &mut seen, 4, "text-like", item);
            }
        }

        // 6) 英语索引（words.hk English Index：food → 食嘢…）
        let mut statement = self
            .connection
            .prepare(
                "SELECT item_id FROM item_search_index WHERE term = ?1 OR term LIKE ?2 LIMIT ?3",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(params![lower, format!("{lower}%"), limit], |row| {
                row.get::<_, String>(0)
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        for item_id in rows {
            if seen.contains(&item_id) {
                continue;
            }
            if let Some(item) = self.item(&item_id)?.filter(|item| {
                lang_filter
                    .as_deref()
                    .is_none_or(|lang| lang == item.language.code())
            }) {
                push_ranked(&mut ranked, &mut seen, 5, "english-index", item);
            }
        }

        ranked.sort_by_key(|(rank, _, _)| *rank);
        Ok(ranked
            .into_iter()
            .take(limit as usize)
            .map(|(_, matched, item)| SearchHit { item, matched })
            .collect())
    }

    /// 读取单个词条（含关联数据，#63）。
    pub fn item_detail(&self, id: &str) -> Result<ItemDetailRows, InfrastructureError> {
        let item = self.item(id)?;
        let meanings = self.meanings(id)?;
        let pronunciations = self.pronunciations(id)?;
        let relations = self.relations(id)?;
        let mut related_items = Vec::new();
        for relation in &relations {
            let counterpart = if relation.from_item_id == id {
                Some(relation.to_item_id.as_str())
            } else {
                Some(relation.from_item_id.as_str())
            };
            let related = match counterpart {
                Some(other) => self.item(other)?,
                None => None,
            };
            if let Some(related) = related {
                related_items.push(related);
            }
        }
        let examples = self.examples(id)?;
        let sentences = self.sentences_for_text(
            &item
                .as_ref()
                .map(|item| item.text.clone())
                .unwrap_or_default(),
        )?;
        let extra = self.item_extra(id)?;
        Ok(ItemDetailRows {
            item,
            meanings,
            pronunciations,
            relations,
            related_items,
            examples,
            sentences,
            extra,
        })
    }

    pub fn item(&self, id: &str) -> Result<Option<LanguageItem>, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT id, language, item_type, text, reading, romanization, meta_json, source
                 FROM language_items WHERE id = ?1",
                [id],
                map_item,
            )
            .optional()
            .map_err(sqlite)
    }

    fn meanings(&self, item_id: &str) -> Result<Vec<Meaning>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, item_id, pos, gloss, raw, sense_key, lang, rank, source
                 FROM meanings WHERE item_id = ?1 ORDER BY rank",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([item_id], |row| {
                Ok(Meaning {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    pos: row.get(2)?,
                    gloss: row.get(3)?,
                    raw: row.get(4)?,
                    sense_key: row.get(5)?,
                    lang: row.get(6)?,
                    rank: row.get(7)?,
                    source: row.get(8)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    fn pronunciations(&self, item_id: &str) -> Result<Vec<Pronunciation>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, item_id, scheme, phonemes, tone, variant, source
                 FROM pronunciations WHERE item_id = ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([item_id], |row| {
                Ok(Pronunciation {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    scheme: scheme_from_name(&row.get::<_, String>(2)?),
                    phonemes: row.get(3)?,
                    tone: row.get(4)?,
                    variant: row.get(5)?,
                    source: row.get(6)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    fn relations(&self, item_id: &str) -> Result<Vec<LanguageRelation>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, from_item_id, to_item_id, kind, note, source
                 FROM relations WHERE from_item_id = ?1 OR to_item_id = ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([item_id], |row| {
                Ok(LanguageRelation {
                    id: row.get(0)?,
                    from_item_id: row.get(1)?,
                    to_item_id: row.get(2)?,
                    kind: relation_kind_from_name(&row.get::<_, String>(3)?),
                    note: row.get(4)?,
                    source: row.get(5)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    fn examples(&self, item_id: &str) -> Result<Vec<ImportedExample>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, item_id, text, translation, source FROM examples WHERE item_id = ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([item_id], |row| {
                Ok(ImportedExample {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    text: row.get(2)?,
                    translation: row.get(3)?,
                    source: row.get(4)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    /// 含指定文本的句子（例句展示用；LIKE 命中，离线可算）。
    pub fn sentences_for_text(
        &self,
        text: &str,
    ) -> Result<Vec<SentenceRecord>, InfrastructureError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, language, text, author, license, source FROM sentences_view
                 WHERE text LIKE ('%' || ?1 || '%') ORDER BY length(text) LIMIT 12",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([text], map_sentence)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    /// 按语言取句子（听力/口语用）。
    pub fn sentences_by_language(
        &self,
        language: LanguageCode,
        limit: usize,
    ) -> Result<Vec<SentenceRecord>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, language, text, author, license, source FROM sentences_view
                 WHERE language = ?1 ORDER BY length(text) LIMIT ?2",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(params![language.code(), limit as i64], map_sentence)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    // ---------------- 学习内容：Lesson / 错题 / 句子拆解 ----------------
    //
    // 掌握度、复习排期、事件流归平台 `learning.db`；本库只存语言内容本身。

    /// 读取多个学习条目（含释义与首个读音），用于组装 Lesson 步骤。
    ///
    /// 单条查询 + 逐项补全，避免在调用方形成 N+1。
    pub fn learning_items(
        &self,
        item_ids: &[String],
    ) -> Result<Vec<LanguageLearningItem>, InfrastructureError> {
        let mut out = Vec::with_capacity(item_ids.len());
        for id in item_ids {
            if let Some(item) = self.learning_item(id)? {
                out.push(item);
            }
        }
        Ok(out)
    }

    /// 取「还没学过」的词，作为学习卡片的来源。
    ///
    /// 「学过」的判定在平台 `learning_progress` 里，语言库看不到，因此由调用方
    /// 把已学 id 传进来排除（单点真相仍在平台）。排除列表只生成占位符，
    /// 值一律走绑定参数，不做字符串拼接。
    ///
    /// 排序用 `imported_at, id` 保证稳定——同一批数据每次进来给的是同一批词，
    /// 不会刷新一次换一批。
    pub fn next_new_items(
        &self,
        language: LanguageCode,
        exclude: &[String],
        limit: usize,
    ) -> Result<Vec<LanguageLearningItem>, InfrastructureError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut sql = String::from(
            "SELECT id FROM language_items
             WHERE language = ?1 AND item_type = 'WORD'
               AND source <> 'cmudict'",
        );
        if !exclude.is_empty() {
            sql.push_str(" AND id NOT IN (");
            for index in 0..exclude.len() {
                if index > 0 {
                    sql.push(',');
                }
                sql.push_str(&format!("?{}", index + 2));
            }
            sql.push(')');
        }
        sql.push_str(&format!(
            " ORDER BY imported_at, id LIMIT ?{}",
            exclude.len() + 2
        ));

        // 绑定顺序必须与占位符一致：`?1` = 语言，`?2..` = 排除列表，最后一个 = limit。
        // （先前把 limit 绑在 `?1` 上，`WHERE language = ?1` 拿到的是 limit，
        //  于是任何语言都查不到新词。）
        let code = language.code();
        let limit_value = limit as i64;
        let mut statement = self.connection.prepare(&sql).map_err(sqlite)?;
        let ids = {
            let bound: Vec<&dyn rusqlite::ToSql> = std::iter::once(&code as &dyn rusqlite::ToSql)
                .chain(exclude.iter().map(|id| id as &dyn rusqlite::ToSql))
                .chain(std::iter::once(&limit_value as &dyn rusqlite::ToSql))
                .collect();
            let rows = statement
                .query_map(bound.as_slice(), |row| row.get::<_, String>(0))
                .map_err(sqlite)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sqlite)?
        };

        self.learning_items(&ids)
    }

    /// 单个学习条目：适配为 [`LanguageLearningItem`]。
    ///
    /// 难度由**真实**的 `mistakes.error_count` 推导（没答错过就是 `Unknown`），
    /// 不猜测考纲级别。
    pub fn learning_item(
        &self,
        item_id: &str,
    ) -> Result<Option<LanguageLearningItem>, InfrastructureError> {
        let Some(item) = self.item(item_id)? else {
            return Ok(None);
        };
        let translation = self.meanings(item_id)?.first().and_then(|meaning| {
            meaning
                .gloss
                .clone()
                .or_else(|| meaning.raw.clone())
                .filter(|gloss| !gloss.trim().is_empty())
        });
        let pronunciation = self
            .pronunciations(item_id)?
            .first()
            .map(|pronunciation| pronunciation.phonemes.clone())
            .filter(|phonemes| !phonemes.trim().is_empty());

        let incorrect_count: u32 = self
            .connection
            .query_row(
                "SELECT COALESCE(SUM(error_count), 0) FROM mistakes WHERE item_id = ?1",
                [item_id],
                |row| row.get(0),
            )
            .map_err(sqlite)?;

        Ok(LanguageLearningItem::from_item(
            &item,
            translation,
            pronunciation,
            Difficulty::derive(incorrect_count),
        ))
    }

    // ---------------- Lesson ----------------

    pub fn upsert_lesson(&self, lesson: &Lesson) -> Result<(), InfrastructureError> {
        let steps = serde_json::to_string(&lesson.steps).map_err(|error| {
            InfrastructureError::Sqlite(format!("lesson steps 序列化失败：{error}"))
        })?;
        self.connection
            .execute(
                "INSERT INTO lessons (id, language, title, description, steps_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    steps_json = excluded.steps_json,
                    updated_at = excluded.updated_at",
                params![
                    lesson.id,
                    lesson.language.code(),
                    lesson.title,
                    lesson.description,
                    steps,
                    lesson.created_at,
                    lesson.updated_at,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn lesson(&self, lesson_id: &str) -> Result<Option<Lesson>, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT id, language, title, description, steps_json, created_at, updated_at
                 FROM lessons WHERE id = ?1",
                [lesson_id],
                map_lesson,
            )
            .optional()
            .map_err(sqlite)
    }

    /// 该语言的 Lesson 列表（按最近更新）。
    pub fn lessons(
        &self,
        language: Option<LanguageCode>,
        limit: usize,
    ) -> Result<Vec<Lesson>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, language, title, description, steps_json, created_at, updated_at
                 FROM lessons
                 WHERE (?1 IS NULL OR language = ?1)
                 ORDER BY updated_at DESC LIMIT ?2",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map(
                params![language.map(LanguageCode::code), limit as i64],
                map_lesson,
            )
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    pub fn delete_lesson(&mut self, lesson_id: &str) -> Result<(), InfrastructureError> {
        let transaction = self.connection.transaction().map_err(sqlite)?;
        transaction
            .execute(
                "DELETE FROM lesson_positions WHERE lesson_id = ?1",
                [lesson_id],
            )
            .map_err(sqlite)?;
        transaction
            .execute("DELETE FROM lessons WHERE id = ?1", [lesson_id])
            .map_err(sqlite)?;
        transaction.commit().map_err(sqlite)
    }

    /// 保存学习进度位置（「继续学习」依赖它跨会话恢复）。
    pub fn save_lesson_position(
        &self,
        position: &LessonPosition,
    ) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "INSERT INTO lesson_positions (lesson_id, step_index, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(lesson_id) DO UPDATE SET
                    step_index = excluded.step_index,
                    updated_at = excluded.updated_at",
                params![position.lesson_id, position.step_index, position.updated_at],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn lesson_position(
        &self,
        lesson_id: &str,
    ) -> Result<Option<LessonPosition>, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT lesson_id, step_index, updated_at FROM lesson_positions WHERE lesson_id = ?1",
                [lesson_id],
                |row| {
                    Ok(LessonPosition {
                        lesson_id: row.get(0)?,
                        step_index: row.get::<_, i64>(1)?.max(0) as usize,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite)
    }

    /// 最近学过的 Lesson（Continue 学习入口）。
    pub fn recent_lesson_positions(
        &self,
        limit: usize,
    ) -> Result<Vec<LessonPosition>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT lesson_id, step_index, updated_at FROM lesson_positions
                 ORDER BY updated_at DESC LIMIT ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([limit as i64], |row| {
                Ok(LessonPosition {
                    lesson_id: row.get(0)?,
                    step_index: row.get::<_, i64>(1)?.max(0) as usize,
                    updated_at: row.get(2)?,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    // ---------------- 错题 ----------------

    /// 记录一次答错。同一 `(item_id, card_id)` 再次答错时**累加**错误次数，
    /// 而不是插入重复行——否则反复答错会无限堆积同一条记录。
    pub fn record_mistake(
        &self,
        mistake: &Mistake,
        card_id: &str,
    ) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "INSERT INTO mistakes
                    (id, item_id, item_type, language, content, question, user_answer,
                     correct_answer, error_count, last_missed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9)
                 ON CONFLICT(item_id, id) DO UPDATE SET
                    user_answer = excluded.user_answer,
                    error_count = mistakes.error_count + 1,
                    last_missed_at = excluded.last_missed_at",
                params![
                    card_id,
                    mistake.item_id,
                    mistake.item_type.as_str(),
                    mistake.language.code(),
                    mistake.content,
                    mistake.question,
                    mistake.user_answer,
                    mistake.correct_answer,
                    mistake.last_missed_at,
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 错题列表（最近答错优先）。
    pub fn mistakes(&self, limit: usize) -> Result<Vec<Mistake>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, item_id, item_type, language, content, question, user_answer,
                        correct_answer, error_count, last_missed_at
                 FROM mistakes ORDER BY last_missed_at DESC LIMIT ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([limit as i64], map_mistake)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    /// 答对后移除该错题（「再次掌握」）。
    pub fn resolve_mistake(&self, item_id: &str, card_id: &str) -> Result<(), InfrastructureError> {
        self.connection
            .execute(
                "DELETE FROM mistakes WHERE item_id = ?1 AND id = ?2",
                params![item_id, card_id],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    pub fn mistake_count(&self) -> Result<i64, InfrastructureError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM mistakes", [], |row| row.get(0))
            .map_err(sqlite)
    }

    // ---------------- 句子拆解 ----------------

    /// 句子学习视图。
    ///
    /// 切分策略：对 CJK 用**词典最长匹配**，对拉丁文按空白/标点切分。
    /// 命中的词才带 `item_id` 与释义；查不到的片段原样保留（`meaning: None`），
    /// 不猜测。这样「词典没有收录」在 UI 上是可见的事实，而不是被编造的解释掩盖。
    pub fn sentence_study(
        &self,
        sentence_id: &str,
    ) -> Result<Option<SentenceStudy>, InfrastructureError> {
        let Some(item) = self.item(sentence_id)? else {
            return Ok(None);
        };
        let sentence = SentenceRecord {
            sentence_id: item.id.clone(),
            language: item.language,
            text: item.text.clone(),
            author: None,
            license: String::new(),
            source: item.source.clone(),
        };

        let extra = self.item_extra(sentence_id)?;
        let author = extra
            .as_ref()
            .and_then(|value| value.get("author"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let license = extra
            .as_ref()
            .and_then(|value| value.get("license"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        // 译文：Tatoeba 的译文以同语言对的形式存在 examples 表里；
        // 找不到就是 None。
        let translation = self
            .examples(sentence_id)?
            .into_iter()
            .find_map(|example| example.translation.filter(|text| !text.trim().is_empty()));

        let chunks = self.segment_sentence(&item.text, item.language)?;
        let key_words = chunks
            .iter()
            .filter_map(|chunk| chunk.item_id.clone())
            .collect();

        Ok(Some(SentenceStudy {
            id: item.id.clone(),
            language: item.language,
            original: sentence.text,
            translation,
            reading: item.reading.clone(),
            romanization: item.romanization.clone(),
            chunks,
            key_words,
            grammar: None,
            usage: None,
            license,
            author,
        }))
    }

    /// 词典最长匹配切分。
    fn segment_sentence(
        &self,
        text: &str,
        language: LanguageCode,
    ) -> Result<Vec<SentenceChunk>, InfrastructureError> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut index = 0usize;

        while index < chars.len() {
            let current = chars[index];

            // 空白与标点：独立成块，不查词典。
            if current.is_whitespace() || is_punctuation(current) {
                let start = index;
                while index < chars.len()
                    && (chars[index].is_whitespace() || is_punctuation(chars[index]))
                {
                    index += 1;
                }
                chunks.push(SentenceChunk {
                    text: chars[start..index].iter().collect(),
                    item_id: None,
                    meaning: None,
                    reading: None,
                });
                continue;
            }

            // CJK：词典最长匹配（最长 8 字，覆盖常见多字词）。
            if is_cjk(current) {
                let start = index;
                let mut end = chars.len().min(index + 8);
                let mut matched: Option<(usize, DictionaryHit)> = None;
                while end > index {
                    let candidate: String = chars[index..end].iter().collect();
                    if let Some(hit) = self.lookup_word(&candidate, language)? {
                        matched = Some((end, hit));
                        break;
                    }
                    end -= 1;
                }
                if let Some((stop, hit)) = matched {
                    chunks.push(SentenceChunk {
                        item_id: Some(hit.item_id),
                        meaning: hit.meaning,
                        reading: hit.reading,
                        text: chars[start..stop].iter().collect(),
                    });
                    index = stop;
                } else {
                    chunks.push(SentenceChunk {
                        text: current.to_string(),
                        item_id: None,
                        meaning: None,
                        reading: None,
                    });
                    index += 1;
                }
                continue;
            }

            // 拉丁字母 / 数字：连续字符成词。
            let start = index;
            while index < chars.len()
                && (chars[index].is_alphanumeric() || chars[index] == '\'')
                && !is_cjk(chars[index])
            {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            let hit = self.lookup_word(&word, language)?;
            chunks.push(SentenceChunk {
                item_id: hit.as_ref().map(|found| found.item_id.clone()),
                meaning: hit.as_ref().and_then(|found| found.meaning.clone()),
                reading: hit.and_then(|found| found.reading),
                text: word,
            });
        }

        Ok(chunks)
    }

    /// 查一个词条，返回 [`DictionaryHit`]。
    fn lookup_word(
        &self,
        text: &str,
        language: LanguageCode,
    ) -> Result<Option<DictionaryHit>, InfrastructureError> {
        if text.is_empty() {
            return Ok(None);
        }
        let id: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM language_items
                 WHERE text = ?1 AND language = ?2 AND item_type IN ('WORD', 'PHRASE')
                 ORDER BY LENGTH(text) DESC LIMIT 1",
                params![text, language.code()],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        let Some(id) = id else { return Ok(None) };
        let meaning = self.meanings(&id)?.first().and_then(|meaning| {
            meaning
                .gloss
                .clone()
                .or_else(|| meaning.raw.clone())
                .filter(|gloss| !gloss.trim().is_empty())
        });
        let reading = self.item(&id).ok().flatten().and_then(|item| item.reading);
        Ok(Some(DictionaryHit {
            item_id: id,
            meaning,
            reading,
        }))
    }

    /// 每语言条目统计（Settings → Language Data）。
    pub fn language_counts(&self) -> Result<Vec<LanguageCount>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT language,
                        SUM(CASE WHEN item_type = 'WORD' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN item_type = 'PHRASE' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN item_type = 'SENTENCE' THEN 1 ELSE 0 END),
                        COUNT(*)
                 FROM language_items GROUP BY language",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([], |row| {
                let code = row.get::<_, String>(0)?;
                LanguageCode::from_code(&code)
                    .map(|language| {
                        Ok(LanguageCount {
                            language,
                            words: row.get(1)?,
                            phrases: row.get(2)?,
                            sentences: row.get(3)?,
                            total: row.get(4)?,
                        })
                    })
                    .unwrap_or_else(|| Err(rusqlite::Error::InvalidColumnName(code)))
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    pub fn item_extra(&self, id: &str) -> Result<Option<serde_json::Value>, InfrastructureError> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT json FROM item_extra WHERE item_id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        json.map(|text| serde_json::from_str(&text))
            .transpose()
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))
    }

    pub fn source_by_id(&self, id: &str) -> Result<Option<LanguageSource>, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT id, name, homepage, download_source, dataset_version, downloaded_at,
                        license_kind, license_url, attribution, commercial_use, redistribution,
                        share_alike, attribution_required, notes
                 FROM sources WHERE id = ?1",
                [id],
                map_source,
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn count_by_source(&self, source_id: &str) -> Result<i64, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM language_items WHERE source = ?1",
                [source_id],
                |row| row.get(0),
            )
            .map_err(sqlite)
    }
    pub fn total_items(&self) -> Result<i64, InfrastructureError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM language_items", [], |row| row.get(0))
            .map_err(sqlite)
    }
}

fn push_ranked(
    ranked: &mut Vec<(i64, String, LanguageItem)>,
    seen: &mut std::collections::HashSet<String>,
    rank: i64,
    matched: &str,
    item: LanguageItem,
) {
    if seen.insert(item.id.clone()) {
        ranked.push((rank, matched.to_string(), item));
    }
}

// ---------------- 映射函数 ----------------

fn map_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LanguageItem> {
    let code = row.get::<_, String>(1)?;
    let language = LanguageCode::from_code(&code).ok_or_else(|| {
        rusqlite::Error::InvalidColumnName(format!("unknown language code: {code}"))
    })?;
    Ok(LanguageItem {
        id: row.get(0)?,
        language,
        item_type: item_type_from_name(&row.get::<_, String>(2)?),
        text: row.get(3)?,
        reading: row.get(4)?,
        romanization: row.get(5)?,
        meta: parse_meta(row.get::<_, Option<String>>(6)?)?,
        source: row.get(7)?,
    })
}

fn parse_meta(json: Option<String>) -> rusqlite::Result<Option<LanguageMetadata>> {
    match json {
        None => Ok(None),
        Some(text) => serde_json::from_str(&text).map_err(|source| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(source),
            )
        }),
    }
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<LanguageSource> {
    let license_kind = license_kind_from_name(&row.get::<_, String>(6)?);
    Ok(LanguageSource {
        id: row.get(0)?,
        name: row.get(1)?,
        homepage: row.get(2)?,
        download_source: row.get(3)?,
        dataset_version: row.get(4)?,
        downloaded_at: row.get(5)?,
        license: SourceLicense {
            kind: license_kind,
            attribution_required: row.get::<_, i64>(12)? != 0,
            commercial_use_allowed: row.get::<_, i64>(9)? != 0,
            redistribution_allowed: row.get::<_, i64>(10)? != 0,
            share_alike_required: row.get::<_, i64>(11)? != 0,
        },
        license_url: row.get(7)?,
        attribution: row.get(8)?,
        commercial_use: row.get::<_, i64>(9)? != 0,
        redistribution: row.get::<_, i64>(10)? != 0,
        notes: row.get(13)?,
    })
}

fn map_sentence(row: &rusqlite::Row<'_>) -> rusqlite::Result<SentenceRecord> {
    let code = row.get::<_, String>(1)?;
    let language = LanguageCode::from_code(&code).ok_or_else(|| {
        rusqlite::Error::InvalidColumnName(format!("unknown language code: {code}"))
    })?;
    Ok(SentenceRecord {
        sentence_id: row.get(0)?,
        language,
        text: row.get(2)?,
        author: row.get(3)?,
        license: row.get(4)?,
        source: row.get(5)?,
    })
}

fn map_lesson(row: &rusqlite::Row<'_>) -> rusqlite::Result<Lesson> {
    let code = row.get::<_, String>(1)?;
    let language = LanguageCode::from_code(&code).ok_or_else(|| {
        rusqlite::Error::InvalidColumnName(format!("unknown language code: {code}"))
    })?;
    let steps_json = row.get::<_, String>(4)?;
    let steps: Vec<LessonStep> = serde_json::from_str(&steps_json).map_err(|error| {
        rusqlite::Error::InvalidColumnName(format!("lesson steps 解析失败：{error}"))
    })?;
    Ok(Lesson {
        id: row.get(0)?,
        language,
        title: row.get(2)?,
        description: row.get(3)?,
        steps,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_mistake(row: &rusqlite::Row<'_>) -> rusqlite::Result<Mistake> {
    let code = row.get::<_, String>(3)?;
    let language = LanguageCode::from_code(&code).ok_or_else(|| {
        rusqlite::Error::InvalidColumnName(format!("unknown language code: {code}"))
    })?;
    let item_type_name = row.get::<_, String>(2)?;
    let item_type = LearningItemType::parse(&item_type_name).ok_or_else(|| {
        rusqlite::Error::InvalidColumnName(format!("unknown learning item type: {item_type_name}"))
    })?;
    let error_count = row.get::<_, i64>(8)?.max(0) as u32;
    Ok(Mistake {
        id: row.get(0)?,
        item_id: row.get(1)?,
        item_type,
        language,
        content: row.get(4)?,
        question: row.get(5)?,
        user_answer: row.get(6)?,
        correct_answer: row.get(7)?,
        error_count,
        last_missed_at: row.get(9)?,
    })
}

fn item_type_name(kind: LanguageItemType) -> &'static str {
    match kind {
        LanguageItemType::Word => "WORD",
        LanguageItemType::Phrase => "PHRASE",
        LanguageItemType::Sentence => "SENTENCE",
        LanguageItemType::Dialogue => "DIALOGUE",
        LanguageItemType::Passage => "PASSAGE",
        LanguageItemType::Grammar => "GRAMMAR",
        LanguageItemType::Pronunciation => "PRONUNCIATION",
    }
}

fn item_type_from_name(name: &str) -> LanguageItemType {
    match name {
        "PHRASE" => LanguageItemType::Phrase,
        "SENTENCE" => LanguageItemType::Sentence,
        "DIALOGUE" => LanguageItemType::Dialogue,
        "PASSAGE" => LanguageItemType::Passage,
        "GRAMMAR" => LanguageItemType::Grammar,
        "PRONUNCIATION" => LanguageItemType::Pronunciation,
        _ => LanguageItemType::Word,
    }
}

fn scheme_name(scheme: PronunciationScheme) -> &'static str {
    match scheme {
        PronunciationScheme::Arpabet => "ARPABET",
        PronunciationScheme::Ipa => "IPA",
        PronunciationScheme::Pinyin => "PINYIN",
        PronunciationScheme::Jyutping => "JYUTPING",
        PronunciationScheme::Kana => "KANA",
        PronunciationScheme::Romaji => "ROMAJI",
    }
}

fn scheme_from_name(name: &str) -> PronunciationScheme {
    match name {
        "ARPABET" => PronunciationScheme::Arpabet,
        "IPA" => PronunciationScheme::Ipa,
        "PINYIN" => PronunciationScheme::Pinyin,
        "JYUTPING" => PronunciationScheme::Jyutping,
        "KANA" => PronunciationScheme::Kana,
        _ => PronunciationScheme::Romaji,
    }
}

fn relation_kind_name(kind: LanguageRelationKind) -> &'static str {
    match kind {
        LanguageRelationKind::Synonym => "SYNONYM",
        LanguageRelationKind::Antonym => "ANTONYM",
        LanguageRelationKind::FormOf => "FORM_OF",
        LanguageRelationKind::RelatedTo => "RELATED_TO",
        LanguageRelationKind::UsedIn => "USED_IN",
        LanguageRelationKind::TranslationOf => "TRANSLATION_OF",
        LanguageRelationKind::BelongsToTopic => "BELONGS_TO_TOPIC",
        LanguageRelationKind::Hypernym => "HYPERNYM",
        LanguageRelationKind::Hyponym => "HYPONYM",
        LanguageRelationKind::Attribute => "ATTRIBUTE",
        LanguageRelationKind::DomainTopic => "DOMAIN_TOPIC",
        LanguageRelationKind::Derivation => "DERIVATION",
    }
}

fn relation_kind_from_name(name: &str) -> LanguageRelationKind {
    match name {
        "SYNONYM" => LanguageRelationKind::Synonym,
        "ANTONYM" => LanguageRelationKind::Antonym,
        "FORM_OF" => LanguageRelationKind::FormOf,
        "RELATED_TO" => LanguageRelationKind::RelatedTo,
        "USED_IN" => LanguageRelationKind::UsedIn,
        "TRANSLATION_OF" => LanguageRelationKind::TranslationOf,
        "BELONGS_TO_TOPIC" => LanguageRelationKind::BelongsToTopic,
        "HYPERNYM" => LanguageRelationKind::Hypernym,
        "HYPONYM" => LanguageRelationKind::Hyponym,
        "ATTRIBUTE" => LanguageRelationKind::Attribute,
        "DOMAIN_TOPIC" => LanguageRelationKind::DomainTopic,
        "DERIVATION" => LanguageRelationKind::Derivation,
        _ => LanguageRelationKind::RelatedTo,
    }
}

fn license_kind_name(kind: LicenseKind) -> &'static str {
    match kind {
        LicenseKind::PublicDomain => "public_domain",
        LicenseKind::Cc0 => "cc0",
        LicenseKind::CcBy => "cc_by",
        LicenseKind::CcBySa => "cc_by_sa",
        LicenseKind::CcByNc => "cc_by_nc",
        LicenseKind::Custom => "custom",
        LicenseKind::Unknown => "unknown",
    }
}

fn license_kind_from_name(name: &str) -> LicenseKind {
    match name {
        "public_domain" => LicenseKind::PublicDomain,
        "cc0" => LicenseKind::Cc0,
        "cc_by" => LicenseKind::CcBy,
        "cc_by_sa" => LicenseKind::CcBySa,
        "cc_by_nc" => LicenseKind::CcByNc,
        "custom" => LicenseKind::Custom,
        _ => LicenseKind::Unknown,
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x3040..=0x30FF | 0x31F0..=0x31FF)
}

/// 一次词典命中的结果：词条 id、首个释义、读音。
struct DictionaryHit {
    item_id: String,
    meaning: Option<String>,
    reading: Option<String>,
}

/// 句子切分用的标点判定（中英文标点 + 常见全角符号）。
fn is_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation()
        || matches!(
            ch,
            '。' | '、'
                | '「'
                | '」'
                | '『'
                | '』'
                | '・'
                | '…'
                | '，'
                | '．'
                | '？'
                | '！'
                | '：'
                | '；'
        )
}

/// 构建 FTS5 前缀查询（把查询按空白切 token，每个加前缀 `*`）。
fn build_fts_query(roman: &str) -> String {
    let tokens: Vec<String> = roman
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"*", escape_fts_token(token)))
        .collect();
    if tokens.is_empty() {
        return "\"\"".to_string();
    }
    tokens.join(" AND ")
}

fn escape_fts_token(token: &str) -> String {
    token.replace('"', "\"\"")
}

/// 合并搜索键：text + reading + romanization + 元数据朗读项，全部小写化。
fn build_search_key(item: &ImportedItem) -> String {
    let mut parts = vec![item.text.clone()];
    if let Some(reading) = item.reading.as_ref() {
        parts.push(reading.clone());
    }
    if let Some(romanization) = item.romanization.as_ref() {
        parts.push(romanization.clone());
        let normalized = normalize_roman(romanization);
        if normalized != *romanization {
            parts.push(normalized);
        }
    }
    if let Some(meta) = item.meta.as_ref() {
        let extra = match meta {
            LanguageMetadata::English(data) => data.arpabet.clone().unwrap_or_default(),
            LanguageMetadata::Japanese(data) => {
                vec![data.kana.clone(), data.romaji.clone(), data.kanji.clone()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            LanguageMetadata::Mandarin(data) => vec![
                data.pinyin.clone(),
                data.simplified.clone(),
                data.traditional.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
            LanguageMetadata::Cantonese(data) => vec![
                data.jyutping.clone(),
                data.simplified.clone(),
                data.traditional.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
        };
        if !extra.is_empty() {
            parts.push(extra);
        }
    }
    parts.join(" ").to_lowercase()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn fixture_store() -> (tempfile::TempDir, LanguageStore) {
        let directory = tempdir().expect("tempdir");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        (directory, store)
    }

    fn fixture_store_mut() -> (tempfile::TempDir, LanguageStore) {
        let directory = tempdir().expect("tempdir");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        (directory, store)
    }

    // 从共享 fixture 走真实管道做集成断言的具体用例见 application 层；
    // 这里验证 schema 创建与学习内容（Lesson / 错题 / 句子拆解 / 迁移）的持久化语义。
    #[test]
    fn open_creates_schema() {
        let (_dir, store) = fixture_store();
        assert!(store.total_items().expect("count") == 0);
        let sources = store.sources().expect("sources");
        assert!(sources.is_empty());
    }

    #[test]
    fn next_new_items_filters_by_language_and_excludes_learned() {
        let (_dir, mut store) = fixture_store_mut();
        seed_item(&mut store, "jmdict:1", "駅", "WORD");
        seed_item(&mut store, "jmdict:2", "電車", "WORD");
        {
            store
                .import_items(
                    &[ImportedItem::new(
                        "wn:reservation".to_string(),
                        LanguageCode::Eng,
                        LanguageItemType::Word,
                        "reservation".to_string(),
                    )],
                    "oewn",
                    crate::now_unix(),
                )
                .expect("import english");
        }

        // 按语言过滤：不能把英语词混进日语队列
        let jp = store
            .next_new_items(LanguageCode::Jap, &[], 10)
            .expect("new items");
        assert_eq!(jp.len(), 2, "日语应有两词，实际 {:?}", jp.len());
        assert!(
            jp.iter().all(|item| item.language == LanguageCode::Jap),
            "队列里出现了别的语言"
        );

        // 排除已学
        let jp_after = store
            .next_new_items(LanguageCode::Jap, &["jmdict:1".to_string()], 10)
            .expect("new items");
        assert_eq!(
            jp_after.len(),
            1,
            "排除后应只剩一个词，实际 {:?}",
            jp_after.len()
        );
        assert_eq!(jp_after[0].id, "jmdict:2");

        // 英语队列独立
        let en = store
            .next_new_items(LanguageCode::Eng, &[], 10)
            .expect("new items");
        assert_eq!(en.len(), 1);
        assert_eq!(en[0].language, LanguageCode::Eng);

        // limit 生效
        assert_eq!(
            store
                .next_new_items(LanguageCode::Jap, &[], 1)
                .expect("new items")
                .len(),
            1
        );
        assert!(
            store
                .next_new_items(LanguageCode::Jap, &[], 0)
                .expect("new items")
                .is_empty()
        );
    }

    #[test]
    fn next_new_items_is_stable_across_calls() {
        let (_dir, mut store) = fixture_store_mut();
        seed_item(&mut store, "jmdict:1", "駅", "WORD");
        seed_item(&mut store, "jmdict:2", "電車", "WORD");
        seed_item(&mut store, "jmdict:3", "切符", "WORD");

        let first: Vec<String> = store
            .next_new_items(LanguageCode::Jap, &[], 2)
            .expect("new items")
            .into_iter()
            .map(|item| item.id)
            .collect();
        let second: Vec<String> = store
            .next_new_items(LanguageCode::Jap, &[], 2)
            .expect("new items")
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(
            first, second,
            "同一批数据每次应给同一批词，否则刷新一次换一批"
        );
    }

    fn seed_item(store: &mut LanguageStore, id: &str, text: &str, item_type: &str) {
        let mut item = ImportedItem::new(
            id.to_string(),
            LanguageCode::Jap,
            item_type_from_name(item_type),
            text.to_string(),
        );
        item.reading = Some("エキ".to_string());
        store
            .import_items(&[item], "jmdict", crate::now_unix())
            .expect("import");
    }

    #[test]
    fn lesson_roundtrip_persists_steps_and_position_across_reopen() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("language.db");
        let now = crate::now_unix();

        {
            let mut store = LanguageStore::open(&path).expect("open");
            seed_item(&mut store, "jmdict:1", "駅", "WORD");
            seed_item(&mut store, "jmdict:2", "電車", "WORD");

            let lesson = Lesson {
                id: "lesson-travel".into(),
                title: "日本 · 交通基础".into(),
                language: LanguageCode::Jap,
                description: Some("出行必需词".into()),
                steps: vec![
                    LessonStep {
                        item_id: "jmdict:1".into(),
                        item_type: LearningItemType::Word,
                        content: "駅".into(),
                        translation: None,
                    },
                    LessonStep {
                        item_id: "jmdict:2".into(),
                        item_type: LearningItemType::Word,
                        content: "電車".into(),
                        translation: None,
                    },
                ],
                created_at: now,
                updated_at: now,
            };
            store.upsert_lesson(&lesson).expect("upsert lesson");
            store
                .save_lesson_position(&LessonPosition {
                    lesson_id: "lesson-travel".into(),
                    step_index: 1,
                    updated_at: now,
                })
                .expect("save position");
        }

        // 重开库（模拟重启 App）→ 步骤与位置都还在
        let reopened = LanguageStore::open(&path).expect("reopen");
        let loaded = reopened
            .lesson("lesson-travel")
            .expect("read lesson")
            .expect("lesson exists");
        assert_eq!(loaded.title, "日本 · 交通基础");
        assert_eq!(loaded.steps.len(), 2);
        assert_eq!(loaded.step_position("jmdict:2"), Some(1));

        let position = reopened
            .lesson_position("lesson-travel")
            .expect("read position")
            .expect("position exists");
        assert_eq!(position.step_index, 1, "学习位置应跨重启保留");
    }

    #[test]
    fn repeated_mistake_increments_error_count_without_duplicating() {
        let (_dir, mut store) = fixture_store_mut();
        seed_item(&mut store, "jmdict:1", "駅", "WORD");
        let now = crate::now_unix();
        let mistake = || Mistake {
            id: "card_x".into(),
            item_id: "jmdict:1".into(),
            item_type: LearningItemType::Word,
            language: LanguageCode::Jap,
            content: "駅".into(),
            question: "駅".into(),
            user_answer: "火车".into(),
            correct_answer: "车站".into(),
            error_count: 1,
            last_missed_at: now,
        };

        store.record_mistake(&mistake(), "card_x").expect("record");
        store
            .record_mistake(&mistake(), "card_x")
            .expect("record again");

        let all = store.mistakes(10).expect("mistakes");
        assert_eq!(all.len(), 1, "同一条错误不应重复插入");
        assert_eq!(all[0].error_count, 2, "重复答错应累加");
        assert_eq!(store.mistake_count().expect("count"), 1);

        // 答对后移除
        store
            .resolve_mistake("jmdict:1", "card_x")
            .expect("resolve");
        assert!(store.mistakes(10).expect("mistakes").is_empty());
    }

    #[test]
    fn sentence_segments_against_real_dictionary_and_admits_misses() {
        let (_dir, mut store) = fixture_store_mut();
        seed_item(&mut store, "jmdict:1", "駅", "WORD");

        let mut sentence = ImportedItem::new(
            "tatoeba:4812".to_string(),
            LanguageCode::Jap,
            LanguageItemType::Sentence,
            "駅に行きます。".to_string(),
        );
        sentence.reading = Some("エキにいきます。".to_string());
        store
            .import_items(&[sentence], "tatoeba", crate::now_unix())
            .expect("import sentence");

        let study = store
            .sentence_study("tatoeba:4812")
            .expect("study")
            .expect("exists");
        assert_eq!(study.original, "駅に行きます。");
        assert!(study.has_breakdown());

        // 「駅」在词典里 → 应带 item_id
        let station = study
            .chunks
            .iter()
            .find(|chunk| chunk.text == "駅")
            .expect("駅 chunk");
        assert_eq!(station.item_id.as_deref(), Some("jmdict:1"));
        assert!(study.key_words.contains(&"jmdict:1".to_string()));

        // 词典未收录的片段不得凭空产生释义
        assert!(
            study
                .chunks
                .iter()
                .filter(|chunk| chunk.item_id.is_none() && chunk.meaning.is_some())
                .count()
                == 0,
            "未收录片段不应有释义"
        );
    }

    #[test]
    fn legacy_learning_tables_are_migrated_away_without_db_reset() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("language.db");

        // 造一个「旧版本」库：带被平台取代的重复学习表与其中的真实用户数据。
        {
            let legacy = LanguageStore::open(&path).expect("open");
            legacy
                .connection_for_tests()
                .execute_batch(
                    "CREATE TABLE learning_states (item_id TEXT PRIMARY KEY, state TEXT NOT NULL, interval_days REAL NOT NULL DEFAULT 0, ease REAL NOT NULL DEFAULT 2.5, due_at INTEGER NOT NULL, review_count INTEGER NOT NULL DEFAULT 0, lapses INTEGER NOT NULL DEFAULT 0, started_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
                     CREATE TABLE review_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, item_id TEXT NOT NULL, reviewed_at INTEGER NOT NULL, rating TEXT NOT NULL, state_before TEXT NOT NULL, state_after TEXT NOT NULL, interval_days REAL NOT NULL);
                     CREATE TABLE favorites (item_id TEXT PRIMARY KEY, created_at INTEGER NOT NULL);
                     CREATE TABLE learning_sessions (id INTEGER PRIMARY KEY AUTOINCREMENT, day TEXT NOT NULL, started_at INTEGER NOT NULL, ended_at INTEGER, new_count INTEGER NOT NULL DEFAULT 0, review_count INTEGER NOT NULL DEFAULT 0, sentences INTEGER NOT NULL DEFAULT 0);
                     INSERT INTO learning_states VALUES ('jmdict:1','learning',2.5,2.5,1700000000,3,0,1,1);
                     INSERT INTO favorites VALUES ('jmdict:1', 1700000000);",
                )
                .expect("create legacy tables");
        }

        // 再次 open 会跑迁移：旧表被删除，无需重置数据库。
        let store = LanguageStore::open(&path).expect("reopen after migration");
        let remaining: i64 = store
            .connection_for_tests()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('learning_states','review_logs','favorites','learning_sessions')",
                [],
                |row| row.get(0),
            )
            .expect("count legacy tables");
        assert_eq!(remaining, 0, "重复学习表应被迁移删除");

        // 词典侧与新表不受影响，且迁移可重复执行（再次 open 不报错）。
        assert!(store.total_items().expect("count") == 0);
        assert!(
            store
                .lessons(Some(LanguageCode::Jap), 10)
                .expect("lessons")
                .is_empty()
        );
        drop(store);
        LanguageStore::open(&path).expect("迁移幂等：二次 open 不应失败");
    }
}
