#![allow(unused_crate_dependencies)]
//! 最终验收脚本（任务书 §41）：真实数据 + 真实 SQLite + 真实服务。
//!
//! 完整场景：
//!   打开 English → 今天该学什么 → Continue → 学词 → 查词 → 阅读 → 跟读 → Quiz
//!   → 完成 → 复习中心出现新任务 → 关闭应用 → 重开后进度仍在
//!
//! 用法：cargo run -p devtoolbox-infrastructure --example acceptance_english
//! （数据来自 config/nce-sample + config/ecdict.csv，均为用户本地真实数据）

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use devtoolbox_application::language::CourseStorePort;
use devtoolbox_application::language::course::{CourseService, ProgressPatch};
use devtoolbox_application::learning::LearningService as PlatformLearningService;
use devtoolbox_core::language::{LessonStage, QuizAnswer, QuizItem, WordMark};
use devtoolbox_infrastructure::LearningStore;
use devtoolbox_infrastructure::language::{LanguageStore, import_nce, scan_nce_source};
use devtoolbox_infrastructure::ports::{CourseStoreAdapter, LearningStoreAdapter};

fn main() {
    let config = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/Users/hans/Code/github/self-tools/config".to_string()),
    );
    let mut failures = 0usize;
    let mut step = 0usize;
    let mut check = |label: &str, ok: bool, detail: String| {
        step += 1;
        if !ok {
            failures += 1;
        }
        println!(
            "{} {step:>2}. {label} {detail}",
            if ok { "PASS" } else { "FAIL" }
        );
    };

    // 用独立数据目录，模拟一次全新的真实安装。
    let work = std::env::temp_dir().join(format!("selftools-acceptance-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("workdir");
    let language = Arc::new(parking_lot::Mutex::new(
        LanguageStore::open(work.join("language.db")).expect("open language"),
    ));
    let platform_store = Arc::new(parking_lot::Mutex::new(
        LearningStore::open(work.join("learning.db")).expect("open learning"),
    ));

    // ---------- 导入真实教材与词典 ----------
    let source = config.join("nce-sample");
    let scan = scan_nce_source(&source);
    check(
        "扫描到本地 NCE 教材",
        scan.total_lessons >= 6,
        format!("{} 册 / {} 课", scan.books.len(), scan.total_lessons),
    );
    {
        let store = language.lock();
        let dict_csv = config.join("ecdict.csv");
        let dict_ok = if dict_csv.exists() {
            let report = devtoolbox_infrastructure::language::import_ecdict_csv(
                &store,
                &dict_csv,
                &Arc::new(AtomicBool::new(false)),
                &|_| {},
            )
            .expect("dict import");
            println!("     词典 {} 条", report.entries);
            true
        } else {
            false
        };
        check("导入 ECDICT 词典", dict_ok, String::new());
        let nce = import_nce(
            &store,
            &devtoolbox_infrastructure::language::NceImport {
                source_dir: source,
                media_dir: work.join("media"),
                cancel: Arc::new(AtomicBool::new(false)),
                on_progress: Box::new(|_| {}),
            },
        )
        .expect("nce import");
        check(
            "导入教材（课文/中文/时间轴/音频）",
            nce.lessons == scan.total_lessons && nce.media_files > 0,
            format!(
                "{} 课 / {} 句 / {} 词 / {} 个音频",
                nce.lessons, nce.sentences, nce.vocab, nce.media_files
            ),
        );
    }

    let build = |language: &Arc<parking_lot::Mutex<LanguageStore>>,
                 platform_store: &Arc<parking_lot::Mutex<LearningStore>>| {
        let content: Arc<dyn CourseStorePort> =
            Arc::new(CourseStoreAdapter::new(Arc::clone(language)));
        let platform = Arc::new(PlatformLearningService::new(Arc::new(
            LearningStoreAdapter::new(Arc::clone(platform_store)),
        )));
        CourseService::new(content, platform)
    };
    let service = build(&language, &platform_store);
    let platform = PlatformLearningService::new(Arc::new(LearningStoreAdapter::new(Arc::clone(
        &platform_store,
    ))));

    // ---------- 1) 今天该学什么 ----------
    let today = service.today(1_700_000_000).expect("today");
    check(
        "首页回答「今天该学什么」",
        today.imported && today.dict_ready && today.next_lesson.is_some(),
        format!(
            "下一课 = {}",
            today
                .next_lesson
                .as_ref()
                .map(|l| format!("Lesson {} {}", l.lesson.lesson_no, l.lesson.title))
                .unwrap_or_else(|| "无".into())
        ),
    );
    let lesson_id = today.next_lesson.expect("next lesson").lesson.id.clone();

    // ---------- 2) 打开这一课 ----------
    let detail = service
        .lesson_detail(&lesson_id, 1_700_000_000)
        .expect("detail")
        .expect("lesson");
    check(
        "Lesson 工作台拿到逐句时间轴 + 中文 + 生词",
        !detail.sentences.is_empty()
            && detail.sentences.iter().any(|s| s.chinese.is_some())
            && !detail.vocab.is_empty(),
        format!(
            "{} 句 / {} 生词 / 音频 {}",
            detail.sentences.len(),
            detail.vocab.len(),
            if detail.lesson.audio_path.is_some() {
                "有"
            } else {
                "无"
            }
        ),
    );

    // ---------- 3) 学本课单词（三态）----------
    let word = detail
        .vocab
        .iter()
        .find(|v| v.vocab.translation_zh.is_some())
        .map(|v| v.vocab.word.clone())
        .expect("vocab word");
    service
        .mark_word(&lesson_id, &word, WordMark::Fuzzy, 1_700_000_000)
        .expect("mark");
    let unknown = detail
        .vocab
        .iter()
        .find(|v| v.vocab.word != word && v.vocab.translation_zh.is_some())
        .map(|v| v.vocab.word.clone())
        .unwrap_or_default();
    if !unknown.is_empty() {
        service
            .mark_word(&lesson_id, &unknown, WordMark::Unknown, 1_700_000_000)
            .expect("mark unknown");
    }
    check(
        "标记生词直接进入复习系统",
        platform
            .get_review_queue(Some("language"), 1_700_000_000, 50)
            .expect("queue")
            .iter()
            .any(|row| row.card.entity_id == format!("en:{unknown}")),
        format!("「{unknown}」标记为不认识 → 今天到期"),
    );

    // ---------- 4) 点击陌生单词查词 ----------
    let target = detail.sentences[0]
        .english
        .split_whitespace()
        .find(|token| {
            let clean: String = token.chars().filter(|c| c.is_ascii_alphabetic()).collect();
            clean.len() > 3
                && !detail
                    .vocab
                    .iter()
                    .any(|v| v.vocab.word == clean.to_lowercase())
        })
        .unwrap_or("lesson")
        .trim_matches(|c: char| !c.is_ascii_alphabetic());
    let lookup = service
        .lookup_word(
            target,
            Some("context sentence"),
            Some(&lesson_id),
            1_700_000_001,
        )
        .expect("lookup");
    check(
        "查词：词典释义 + 「在哪见过」",
        lookup.entry.is_some() && lookup.seen_count >= 1,
        format!(
            "{target} → {}，见过 {} 次",
            lookup
                .entry
                .as_ref()
                .and_then(|e| e.translation_zh.as_ref())
                .map(|t| t
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(18)
                    .collect::<String>())
                .unwrap_or_else(|| "未收录".into()),
            lookup.seen_count
        ),
    );

    // ---------- 5) 进度：做到一半 ----------
    service
        .update_lesson_progress(
            &lesson_id,
            ProgressPatch {
                stage: Some(LessonStage::Sentence),
                position_ms: Some(12_345),
                sentence_seq: Some(3),
                vocab_index: Some(2),
                shadow_seq: None,
                study_seconds_delta: Some(240),
            },
            1_700_000_002,
        )
        .expect("progress");

    // ---------- 6) Quiz ----------
    let items = service.generate_quiz(&lesson_id).expect("quiz");
    let kinds = [
        (
            "词汇",
            items
                .iter()
                .filter(|i| matches!(i, QuizItem::Vocabulary { .. }))
                .count(),
        ),
        (
            "填空",
            items
                .iter()
                .filter(|i| matches!(i, QuizItem::FillBlank { .. }))
                .count(),
        ),
        (
            "听力",
            items
                .iter()
                .filter(|i| matches!(i, QuizItem::Dictation { .. }))
                .count(),
        ),
        (
            "翻译",
            items
                .iter()
                .filter(|i| matches!(i, QuizItem::Translate { .. }))
                .count(),
        ),
    ];
    check(
        "Quiz 覆盖多种题型（全部来自本课内容）",
        kinds.iter().all(|(_, count)| *count > 0),
        kinds
            .iter()
            .map(|(name, count)| format!("{name}{count}"))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let answers: Vec<QuizAnswer> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let word = match item {
                QuizItem::Vocabulary { word, .. } => Some(word.clone()),
                QuizItem::FillBlank { answer, .. } => Some(answer.clone()),
                _ => None,
            };
            QuizAnswer {
                item_index: index as u32,
                correct: index >= 2,
                user_answer: word,
            }
        })
        .collect();
    let result = service
        .submit_quiz(&lesson_id, &answers, 1_700_000_003)
        .expect("submit");
    check(
        "Quiz 计分并完成本课",
        result.score >= 60 && !result.wrong_words.is_empty(),
        format!(
            "{} 分（{}/{}），错词 {:?}",
            result.score, result.correct, result.total, result.wrong_words
        ),
    );

    // ---------- 7) 复习中心出现新任务 ----------
    let due_now = platform
        .get_review_queue(Some("language"), 1_700_000_003, 50)
        .expect("queue")
        .len();
    check(
        "复习中心出现新产生的复习任务",
        due_now > 0,
        format!("今天到期 {due_now} 张"),
    );

    // ---------- 8) 关闭应用再打开 ----------
    drop(service);
    let reopened = build(&language, &platform_store);
    let today_after = reopened.today(1_700_000_004).expect("today after restart");
    let progress = reopened
        .lesson_detail(&lesson_id, 1_700_000_004)
        .expect("detail")
        .expect("lesson")
        .progress;
    check(
        "重启后学习进度仍在（断点续学）",
        today_after.continue_lesson.is_some()
            && progress.completed_at.is_some()
            && progress.quiz_score.is_some(),
        format!(
            "Continue = {:?}，分数 {:?}，累计 {} 秒",
            today_after.continue_lesson.map(|l| l.lesson.id.clone()),
            progress.quiz_score,
            progress.study_seconds
        ),
    );

    // ---------- 9) 学习统计 ----------
    let stats = reopened.english_progress(1_700_000_004).expect("stats");
    check(
        "学习统计反映真实数据",
        stats.lessons_completed >= 1 && stats.words_learned >= 2,
        format!(
            "完成 {} 课 / 学过 {} 词 / 掌握 {} 词 / 复习 {} 张",
            stats.lessons_completed, stats.words_learned, stats.words_mastered, stats.due_reviews
        ),
    );

    let _ = std::fs::remove_dir_all(&work);
    println!(
        "\n=== 验收{}：{}/{} 步通过 ===",
        if failures == 0 { "通过" } else { "未通过" },
        step - failures,
        step
    );
    if failures > 0 {
        std::process::exit(1);
    }
}
