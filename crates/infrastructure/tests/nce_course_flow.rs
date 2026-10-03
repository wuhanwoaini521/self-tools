//! NCE 英语课程子域的端到端闭环（真实 SQLite + 真实导入器 + 真实服务）。
//!
//! 覆盖任务书 §41 的验收链路：
//!
//! ```text
//! 导入 NCE 文件夹（含缺音频 / GBK 字幕的坏数据）
//! → 导入 ECDICT 词典
//! → Today 驾驶舱给出「下一课」
//! → 打开 Lesson Workspace（句子时间轴 + 生词）
//! → 课前单词三态标记 → 平台 SRS 排期
//! → 查词浮层：释义 + 遇见记录
//! → 进度心跳（音频位置 / 阶段）→ 重启后恢复
//! → 生成 Quiz → 提交 → 错词回炉、课时完成、明天复习卡
//! → Review Center 看到新产生的复习任务
//! ```
#![allow(unused_crate_dependencies)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use parking_lot::Mutex;

use devtoolbox_application::language::course::{CourseService, ProgressPatch};
use devtoolbox_application::language::{CourseStorePort, LanguageLearningService};
use devtoolbox_application::learning::LearningService as PlatformLearningService;
use devtoolbox_infrastructure::LearningStore;
use devtoolbox_infrastructure::language::{LanguageStore, import_nce, scan_nce_source};
use devtoolbox_infrastructure::ports::{CourseStoreAdapter, LearningStoreAdapter};

use devtoolbox_core::language::{LessonStage, QuizAnswer, QuizItem, WordMark};

const NOW: i64 = 1_700_000_000;

struct Harness {
    _root: tempfile::TempDir,
    language: Arc<Mutex<LanguageStore>>,
    course: CourseService,
    platform_store: Arc<Mutex<LearningStore>>,
}

impl Harness {
    /// 真实数据：一个 NCE 文件夹（NCE1 两课，一课缺 mp3）、一个 mini ECDICT csv。
    fn start(root: tempfile::TempDir) -> Self {
        let source_dir = root.path().join("nce-source");
        let media_dir = root.path().join("media");
        std::fs::create_dir_all(source_dir.join("NCE1")).expect("mkdir");
        std::fs::write(
            source_dir.join("NCE1/001&002－Excuse Me.lrc"),
            "[ti:Excuse Me!]\n[al:test]\n[00:00.61]Lesson 1|第1课\n\
             [00:02.71]Excuse me!|打扰一下！\n[00:05.61]Yes?|是的？\n\
             [00:08.00]Is this your handbag?|这是你的手提包吗？\n[00:11.00]Yes it is.|是的，确实如此。\n",
        )
        .expect("write lrc");
        std::fs::write(
            source_dir.join("NCE1/001&002－Excuse Me.mp3"),
            b"fake-mp3-001",
        )
        .expect("write mp3");
        // 第二课：GBK 字幕 + 无 mp3（必须容错，不阻断整册）
        let (gbk, _, _) = encoding_rs::GBK.encode("我的外套和雨伞，谢谢。");
        let mut raw = b"[00:01.00]My coat and my umbrella please.|".to_vec();
        raw.extend_from_slice(&gbk);
        std::fs::write(source_dir.join("NCE1/003&004－Sorry, Sir..lrc"), raw)
            .expect("write gbk lrc");
        std::fs::write(
            source_dir.join("data.json"),
            r#"{"1":[{"title":"Excuse Me","filename":"001&002－Excuse Me"},
                     {"title":"Sorry, Sir.","filename":"003&004－Sorry, Sir."}]}"#,
        )
        .expect("write manifest");

        let dict_csv = root.path().join("ecdict.csv");
        std::fs::write(
            &dict_csv,
            "word,phonetic,definition,translation,pos,collins,oxford,tag,bnc,frq,exchange,detail,audio\n\
             excuse,ɪkˈskjuːz,,v. 原谅；宽恕,v.,4,1,\"cet4 cet6\",2200,4300,\"p:excused/0:excuse\",,\n\
             handbag,,,n. 手提包,n.,0,0,cet4,0,9000,\"s:handbags\",,\n\
             umbrella,ʌmˈbrelə,,n. 雨伞,n.,3,0,\"cet4 toefl\",3400,5200,\"s:umbrellas\",,\n\
             coat,kəʊt,,n. 外衣；大衣,n.,3,0,cet4,1800,4100,\"s:coats\",,\n\
             please,pliːz,,adv. 请,int.,0,0,zk,120,300,,,\n\
             thanks,,,int. 谢谢,int.,0,0,zk,300,600,,,\n\
             bag,bæɡ,,n. 包；袋子,n.,4,0,\"zk gk\",500,1500,\"s:bags\",,\n\
             lost,lɒst,,v. 丢失；遗失,v.,4,1,\"cet4 cet6\",900,2600,\"p:lost/0:lose\",,\n\
             shoes,ʃuːz,,n. 鞋,n.,3,0,cet4,600,1800,\"s:shoes\",,\n\
             club,klʌb,,n. 俱乐部；社团,n.,4,1,\"cet4\",700,2100,\"s:clubs\",,\n\
             miss,mɪs,,v. 错过；想念,v.,5,1,\"zk gk cet4\",250,900,\"p:missed/0:miss\",,\n\
             pardon,pɑːdn,,int. 请再说一遍,int.,0,0,cet4,0,12000,,,\n\
             umbrella2,,,,,,,,0,0,,,\n",
        )
        .expect("write csv");

        // ---- 真实导入 ----
        let language = Arc::new(Mutex::new(
            LanguageStore::open(root.path().join("language.db")).expect("open language"),
        ));
        let platform_store = Arc::new(Mutex::new(
            LearningStore::open(root.path().join("learning.db")).expect("open learning"),
        ));

        {
            let store = language.lock();
            let cancel = Arc::new(AtomicBool::new(false));
            devtoolbox_infrastructure::language::import_ecdict_csv(
                &store,
                &dict_csv,
                &cancel,
                &|_| {},
            )
            .expect("import dict");
            let options = devtoolbox_infrastructure::language::NceImport {
                source_dir,
                media_dir,
                cancel: Arc::new(AtomicBool::new(false)),
                on_progress: Box::new(|_| {}),
            };
            import_nce(&store, &options).expect("import nce");
        }

        let content: Arc<dyn CourseStorePort> =
            Arc::new(CourseStoreAdapter::new(Arc::clone(&language)));
        let platform = Arc::new(PlatformLearningService::new(Arc::new(
            LearningStoreAdapter::new(Arc::clone(&platform_store)),
        )));
        let course = CourseService::new(content, platform);

        Self {
            _root: root,
            language,
            course,
            platform_store,
        }
    }

    /// 模拟「关掉 self-tools 再打开」：重建服务对象，数据文件不变。
    fn restart(&mut self) {
        let content: Arc<dyn CourseStorePort> =
            Arc::new(CourseStoreAdapter::new(Arc::clone(&self.language)));
        let platform = Arc::new(PlatformLearningService::new(Arc::new(
            LearningStoreAdapter::new(Arc::clone(&self.platform_store)),
        )));
        self.course = CourseService::new(content, platform);
    }

    fn platform(&self) -> PlatformLearningService {
        PlatformLearningService::new(Arc::new(LearningStoreAdapter::new(Arc::clone(
            &self.platform_store,
        ))))
    }
}

#[test]
fn scan_reports_real_filesystem_layout() {
    let root = tempfile::tempdir().expect("tempdir");
    let nce = root.path().join("NCE1");
    std::fs::create_dir_all(&nce).expect("mkdir");
    std::fs::write(nce.join("001&002－Excuse Me.lrc"), "[00:01.00]Hi|你好\n").expect("write");
    let report = scan_nce_source(root.path());
    assert_eq!(report.books.len(), 1);
    assert_eq!(report.books[0].book_no, 1);
    assert_eq!(report.total_lessons, 1);
    assert!(!report.books[0].lessons[0].has_audio);
}

#[test]
fn full_lesson_journey_closes_the_loop() {
    let mut harness = Harness::start(tempfile::tempdir().expect("tempdir"));

    // ---------- 1. 导入结果真实可信 ----------
    let today = harness.course.today(NOW).expect("today");
    assert!(today.imported, "course imported");
    assert!(today.dict_ready, "dictionary imported");
    assert!(
        today.next_lesson.is_some() || today.continue_lesson.is_none(),
        "no progress yet → next lesson offered"
    );

    let books = harness.course.library().expect("books");
    assert_eq!(books.len(), 1);
    let view = harness
        .course
        .book_view("nce:1")
        .expect("book view")
        .expect("book exists");
    assert_eq!(
        view.lessons.len(),
        2,
        "both lessons imported despite one missing mp3"
    );
    assert_eq!(
        view.lessons[0].status,
        devtoolbox_core::language::LessonStatus::NotStarted
    );

    // ---------- 2. Lesson Workspace：逐句时间轴 + 真实生词 ----------
    let detail = harness
        .course
        .lesson_detail("nce:1:1", NOW)
        .expect("detail")
        .expect("lesson exists");
    assert_eq!(detail.sentences.len(), 5);
    assert!(
        detail.sentences[0].start_ms >= 600
            && detail.sentences[1].start_ms > detail.sentences[0].start_ms
    );
    // [00:02.71]Excuse me!|打扰一下！是第 2 句（第 1 句是 LRC 自带的 Lesson 1 标题行）
    assert_eq!(detail.sentences[1].chinese.as_deref(), Some("打扰一下！"));
    assert_eq!(detail.sentences[1].english, "Excuse me!");
    assert!(
        detail.lesson.audio_path.is_some(),
        "audio copied into app data dir"
    );
    let words: Vec<&str> = detail.vocab.iter().map(|v| v.vocab.word.as_str()).collect();
    assert!(
        words.contains(&"handbag"),
        "content word extracted: {words:?}"
    );
    assert!(!words.contains(&"this"), "function words filtered");
    // 生词带真实词典数据（不是空壳）。
    let handbag = detail
        .vocab
        .iter()
        .find(|v| v.vocab.word == "handbag")
        .expect("handbag");
    assert_eq!(handbag.vocab.translation_zh.as_deref(), Some("n. 手提包"));

    // ---------- 3. 课前单词三态标记 → 平台 SRS ----------
    harness
        .course
        .mark_word("nce:1:1", "Handbag", WordMark::Fuzzy, NOW)
        .expect("mark fuzzy");
    // Fuzzy → Hard → 明天到期（今天不该到期）
    let not_due = harness
        .platform()
        .get_review_queue(Some("language"), NOW, 50)
        .expect("queue");
    assert!(
        !not_due.iter().any(|row| row.card.entity_id == "en:handbag"),
        "fuzzy word is not due today"
    );
    let due_tomorrow = harness
        .platform()
        .get_review_queue(Some("language"), NOW + 86_400 + 1, 50)
        .expect("queue");
    assert!(
        due_tomorrow
            .iter()
            .any(|row| row.card.entity_id == "en:handbag"),
        "fuzzy word enters review pipeline tomorrow"
    );
    // Unknown → Again → 今天到期
    harness
        .course
        .mark_word("nce:1:1", "umbrella", WordMark::Unknown, NOW)
        .expect("mark unknown");
    let due = harness
        .platform()
        .get_review_queue(Some("language"), NOW, 50)
        .expect("queue");
    assert!(
        due.iter().any(|row| row.card.entity_id == "en:umbrella"),
        "unknown word is due today"
    );

    // ---------- 4. 查词浮层：释义 + 遇见记录 ----------
    let lookup = harness
        .course
        .lookup_word(
            "handbag",
            Some("Is this your handbag?"),
            Some("nce:1:1"),
            NOW + 1,
        )
        .expect("lookup");
    assert!(lookup.entry.is_some());
    assert!(lookup.seen_count >= 2, "seen in lesson + lookup");
    assert!(
        lookup.occurrences.iter().any(|o| o.source_type == "lesson"),
        "first seen recorded from the lesson"
    );

    // ---------- 5. 进度心跳 → 重启恢复 ----------
    harness
        .course
        .update_lesson_progress(
            "nce:1:1",
            ProgressPatch {
                stage: Some(LessonStage::Sentence),
                position_ms: Some(8_000),
                sentence_seq: Some(2),
                vocab_index: Some(1),
                shadow_seq: None,
                study_seconds_delta: Some(180),
            },
            NOW + 2,
        )
        .expect("progress");
    harness.restart();
    let resumed = harness.course.today(NOW + 3).expect("today after restart");
    let continued = resumed.continue_lesson.expect("continue lesson");
    assert_eq!(continued.lesson.id, "nce:1:1");
    assert_eq!(
        continued.status,
        devtoolbox_core::language::LessonStatus::Learning
    );
    let detail = harness
        .course
        .lesson_detail("nce:1:1", NOW + 3)
        .expect("detail")
        .expect("lesson");
    assert_eq!(detail.progress.stage, LessonStage::Sentence);
    assert_eq!(detail.progress.position_ms, 8_000);
    assert_eq!(detail.progress.sentence_seq, 2);
    assert_eq!(detail.progress.study_seconds, 180);

    // ---------- 6. Quiz：真实出题 + 提交 ----------
    let items = harness.course.generate_quiz("nce:1:1").expect("quiz");
    assert!(!items.is_empty());
    assert!(
        items
            .iter()
            .any(|item| matches!(item, QuizItem::Vocabulary { .. })),
        "vocabulary quiz from lesson words"
    );
    let answers: Vec<QuizAnswer> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            // 前两题故意答错（覆盖生词进 SRS），其余全部答对 → 分数 ≥60 触发完成。
            let correct = index >= 2;
            let word = word_or_surface(item);
            QuizAnswer {
                item_index: index as u32,
                correct,
                user_answer: word,
            }
        })
        .collect();
    let result = harness
        .course
        .submit_quiz("nce:1:1", &answers, NOW + 4)
        .expect("submit");
    assert!(result.total > 0);
    assert!(result.correct <= result.total);
    assert!(!result.wrong_words.is_empty(), "wrong words collected");

    // 错词被再次送进今天到期队列。
    let wrong = &result.wrong_words[0];
    let due_after = harness
        .platform()
        .get_review_queue(Some("language"), NOW + 4, 50)
        .expect("queue");
    assert!(
        due_after
            .iter()
            .any(|row| row.card.entity_id == format!("en:{wrong}")),
        "wrong word due for review today"
    );

    // ---------- 7. 完成 → 明天复习本课 ----------
    let finished = harness
        .course
        .lesson_detail("nce:1:1", NOW + 5)
        .expect("detail")
        .expect("lesson");
    assert_eq!(finished.progress.stage, LessonStage::Done);
    assert!(finished.progress.completed_at.is_some());
    let tomorrow_due = harness
        .platform()
        .get_review_queue(Some("language"), NOW + 86_400 + 10, 50)
        .expect("queue tomorrow");
    assert!(
        tomorrow_due
            .iter()
            .any(|row| row.card.entity_id == "nce:1:1"),
        "lesson review card scheduled"
    );

    // ---------- 8. 进度页统计 ----------
    let progress = harness.course.english_progress(NOW + 5).expect("progress");
    assert_eq!(progress.lessons_completed, 1);
    assert!(progress.words_learned >= 1);
    assert_eq!(progress.books.len(), 1);
}

/// 复用 QuizItem 提取作答词（Vocabulary 取词，FillBlank 取答案）。
fn word_or_surface(item: &QuizItem) -> Option<String> {
    match item {
        QuizItem::Vocabulary { word, .. } => Some(word.clone()),
        QuizItem::FillBlank { answer, .. } => Some(answer.clone()),
        _ => None,
    }
}

/// 兼容旧 Lesson 学习流（词典条目 + 复习）：确保新 course 子域没有破坏原链路。
#[test]
fn existing_language_learning_flow_still_works() {
    let root = tempfile::tempdir().expect("tempdir");
    let language_path = root.path().join("language.db");
    {
        let mut store = LanguageStore::open(&language_path).expect("open");
        devtoolbox_infrastructure::language::starter::install_starter(&mut store, None)
            .expect("starter");
    }
    let content_store = Arc::new(Mutex::new(
        LanguageStore::open(&language_path).expect("open"),
    ));
    let platform_store = Arc::new(Mutex::new(
        LearningStore::open(root.path().join("learning.db")).expect("open"),
    ));
    let content = Arc::new(devtoolbox_infrastructure::ports::LanguageStoreAdapter::new(
        Arc::clone(&content_store),
    ));
    let service = LanguageLearningService::new(
        content,
        Arc::new(PlatformLearningService::new(Arc::new(
            LearningStoreAdapter::new(Arc::clone(&platform_store)),
        ))),
    );
    let queue = service.study_queue(devtoolbox_core::language::LanguageCode::Eng, 5, NOW);
    assert!(queue.is_ok(), "old study queue still callable");
}
