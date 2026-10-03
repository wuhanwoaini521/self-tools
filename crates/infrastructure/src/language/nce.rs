//! NCE（New Concept English 1–4）本地文件夹导入器。
//!
//! 支持的真实数据布局（见 `docs/language/NCE_IMPORT.md`）：
//!
//! ```text
//! 根目录
//! ├── NCE1/ 001&002－Excuse Me.lrc + .mp3            （NCE-Flow 风格：裸文件）
//! ├── NCE2/ book.json + <filename>.lrc + .mp3        （iChochy 风格：book.json 清单）
//! ├── data.json  {"1": [{"title","filename"},…]}     （NCE-Flow 风格：根清单）
//! └── static/data.json                               （NCE-Flow 原仓库位置）
//! ```
//!
//! 原则：
//! - **幂等**：id 内容派生（`nce:{book}:{lesson}`），重复导入 = 覆盖内容、保留进度；
//! - **容错**：缺 MP3 / LRC 畸形 / 编码问题逐课记录到 `issues`，不中断整册导入；
//! - **可取消**：每课之间检查 cancel flag；
//! - **离线**：MP3 拷贝进用户数据目录（`media_dir`），导入后不依赖源文件夹。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use regex::Regex;
use serde::{Deserialize, Serialize};

use devtoolbox_core::language::{
    Course, CourseBook, CourseLesson, LanguageCode, LessonSentence, parse_lrc,
};

use super::store::LanguageStore;
use super::vocab::extract_lesson_vocab;

// ============================================================================
// 报告类型
// ============================================================================

/// 扫描到的一课（导入前的预览）。
#[derive(Clone, Debug, Serialize)]
pub struct NceLessonScan {
    pub lesson_no: u32,
    pub title: String,
    pub has_lrc: bool,
    pub has_audio: bool,
}

/// 扫描到的一册书。
#[derive(Clone, Debug, Serialize)]
pub struct NceBookScan {
    pub book_no: u32,
    pub folder: String,
    pub lessons: Vec<NceLessonScan>,
}

/// 扫描报告（导入前给用户确认）。
#[derive(Clone, Debug, Serialize)]
pub struct NceScanReport {
    pub books: Vec<NceBookScan>,
    pub total_lessons: usize,
    pub issues: Vec<String>,
}

/// 导入进度事件（前端进度条用）。
#[derive(Clone, Debug, Serialize)]
pub struct NceImportProgress {
    pub stage: String,
    pub book_no: u32,
    pub lesson_no: u32,
    pub done: usize,
    pub total: usize,
    pub message: String,
}

/// 导入完成报告。
#[derive(Clone, Debug, Default, Serialize)]
pub struct NceImportReport {
    pub books: usize,
    pub lessons: usize,
    pub sentences: usize,
    pub vocab: usize,
    pub media_files: usize,
    pub media_bytes: u64,
    pub skipped: usize,
    pub cancelled: bool,
    pub issues: Vec<String>,
}

// ============================================================================
// 扫描
// ============================================================================

/// book.json（iChochy）单元。
#[derive(Deserialize)]
struct BookJsonUnit {
    title: Option<String>,
    filename: Option<String>,
}

#[derive(Deserialize)]
struct BookJson {
    units: Option<Vec<BookJsonUnit>>,
}

fn read_text_lossy(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if let Ok(text) = String::from_utf8(bytes.clone()) {
        return Ok(text.trim_start_matches('\u{feff}').to_string());
    }
    // 中文 Windows 工具生成的 LRC 常见 GBK 编码。
    let (text, _, _) = encoding_rs::GBK.decode(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

/// 从文件名解析 `(lesson_no, title)`：
/// `001&002－Excuse Me` / `001&002.Excuse Me` / `017－Always Young` / `1 A private conversation`。
fn parse_lesson_filename(stem: &str) -> Option<(u32, String)> {
    let pattern = Regex::new(r"^(\d{1,3})(?:\s*[&＆]\s*\d{1,3})?\s*[－\-—.．:：]?\s*(.*)$").ok()?;
    let captures = pattern.captures(stem.trim())?;
    let lesson_no: u32 = captures.get(1)?.as_str().parse().ok()?;
    let title = captures
        .get(2)
        .map(|value| value.as_str().trim().trim_end_matches('.').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Lesson {lesson_no}"));
    Some((lesson_no, title))
}

/// 发现书册文件夹：`NCE1`..`NCE4`（大小写/空格宽容），或根目录本身即一册。
fn discover_book_folders(root: &Path, issues: &mut Vec<String>) -> Vec<(u32, PathBuf)> {
    let book_pattern = Regex::new(r"(?i)^nce[\s_-]*([1-4])").expect("regex");
    let mut books: Vec<(u32, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(captures) = book_pattern.captures(&name)
                && let Some(no) = captures
                    .get(1)
                    .and_then(|value| value.as_str().parse::<u32>().ok())
            {
                books.push((no, path));
            }
        }
    }
    books.sort_by_key(|(no, _)| *no);
    books.dedup_by_key(|(no, _)| *no);
    if books.is_empty() {
        // 根目录本身可能是一册（直接包含 .lrc）。
        let has_lrc = fs::read_dir(root)
            .map(|entries| {
                entries.flatten().any(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("lrc"))
                })
            })
            .unwrap_or(false);
        if has_lrc {
            let guessed = book_pattern
                .captures(&root.file_name().unwrap_or_default().to_string_lossy())
                .and_then(|captures| captures.get(1))
                .and_then(|value| value.as_str().parse::<u32>().ok())
                .unwrap_or(1);
            books.push((guessed, root.to_path_buf()));
        } else {
            issues.push(format!(
                "no NCE book folders found under {} (expected NCE1..NCE4 with .lrc files)",
                root.display()
            ));
        }
    }
    books
}

/// 根/静态目录的 data.json（NCE-Flow：`{"1": [{"title","filename"}]}`）。
fn load_root_manifest(root: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    for candidate in [
        root.join("data.json"),
        root.join("static").join("data.json"),
    ] {
        if let Ok(text) = read_text_lossy(&candidate)
            && let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(&text)
        {
            // iChochy 根 data.json 是 {"books": [...]}（索引），不是课时清单。
            if !map.contains_key("books") {
                return Some(map);
            }
        }
    }
    None
}

struct LessonSource {
    lesson_no: u32,
    title: String,
    lrc: Option<PathBuf>,
    mp3: Option<PathBuf>,
}

/// 列举一册书的课时来源（book.json 清单优先，其次根 data.json，最后裸扫 LRC）。
fn collect_lesson_sources(
    root: &Path,
    book_no: u32,
    folder: &Path,
    issues: &mut Vec<String>,
) -> Vec<LessonSource> {
    let find_media = |filename: &str| -> (Option<PathBuf>, Option<PathBuf>) {
        let lrc = folder.join(format!("{filename}.lrc"));
        let mp3 = folder.join(format!("{filename}.mp3"));
        (lrc.is_file().then_some(lrc), mp3.is_file().then_some(mp3))
    };

    // 1) book.json（iChochy）。
    let book_json = folder.join("book.json");
    if book_json.is_file() {
        match read_text_lossy(&book_json)
            .and_then(|text| serde_json::from_str::<BookJson>(&text).map_err(|e| e.to_string()))
        {
            Ok(manifest) => {
                let mut out = Vec::new();
                for (index, unit) in manifest.units.unwrap_or_default().into_iter().enumerate() {
                    let Some(filename) = unit.filename.filter(|value| !value.is_empty()) else {
                        continue;
                    };
                    let (lrc, mp3) = find_media(&filename);
                    let (lesson_no, title) = parse_lesson_filename(&filename)
                        .map(|(no, parsed)| {
                            // book.json 的 title 更干净时优先用它。
                            let title = unit
                                .title
                                .clone()
                                .map(|t| {
                                    // iChochy title 形如 `001&002.Excuse Me`，剥掉编号前缀。
                                    parse_lesson_filename(&t)
                                        .map(|(_, clean)| clean)
                                        .unwrap_or(t)
                                })
                                .unwrap_or(parsed);
                            (no, title)
                        })
                        .unwrap_or(((index + 1) as u32, filename.clone()));
                    out.push(LessonSource {
                        lesson_no,
                        title,
                        lrc,
                        mp3,
                    });
                }
                if !out.is_empty() {
                    out.sort_by_key(|source| source.lesson_no);
                    return out;
                }
                issues.push(format!(
                    "book.json in {} has no usable units",
                    folder.display()
                ));
            }
            Err(error) => issues.push(format!("book.json parse failed: {error}")),
        }
    }

    // 2) 根 data.json（NCE-Flow）。
    if let Some(manifest) = load_root_manifest(root)
        && let Some(serde_json::Value::Array(units)) = manifest.get(&book_no.to_string())
    {
        let mut out = Vec::new();
        for (index, unit) in units.iter().enumerate() {
            let filename = unit
                .get("filename")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if filename.is_empty() {
                continue;
            }
            let (lrc, mp3) = find_media(filename);
            let (lesson_no, title) = parse_lesson_filename(filename)
                .map(|(no, parsed)| {
                    let title = unit
                        .get("title")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .unwrap_or(parsed);
                    (no, title)
                })
                .unwrap_or(((index + 1) as u32, filename.to_string()));
            out.push(LessonSource {
                lesson_no,
                title,
                lrc,
                mp3,
            });
        }
        if !out.is_empty() {
            out.sort_by_key(|source| source.lesson_no);
            return out;
        }
    }

    // 3) 裸扫 LRC 文件。
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lrc"))
            {
                continue;
            }
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let Some((lesson_no, title)) = parse_lesson_filename(&stem) else {
                issues.push(format!("cannot parse lesson filename: {stem}"));
                continue;
            };
            let mp3 = folder.join(format!("{stem}.mp3"));
            out.push(LessonSource {
                lesson_no,
                title,
                lrc: Some(path),
                mp3: mp3.is_file().then_some(mp3),
            });
        }
    }
    out.sort_by_key(|source| source.lesson_no);
    out
}

/// 扫描 NCE 源文件夹（只读，不导入）。
pub fn scan_nce_source(root: &Path) -> NceScanReport {
    let mut issues = Vec::new();
    let books = discover_book_folders(root, &mut issues);
    let mut report = NceScanReport {
        books: Vec::new(),
        total_lessons: 0,
        issues: issues.clone(),
    };
    for (book_no, folder) in books {
        let sources = collect_lesson_sources(root, book_no, &folder, &mut report.issues);
        report.total_lessons += sources.len();
        report.books.push(NceBookScan {
            book_no,
            folder: folder.display().to_string(),
            lessons: sources
                .iter()
                .map(|source| NceLessonScan {
                    lesson_no: source.lesson_no,
                    title: source.title.clone(),
                    has_lrc: source.lrc.is_some(),
                    has_audio: source.mp3.is_some(),
                })
                .collect(),
        });
    }
    report
}

// ============================================================================
// 导入
// ============================================================================

/// 导入选项。
pub struct NceImport {
    /// 用户的 NCE 源文件夹。
    pub source_dir: PathBuf,
    /// 媒体落地目录（如 `<config>/language/nce`）。
    pub media_dir: PathBuf,
    /// 取消 flag（每课之间检查）。
    pub cancel: Arc<AtomicBool>,
    /// 进度回调（桌面层转 Tauri 事件）。
    pub on_progress: Box<dyn Fn(NceImportProgress) + Send>,
}

fn copy_media(source: &Path, media_dir: &Path, book_no: u32) -> Result<(PathBuf, u64), String> {
    let file_name = source
        .file_name()
        .ok_or_else(|| format!("bad media path: {}", source.display()))?;
    let target_dir = media_dir.join(format!("NCE{book_no}"));
    fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;
    let target = target_dir.join(file_name);
    let source_size = source.metadata().map_err(|error| error.to_string())?.len();
    // 幂等：同名同大小视为已拷贝。
    if target.is_file() && target.metadata().map(|meta| meta.len()).ok() == Some(source_size) {
        return Ok((target, 0));
    }
    fs::copy(source, &target).map_err(|error| format!("copy {}: {error}", source.display()))?;
    Ok((target, source_size))
}

/// 执行导入。
pub fn import_nce(store: &LanguageStore, options: &NceImport) -> Result<NceImportReport, String> {
    let mut report = NceImportReport::default();
    let now = crate::now_unix();

    store
        .upsert_course(&Course {
            id: "nce".into(),
            language: LanguageCode::Eng,
            code: "nce".into(),
            title: "New Concept English".into(),
            description: Some("新概念英语 1–4（本地导入）".into()),
            source_type: "nce".into(),
            created_at: now,
        })
        .map_err(|error| error.to_string())?;

    let books = discover_book_folders(&options.source_dir, &mut report.issues);
    if books.is_empty() {
        return Err(format!(
            "no NCE books found under {}",
            options.source_dir.display()
        ));
    }

    // 先收集全部课时，便于报告总进度。
    let mut all: Vec<(u32, PathBuf, Vec<LessonSource>)> = Vec::new();
    let mut total = 0usize;
    for (book_no, folder) in &books {
        let sources =
            collect_lesson_sources(&options.source_dir, *book_no, folder, &mut report.issues);
        total += sources.len();
        all.push((*book_no, folder.clone(), sources));
    }
    if total == 0 {
        return Err("no lessons found (missing .lrc files?)".to_string());
    }

    let mut done = 0usize;
    for (book_no, folder, sources) in all {
        store
            .upsert_book(&CourseBook {
                id: format!("nce:{book_no}"),
                course_id: "nce".into(),
                book_no,
                title: format!("New Concept English {book_no}"),
                subtitle: Some(format!("新概念英语 第{book_no}册")),
                total_lessons: sources.len() as u32,
            })
            .map_err(|error| error.to_string())?;
        report.books += 1;

        for source in sources {
            if options.cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                return Ok(report);
            }
            done += 1;
            (options.on_progress)(NceImportProgress {
                stage: "lesson".into(),
                book_no,
                lesson_no: source.lesson_no,
                done,
                total,
                message: source.title.clone(),
            });

            let lesson_id = format!("nce:{book_no}:{}", source.lesson_no);
            let Some(lrc_path) = &source.lrc else {
                report.skipped += 1;
                report
                    .issues
                    .push(format!("{lesson_id} {}: missing .lrc", source.title));
                continue;
            };

            let lrc_text = match read_text_lossy(lrc_path) {
                Ok(text) => text,
                Err(error) => {
                    report.skipped += 1;
                    report.issues.push(format!("{lesson_id}: {error}"));
                    continue;
                }
            };
            let parsed = parse_lrc(&lrc_text);
            if parsed.lines.is_empty() {
                report.skipped += 1;
                report
                    .issues
                    .push(format!("{lesson_id}: no timed sentences in .lrc"));
                continue;
            }
            for issue in parsed.issues.iter().take(3) {
                report.issues.push(format!("{lesson_id}: {issue}"));
            }

            let audio_path = match &source.mp3 {
                Some(mp3) => match copy_media(mp3, &options.media_dir, book_no) {
                    Ok((target, bytes)) => {
                        report.media_files += usize::from(bytes > 0);
                        report.media_bytes += bytes;
                        Some(target.display().to_string())
                    }
                    Err(error) => {
                        report.issues.push(format!("{lesson_id}: audio {error}"));
                        None
                    }
                },
                None => {
                    report
                        .issues
                        .push(format!("{lesson_id}: missing .mp3 (listening disabled)"));
                    None
                }
            };

            let sentences: Vec<LessonSentence> = parsed
                .lines
                .iter()
                .enumerate()
                .map(|(sequence, line)| LessonSentence {
                    id: format!("{lesson_id}#{sequence}"),
                    lesson_id: lesson_id.clone(),
                    sequence: sequence as u32,
                    start_ms: line.start_ms,
                    end_ms: line.end_ms,
                    english: line.english.clone(),
                    chinese: line.chinese.clone(),
                })
                .collect();
            let duration_ms = sentences.last().map(|sentence| sentence.end_ms);
            let vocab = extract_lesson_vocab(store, &lesson_id, &sentences);

            let lesson = CourseLesson {
                id: lesson_id.clone(),
                book_id: format!("nce:{book_no}"),
                lesson_no: source.lesson_no,
                title: source.title.clone(),
                audio_path,
                duration_ms,
                sentence_count: sentences.len() as u32,
                vocab_count: vocab.len() as u32,
            };
            store
                .replace_lesson_content(&lesson, &sentences, &vocab)
                .map_err(|error| format!("{lesson_id}: {error}"))?;
            report.lessons += 1;
            report.sentences += sentences.len();
            report.vocab += vocab.len();
        }
        let _ = folder;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, content).expect("write");
    }

    fn sample_source() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write(
            &root.join("NCE1/001&002－Excuse Me.lrc"),
            "[ti:Excuse Me!]\n[00:00.61]Lesson 1|第1课\n[00:02.71]Excuse me!|打扰一下！\n[00:05.61]Yes?|是的？\n",
        );
        write(&root.join("NCE1/001&002－Excuse Me.mp3"), "fake-mp3-bytes");
        write(
            &root.join("NCE1/003&004－Sorry, Sir..lrc"),
            "[00:00.50]Lesson 3|第3课\n[00:02.00]My coat and my umbrella please.|请把我的大衣和雨伞给我。\n",
        );
        // 无 mp3 的课也要能导入（listening 禁用、进 issues）。
        write(
            &root.join("data.json"),
            r#"{"1": [{"title": "Excuse Me", "filename": "001&002－Excuse Me"}, {"title": "Sorry, Sir.", "filename": "003&004－Sorry, Sir."}]}"#,
        );
        dir
    }

    #[test]
    fn scan_finds_books_and_lessons() {
        let dir = sample_source();
        let report = scan_nce_source(dir.path());
        assert_eq!(report.books.len(), 1);
        assert_eq!(report.books[0].book_no, 1);
        assert_eq!(report.books[0].lessons.len(), 2);
        assert_eq!(report.total_lessons, 2);
        assert!(report.books[0].lessons[0].has_audio);
        assert!(!report.books[0].lessons[1].has_audio);
    }

    #[test]
    fn lesson_filename_parsing_covers_real_shapes() {
        assert_eq!(
            parse_lesson_filename("001&002－Excuse Me"),
            Some((1, "Excuse Me".to_string()))
        );
        assert_eq!(
            parse_lesson_filename("001&002.Excuse Me"),
            Some((1, "Excuse Me".to_string()))
        );
        assert_eq!(
            parse_lesson_filename("017－Always Young"),
            Some((17, "Always Young".to_string()))
        );
        assert_eq!(
            parse_lesson_filename("003&004－Sorry, Sir."),
            Some((3, "Sorry, Sir".to_string()))
        );
    }

    #[test]
    fn import_is_idempotent_and_tolerates_missing_audio() {
        let dir = sample_source();
        let db_dir = tempfile::tempdir().expect("db tempdir");
        let store = LanguageStore::open(db_dir.path().join("language.db")).expect("open");
        let media = tempfile::tempdir().expect("media tempdir");
        let options = NceImport {
            source_dir: dir.path().to_path_buf(),
            media_dir: media.path().to_path_buf(),
            cancel: Arc::new(AtomicBool::new(false)),
            on_progress: Box::new(|_| {}),
        };
        let report = import_nce(&store, &options).expect("import");
        assert_eq!(report.lessons, 2);
        assert_eq!(report.sentences, 5);
        assert!(!report.issues.is_empty()); // 第二课缺 mp3 有记录

        // 再导一次：内容覆盖、行数不变。
        let second = import_nce(&store, &options).expect("re-import");
        assert_eq!(second.lessons, 2);
        assert_eq!(store.book_lessons("nce:1").expect("lessons").len(), 2);
        let sentences = store.lesson_sentences("nce:1:1").expect("sentences");
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[1].chinese.as_deref(), Some("打扰一下！"));
        // 音频落地且进度表未被内容导入触碰。
        let lesson = store
            .course_lesson("nce:1:1")
            .expect("lesson")
            .expect("some");
        assert!(lesson.audio_path.is_some());
        assert!(lesson.duration_ms.is_some());
    }

    #[test]
    fn import_can_be_cancelled() {
        let dir = sample_source();
        let db_dir = tempfile::tempdir().expect("db tempdir");
        let store = LanguageStore::open(db_dir.path().join("language.db")).expect("open");
        let media = tempfile::tempdir().expect("media tempdir");
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_inside = Arc::clone(&cancel);
        let options = NceImport {
            source_dir: dir.path().to_path_buf(),
            media_dir: media.path().to_path_buf(),
            cancel,
            on_progress: Box::new(move |_| {
                cancel_inside.store(true, Ordering::Relaxed);
            }),
        };
        let report = import_nce(&store, &options).expect("import");
        assert!(report.cancelled);
        assert!(report.lessons < 2);
    }

    #[test]
    fn gbk_encoded_lrc_decodes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lrc_path = dir.path().join("test.lrc");
        // GBK 编码「打扰一下！」。
        let mut bytes = b"[00:01.00]Excuse me!|".to_vec();
        bytes.extend_from_slice("打扰一下！".as_bytes()); // 已是 UTF-8；构造混合场景
        fs::write(&lrc_path, &bytes).expect("write");
        let text = read_text_lossy(&lrc_path).expect("decode utf8");
        assert!(text.contains("打扰一下！"));
        // 纯 GBK 字节（非 UTF-8）：
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode("第1课");
        let mut raw = b"[00:01.00]Lesson 1|".to_vec();
        raw.extend_from_slice(&gbk_bytes);
        fs::write(&lrc_path, &raw).expect("write gbk");
        let text = read_text_lossy(&lrc_path).expect("decode gbk");
        assert!(text.contains("第1课"));
    }
}
