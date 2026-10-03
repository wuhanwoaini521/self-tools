//! ECDICT CSV 导入器（`ecdict.csv` / `stardict.csv`，约 3.4M 词条）。
//!
//! CSV 列（真实表头）：
//! `word,phonetic,definition,translation,pos,collins,oxford,tag,bnc,frq,exchange,detail,audio`
//!
//! 原则：
//! - 流式读取 + 分批事务（默认 2 万行/批），3.4M 行不会撑爆内存；
//! - 全量重导入 = 清空后重建（词典无用户状态，安全）；
//! - 可取消（批与批之间检查 flag），进度按批回调；
//! - 跳过无内容行（word 为空或 音标/释义/翻译 全空）。

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use devtoolbox_core::language::WordEntry;

use super::store::LanguageStore;

const BATCH_SIZE: usize = 20_000;

/// 导入进度回调形状。
#[derive(Clone, Debug, Serialize)]
pub struct DictImportProgress {
    pub done: usize,
    pub message: String,
}

/// 导入报告。
#[derive(Clone, Debug, Default, Serialize)]
pub struct DictImportReport {
    pub entries: usize,
    pub skipped: usize,
    pub cancelled: bool,
}

fn parse_u32(raw: &str) -> u32 {
    raw.trim().parse::<i64>().unwrap_or(0).max(0) as u32
}

/// ECDICT 的释义字段用**字面量** `\n` 分隔多条释义（不是真换行）。
///
/// 前端按换行拆分取第一条中文释义，不归一化就会把 `n. 长袜\n[医] …` 整段当一行显示。
fn unescape_newlines(raw: &str) -> String {
    raw.replace("\\r\\n", "\n").replace("\\n", "\n")
}

/// 解析 ECDICT `exchange` 列为 `(kind, form)` 列表。
fn parse_exchange_forms(raw: &str) -> Vec<(String, String)> {
    raw.split('/')
        .filter_map(|part| {
            let (kind, form) = part.split_once(':')?;
            if form.is_empty() {
                return None;
            }
            Some((kind.to_string(), form.to_string()))
        })
        .collect()
}

/// 从 CSV 文件导入词典。
pub fn import_ecdict_csv(
    store: &LanguageStore,
    path: &Path,
    cancel: &Arc<AtomicBool>,
    on_progress: &dyn Fn(DictImportProgress),
) -> Result<DictImportReport, String> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;

    // 校验表头（防止用户选错文件时静默导入垃圾）。
    {
        let headers = reader
            .headers()
            .map_err(|error| format!("read headers: {error}"))?;
        let first = headers.get(0).unwrap_or("");
        if !first.eq_ignore_ascii_case("word") {
            return Err(format!(
                "not an ECDICT csv: expected header starting with 'word', got '{first}'"
            ));
        }
    }

    let mut report = DictImportReport::default();
    let mut batch: Vec<WordEntry> = Vec::with_capacity(BATCH_SIZE);
    let mut first_batch = true;

    let flush = |store: &LanguageStore,
                 batch: &mut Vec<WordEntry>,
                 first_batch: &mut bool,
                 report: &DictImportReport|
     -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let tx = store
            .conn()
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        if *first_batch {
            tx.execute("DELETE FROM dict_entries", [])
                .map_err(|error| error.to_string())?;
            *first_batch = false;
        }
        LanguageStore::insert_dict_batch(&tx, batch).map_err(|error| error.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        on_progress(DictImportProgress {
            done: report.entries,
            message: format!("{} entries", report.entries),
        });
        batch.clear();
        Ok(())
    };

    for record in reader.records() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let record = match record {
            Ok(record) => record,
            Err(_) => {
                report.skipped += 1;
                continue;
            }
        };
        let word = record.get(0).unwrap_or("").trim();
        let phonetic = record.get(1).unwrap_or("").trim();
        let definition = record.get(2).unwrap_or("").trim();
        let translation = record.get(3).unwrap_or("").trim();
        if word.is_empty()
            || (phonetic.is_empty() && definition.is_empty() && translation.is_empty())
        {
            report.skipped += 1;
            continue;
        }
        batch.push(WordEntry {
            word: word.to_string(),
            lemma: String::new(), // 查询时按 exchange 还原；导入不重复存储
            phonetic: (!phonetic.is_empty()).then(|| phonetic.to_string()),
            pos: {
                let pos = record.get(4).unwrap_or("").trim();
                (!pos.is_empty()).then(|| pos.to_string())
            },
            translation_zh: (!translation.is_empty()).then(|| unescape_newlines(translation)),
            definition_en: (!definition.is_empty()).then(|| unescape_newlines(definition)),
            frequency: parse_u32(record.get(9).unwrap_or("")),
            bnc: parse_u32(record.get(8).unwrap_or("")),
            tags: record
                .get(7)
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            collins: parse_u32(record.get(5).unwrap_or("")),
            forms: parse_exchange_forms(record.get(10).unwrap_or("")),
        });
        report.entries += 1;
        if batch.len() >= BATCH_SIZE {
            flush(store, &mut batch, &mut first_batch, &report)?;
        }
    }
    flush(store, &mut batch, &mut first_batch, &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_real_ecdict_shape_and_skips_empty_rows() {
        let directory = tempfile::tempdir().expect("tempdir");
        let csv_path = directory.path().join("ecdict.csv");
        std::fs::write(
            &csv_path,
            "word,phonetic,definition,translation,pos,collins,oxford,tag,bnc,frq,exchange,detail,audio\n\
             hesitate,\"hɪˈzɪteɪt\",\"\",\"v. 犹豫；迟疑；踌躇\",v.,3,0,\"cet4 cet6 ky\",5100,8064,\"p:hesitated/d:hesitated/i:hesitating/3:hesitates\",,\n\
             conversation,\"ˌkɒnvəˈseɪʃn\",\"n. talk\",\"n. 交谈；谈话；会话\",n.,4,1,\"cet4 cet6\",2600,2900,\"s:conversations\",,\n\
             emptyrow,,,,,,,,0,0,,,\n",
        )
        .expect("write csv");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        let cancel = Arc::new(AtomicBool::new(false));
        let report = import_ecdict_csv(&store, &csv_path, &cancel, &|_| {}).expect("import");
        assert_eq!(report.entries, 2);
        assert_eq!(report.skipped, 1);
        assert_eq!(store.dict_count().expect("count"), 2);

        let entry = store
            .dict_lookup("HESITATE")
            .expect("lookup")
            .expect("found");
        assert_eq!(entry.translation_zh.as_deref(), Some("v. 犹豫；迟疑；踌躇"));
        assert_eq!(
            entry.tags,
            vec!["cet4".to_string(), "cet6".to_string(), "ky".to_string()]
        );
        assert_eq!(entry.collins, 3);
        assert_eq!(entry.frequency, 8064);
    }

    #[test]
    fn unescapes_literal_newlines_in_definitions() {
        assert_eq!(
            unescape_newlines("n. 长袜\\n[医] 马足水肿"),
            "n. 长袜\n[医] 马足水肿"
        );
        assert_eq!(unescape_newlines("plain"), "plain");
    }

    #[test]
    fn rejects_wrong_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let csv_path = directory.path().join("random.csv");
        std::fs::write(&csv_path, "name,age\njohn,30\n").expect("write");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        let cancel = Arc::new(AtomicBool::new(false));
        let error = import_ecdict_csv(&store, &csv_path, &cancel, &|_| {}).unwrap_err();
        assert!(error.contains("not an ECDICT csv"));
    }

    #[test]
    fn cancel_stops_import() {
        let directory = tempfile::tempdir().expect("tempdir");
        let csv_path = directory.path().join("ecdict.csv");
        std::fs::write(
            &csv_path,
            "word,phonetic,definition,translation,pos,collins,oxford,tag,bnc,frq,exchange,detail,audio\n\
             apple,ˈæpl,,n. 苹果,n.,5,1,,100,200,,\n",
        )
        .expect("write");
        let store = LanguageStore::open(directory.path().join("language.db")).expect("open");
        let cancel = Arc::new(AtomicBool::new(true));
        let report = import_ecdict_csv(&store, &csv_path, &cancel, &|_| {}).expect("import");
        assert!(report.cancelled);
        assert_eq!(report.entries, 0);
    }
}
