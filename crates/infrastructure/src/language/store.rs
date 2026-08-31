//! Language Learning Hub 的 SQLite 存储（`language.db`）。
//!
//! 与 RSS/Travel/History 同款 rusqlite 方案：`Connection` 非 `Sync`，由上层 `Mutex` 串行。
//! 词典表与用户学习表严格分离（#59）：`import_items` 只动词典表，绝不触碰用户进度。
//! 搜索使用 **FTS5**（bundled SQLite 自带，已核对 libsqlite3-sys build flags）。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use devtoolbox_core::language::{
    LanguageCode, LanguageCount, LanguageItem, LanguageItemType, LanguageMetadata,
    LanguageRelation, LanguageRelationKind, LanguageSource, LearningState, LearningStateKind,
    LicenseKind, Meaning, Pronunciation, PronunciationScheme, ReviewOutcome, ReviewRating,
    ReviewScheduler, SentenceRecord, SourceLicense, TodayPlan, normalize_roman,
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
    pub state: Option<LearningState>,
    pub favorite: bool,
    /// item_extra JSON（kanji 元数据等）。
    pub extra: Option<serde_json::Value>,
}

pub struct LanguageStore {
    connection: Connection,
}

fn sqlite(error: rusqlite::Error) -> InfrastructureError {
    InfrastructureError::Sqlite(error.to_string())
}

impl LanguageStore {
    /// 打开（必要时创建）语言数据库并确保 Schema 存在。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InfrastructureError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| crate::error::io_error(parent, source))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| InfrastructureError::Sqlite(error.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite)?;
        let store = Self { connection };
        store.ensure_schema()?;
        store.seed_languages()?;
        Ok(store)
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
            -- 用户学习数据（与词典表隔离，#59）
            CREATE TABLE IF NOT EXISTS learning_states (
                item_id TEXT PRIMARY KEY, state TEXT NOT NULL, interval_days REAL NOT NULL DEFAULT 0,
                ease REAL NOT NULL DEFAULT 2.5, due_at INTEGER NOT NULL,
                review_count INTEGER NOT NULL DEFAULT 0, lapses INTEGER NOT NULL DEFAULT 0,
                started_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_learning_due ON learning_states(due_at);
            CREATE TABLE IF NOT EXISTS review_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT, item_id TEXT NOT NULL,
                reviewed_at INTEGER NOT NULL, rating TEXT NOT NULL, state_before TEXT NOT NULL,
                state_after TEXT NOT NULL, interval_days REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_review_logs_item ON review_logs(item_id, reviewed_at);
            CREATE TABLE IF NOT EXISTS favorites (
                item_id TEXT PRIMARY KEY, created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS learning_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT, day TEXT NOT NULL,
                started_at INTEGER NOT NULL, ended_at INTEGER,
                new_count INTEGER NOT NULL DEFAULT 0, review_count INTEGER NOT NULL DEFAULT 0,
                sentences INTEGER NOT NULL DEFAULT 0
            );
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
        ).map_err(sqlite)
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
        let state = self.learning_state(id)?;
        let favorite = self.is_favorite(id)?;
        let extra = self.item_extra(id)?;
        Ok(ItemDetailRows {
            item,
            meanings,
            pronunciations,
            relations,
            related_items,
            examples,
            sentences,
            state,
            favorite,
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

    // ---------------- 用户学习数据 ----------------

    pub fn learning_state(
        &self,
        item_id: &str,
    ) -> Result<Option<LearningState>, InfrastructureError> {
        self.connection
            .query_row(
                "SELECT item_id, state, interval_days, ease, due_at, review_count, lapses, started_at, updated_at
                 FROM learning_states WHERE item_id = ?1",
                [item_id],
                map_state,
            )
            .optional()
            .map_err(sqlite)
    }

    pub fn upsert_state(&self, state: &LearningState) -> Result<(), InfrastructureError> {
        let state_name = state_kind_name(state.state);
        self.connection
            .execute(
                "INSERT OR REPLACE INTO learning_states
                    (item_id, state, interval_days, ease, due_at, review_count, lapses, started_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    state.item_id, state_name, state.interval_days, state.ease, state.due_at,
                    state.review_count, state.lapses, state.started_at, state.updated_at
                ],
            )
            .map_err(sqlite)?;
        Ok(())
    }

    /// 复习一次：更新状态 + 写日志。
    pub fn rate_review(
        &mut self,
        item_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewOutcome, InfrastructureError> {
        let transaction = self.connection.transaction().map_err(sqlite)?;
        let current = transaction
            .query_row(
                "SELECT item_id, state, interval_days, ease, due_at, review_count, lapses, started_at, updated_at
                 FROM learning_states WHERE item_id = ?1",
                [item_id],
                map_state,
            )
            .optional()
            .map_err(sqlite)?
            .unwrap_or_else(|| LearningState::new(item_id, now));
        let outcome = ReviewScheduler::schedule(&current, rating, now);
        let updated = LearningState {
            item_id: item_id.to_string(),
            state: outcome.state,
            interval_days: outcome.interval_days,
            ease: outcome.ease,
            due_at: outcome.due_at,
            review_count: current.review_count + 1,
            lapses: outcome.lapses,
            started_at: current.started_at,
            updated_at: now,
        };
        transaction
            .execute(
                "INSERT OR REPLACE INTO learning_states
                    (item_id, state, interval_days, ease, due_at, review_count, lapses, started_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    updated.item_id, state_kind_name(updated.state), updated.interval_days,
                    updated.ease, updated.due_at, updated.review_count, updated.lapses,
                    updated.started_at, updated.updated_at
                ],
            )
            .map_err(sqlite)?;
        transaction
            .execute(
                "INSERT INTO review_logs (item_id, reviewed_at, rating, state_before, state_after, interval_days)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    item_id, now, rating_name(rating), state_kind_name(current.state),
                    state_kind_name(updated.state), updated.interval_days
                ],
            )
            .map_err(sqlite)?;
        transaction.commit().map_err(sqlite)?;
        Ok(outcome)
    }

    /// 批量标记学习状态（Library / Word Detail 手动设置）。
    pub fn set_learning_state(
        &self,
        item_id: &str,
        state: LearningStateKind,
        now: i64,
    ) -> Result<(), InfrastructureError> {
        let current = self
            .learning_state(item_id)?
            .unwrap_or_else(|| LearningState::new(item_id, now));
        let updated = LearningState {
            state,
            updated_at: now,
            ..current
        };
        self.upsert_state(&updated)
    }

    pub fn is_favorite(&self, item_id: &str) -> Result<bool, InfrastructureError> {
        let exists = self
            .connection
            .query_row(
                "SELECT 1 FROM favorites WHERE item_id = ?1",
                [item_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(sqlite)?;
        Ok(exists.is_some())
    }

    pub fn toggle_favorite(&self, item_id: &str, now: i64) -> Result<bool, InfrastructureError> {
        let exists = self.is_favorite(item_id)?;
        if exists {
            self.connection
                .execute("DELETE FROM favorites WHERE item_id = ?1", [item_id])
                .map_err(sqlite)?;
            Ok(false)
        } else {
            self.connection
                .execute(
                    "INSERT OR IGNORE INTO favorites (item_id, created_at) VALUES (?1, ?2)",
                    params![item_id, now],
                )
                .map_err(sqlite)?;
            Ok(true)
        }
    }

    pub fn favorites(&self, limit: usize) -> Result<Vec<LanguageItem>, InfrastructureError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT i.id, i.language, i.item_type, i.text, i.reading, i.romanization, i.meta_json, i.source
                 FROM favorites f JOIN language_items i ON i.id = f.item_id
                 ORDER BY f.created_at DESC LIMIT ?1",
            )
            .map_err(sqlite)?;
        let rows = statement
            .query_map([limit as i64], map_item)
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)?;
        Ok(rows)
    }

    /// 下一条复习卡片：先到期，再新词（本语言）。
    pub fn review_next(
        &self,
        language: LanguageCode,
        now: i64,
    ) -> Result<Option<LanguageItem>, InfrastructureError> {
        let code = language.code();
        let due = self
            .connection
            .query_row(
                "SELECT i.id, i.language, i.item_type, i.text, i.reading, i.romanization, i.meta_json, i.source
                 FROM learning_states s JOIN language_items i ON i.id = s.item_id
                 WHERE i.language = ?1 AND s.due_at <= ?2
                 ORDER BY s.due_at LIMIT 1",
                params![code, now],
                map_item,
            )
            .optional()
            .map_err(sqlite)?;
        if let Some(item) = due {
            return Ok(Some(item));
        }
        // 新词：该语言尚未开始学习的词
        let fresh = self
            .connection
            .query_row(
                "SELECT i.id, i.language, i.item_type, i.text, i.reading, i.romanization, i.meta_json, i.source
                 FROM language_items i
                 WHERE i.language = ?1 AND i.item_type = 'WORD'
                   AND NOT EXISTS (SELECT 1 FROM learning_states s WHERE s.item_id = i.id)
                 ORDER BY i.imported_at, i.id LIMIT 1",
                [code],
                map_item,
            )
            .optional()
            .map_err(sqlite)?;
        Ok(fresh)
    }

    /// Today 计划（#61）。
    pub fn today_plan(
        &self,
        language: LanguageCode,
        now: i64,
    ) -> Result<TodayPlan, InfrastructureError> {
        let code = language.code();
        let due_reviews: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM learning_states s JOIN language_items i ON i.id = s.item_id
                 WHERE i.language = ?1 AND s.due_at <= ?2",
                params![code, now],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        let new_words: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM language_items i
                 WHERE i.language = ?1 AND i.item_type = 'WORD' AND i.source <> 'cmudict'
                   AND NOT EXISTS (SELECT 1 FROM learning_states s WHERE s.item_id = i.id)",
                [code],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        let sentences: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM language_items WHERE language = ?1 AND item_type = 'SENTENCE'",
                [code],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        Ok(TodayPlan {
            due_reviews,
            new_words,
            sentences,
            listening: sentences.min(5),
            speaking: sentences.min(3),
            total: due_reviews + new_words.min(10) + sentences.min(5),
        })
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

    /// 用户学习概览（Library）。
    pub fn progress(&self) -> Result<serde_json::Value, InfrastructureError> {
        let total: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM learning_states WHERE review_count > 0",
                [],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        let mastered: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM learning_states WHERE state = 'mastered'",
                [],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        let learning: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM learning_states WHERE state = 'learning' OR state = 'review'",
                [],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        let total_reviews: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM review_logs", [], |row| row.get(0))
            .map_err(sqlite)?;
        Ok(serde_json::json!({
            "total": total,
            "mastered": mastered,
            "learning": learning,
            "reviews": total_reviews,
        }))
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

    pub fn favorites_count(&self) -> Result<i64, InfrastructureError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM favorites", [], |row| row.get(0))
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

fn map_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningState> {
    Ok(LearningState {
        item_id: row.get(0)?,
        state: state_kind_from_name(&row.get::<_, String>(1)?),
        interval_days: row.get(2)?,
        ease: row.get(3)?,
        due_at: row.get(4)?,
        review_count: row.get(5)?,
        lapses: row.get(6)?,
        started_at: row.get(7)?,
        updated_at: row.get(8)?,
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

fn state_kind_name(kind: LearningStateKind) -> &'static str {
    match kind {
        LearningStateKind::New => "new",
        LearningStateKind::Learning => "learning",
        LearningStateKind::Review => "review",
        LearningStateKind::Mastered => "mastered",
    }
}

fn state_kind_from_name(name: &str) -> LearningStateKind {
    match name {
        "learning" => LearningStateKind::Learning,
        "review" => LearningStateKind::Review,
        "mastered" => LearningStateKind::Mastered,
        _ => LearningStateKind::New,
    }
}

fn rating_name(rating: ReviewRating) -> &'static str {
    match rating {
        ReviewRating::Again => "again",
        ReviewRating::Hard => "hard",
        ReviewRating::Good => "good",
        ReviewRating::Easy => "easy",
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x3040..=0x30FF | 0x31F0..=0x31FF)
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
    // 这里验证 schema 创建与用户学习数据的隔离语义。
    #[test]
    fn open_creates_schema() {
        let (_dir, store) = fixture_store();
        assert!(store.total_items().expect("count") == 0);
        let sources = store.sources().expect("sources");
        assert!(sources.is_empty());
    }

    #[test]
    fn learning_tables_isolated_from_items() {
        let (_dir, mut store) = fixture_store_mut();
        let now = crate::now_unix();
        store
            .upsert_state(&LearningState::new("x", now))
            .expect("state");
        assert!(store.learning_state("x").expect("read").is_some());
        // 词典表操作不影响学习表
        let items: Vec<ImportedItem> = Vec::new();
        store.import_items(&items, "s", now).expect("import empty");
        assert!(store.learning_state("x").expect("read").is_some());
    }

    #[test]
    fn review_roundtrip() {
        let (_dir, mut store) = fixture_store_mut();
        let now = crate::now_unix();
        let outcome = store
            .rate_review("en:wn:reservation", ReviewRating::Good, now)
            .expect("rate");
        assert!(outcome.interval_days >= 1.0);
        assert_eq!(outcome.state, LearningStateKind::Review);
    }

    #[test]
    fn favorites_toggle_roundtrip() {
        let (_dir, store) = fixture_store();
        let now = crate::now_unix();
        assert!(!store.is_favorite("a").expect("not favorite"));
        assert!(store.toggle_favorite("a", now).expect("add"));
        assert!(store.is_favorite("a").expect("favorite"));
        assert!(!store.toggle_favorite("a", now).expect("remove"));
    }
}
