#![allow(unused_crate_dependencies)] // 与本包 library target 共享依赖（workspace lint 兼容）

//! `language-data` 开发工具（任务 #46）：从官方原始文件导入语言数据、校验、统计、搜索。
//!
//! ```text
//! language-data import english     <oewn-json.zip|dir> <cmudict.dict>   [--db <path>]
//! language-data import japanese    <JMdict_e(.gz)>                      [--db <path>]
//! language-data import kanji       <kanjidic2.xml(.gz)>                 [--db <path>]
//! language-data import mandarin    <cedict.txt(.gz)>                    [--db <path>]
//! language-data import cantonese   <wordslist.json> [--chars <charlist>] [--english <index>] [--cccanto <file>] [--db <path>]
//! language-data import sentences   <sentences.tsv|csv> --license CC0|CCBY [--db <path>]
//! language-data import starter     [--db <path>]
//! language-data validate           english|japanese|mandarin|sentences <file>
//! language-data stats              [--db <path>]
//! language-data search             <query> [--lang en|jp|zh|yue] [--db <path>]
//! ```
//! 原始文件官方下载地址见 `docs/language/DATA_SOURCES.md`。导入不发生在 App 启动（#47）。
//!
//! Gate 7.5：随 Starter/Importing 一起从 application 迁入 infrastructure
//! （数据 ETL 属于基础设施职责；应用层只保留 `LanguageStorePort` 用例消费面）。

use std::path::PathBuf;
use std::process::ExitCode;

use devtoolbox_infrastructure::error::InfrastructureError;
use devtoolbox_infrastructure::language::{
    ImportError, ImportingError, LanguageStore, StarterError, importing,
};
use devtoolbox_infrastructure::language::{starter, import};

const DEFAULT_DB: &str = "config/language.db";

/// CLI 错误：把底层错误统一成一行可读文本（与旧 `ApplicationError` 用法等价）。
#[derive(Debug)]
enum CliError {
    License(String),
    Message(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::License(message) | Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

impl From<InfrastructureError> for CliError {
    fn from(error: InfrastructureError) -> Self {
        Self::Message(error.to_string())
    }
}

impl From<ImportingError> for CliError {
    fn from(error: ImportingError) -> Self {
        Self::Message(error.to_string())
    }
}

impl From<ImportError> for CliError {
    fn from(error: ImportError) -> Self {
        Self::Message(error.to_string())
    }
}

impl From<StarterError> for CliError {
    fn from(error: StarterError) -> Self {
        match error {
            StarterError::License(message) => Self::License(message),
            StarterError::Store(source) => Self::Message(source.to_string()),
        }
    }
}

fn project_config_db() -> PathBuf {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut current = start.clone();
    loop {
        if current.join("Cargo.toml").is_file() || current.join(".git").exists() {
            return current.join(DEFAULT_DB);
        }
        if !current.pop() {
            return start.join(DEFAULT_DB);
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "help" || args[0] == "--help" {
        print_help();
        return ExitCode::SUCCESS;
    }
    if let Err(error) = run(&args) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn print_help() {
    println!(
        "language-data — 语言数据导入/校验/统计工具\n\
         \n\
         USAGE:\n\
         \x20 language-data import english <oewn-json.zip|dir> <cmudict.dict>   [--db <path>]\n\
         \x20 language-data import japanese <JMdict_e(.gz)>                      [--db <path>]\n\
         \x20 language-data import kanji <kanjidic2.xml(.gz)>                    [--db <path>]\n\
         \x20 language-data import mandarin <cedict.txt(.gz)>                    [--db <path>]\n\
         \x20 language-data import cantonese <wordslist.txt> [--english <index>] [--cccanto <file>] [--db <path>]\n\
         \x20 language-data import sentences <file> --license CC0|CCBY           [--db <path>]\n\
         \x20 language-data import starter                                       [--db <path>]\n\
         \x20 language-data validate <dataset> <file>\n\
         \x20 language-data stats                                               [--db <path>]\n\
         \x20 language-data search <query> [--lang eng|jpn|cmn|yue]             [--db <path>]"
    );
}

fn require_arg<'a>(args: &'a [String], index: usize, hint: &str) -> Result<&'a str, CliError> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::License(hint.to_string()))
}

fn db_path(args: &[String]) -> PathBuf {
    args.iter()
        .position(|arg| arg == "--db")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
        .unwrap_or_else(project_config_db)
}

fn open_store(args: &[String]) -> Result<LanguageStore, CliError> {
    let path = db_path(args);
    LanguageStore::open(&path).map_err(CliError::from)
}

fn run(args: &[String]) -> Result<(), CliError> {
    match args[0].as_str() {
        "import" => {
            let sub = args
                .get(1)
                .ok_or_else(|| CliError::License("import 需要子命令".to_string()))?;
            match sub.as_str() {
                "english" => {
                    let oewn = PathBuf::from(require_arg(args, 2, "缺少 OEWN zip/目录 参数")?);
                    let cmudict = PathBuf::from(require_arg(args, 3, "缺少 cmudict.dict 参数")?);
                    let mut store = open_store(args)?;
                    let (oewn_report, cmu_report) =
                        importing::import_english(&mut store, &oewn, &cmudict, "full-oewn")?;
                    println!("OEWN: +{} / ~{}", oewn_report.inserted, oewn_report.updated);
                    println!(
                        "CMUdict: +{} / ~{}",
                        cmu_report.inserted, cmu_report.updated
                    );
                    Ok(())
                }
                "japanese" => {
                    let path = PathBuf::from(require_arg(args, 2, "缺少 JMdict 文件参数")?);
                    let mut store = open_store(args)?;
                    let report = importing::import_japanese(&mut store, &path, "full-jmdict")?;
                    println!("JMdict: +{} / ~{}", report.inserted, report.updated);
                    Ok(())
                }
                "kanji" => {
                    let path = PathBuf::from(require_arg(args, 2, "缺少 kanjidic2.xml 参数")?);
                    let mut store = open_store(args)?;
                    let report = importing::import_kanji(&mut store, &path, "full-kanjidic2")?;
                    println!("KANJIDIC2: +{} / ~{}", report.inserted, report.updated);
                    Ok(())
                }
                "mandarin" => {
                    let path = PathBuf::from(require_arg(args, 2, "缺少 cedict 文件参数")?);
                    let mut store = open_store(args)?;
                    let report = importing::import_mandarin(&mut store, &path, "full-cedict")?;
                    println!("CC-CEDICT: +{} / ~{}", report.inserted, report.updated);
                    Ok(())
                }
                "cantonese" => {
                    let words =
                        PathBuf::from(require_arg(args, 2, "缺少 words.hk 词表 JSON 参数")?);
                    let mut store = open_store(args)?;
                    let flag = |name: &str| -> Result<Option<PathBuf>, CliError> {
                        args.iter()
                            .position(|arg| arg == name)
                            .map(|index| {
                                args.get(index + 1).map(PathBuf::from).ok_or_else(|| {
                                    CliError::License(format!("{name} 缺少值"))
                                })
                            })
                            .transpose()
                    };
                    let english = flag("--english")?;
                    let cccanto = flag("--cccanto")?;
                    let chars = flag("--chars")?;
                    let report = importing::import_cantonese(
                        &mut store,
                        &words,
                        chars.as_deref(),
                        english.as_deref(),
                        cccanto.as_deref(),
                        "full-words-hk",
                    )?;
                    println!("Cantonese pack: +{} / ~{}", report.inserted, report.updated);
                    Ok(())
                }
                "sentences" => {
                    let path = PathBuf::from(require_arg(args, 2, "缺少 sentences 文件参数")?);
                    let license = match args.iter().position(|arg| arg == "--license") {
                        Some(index) => match args.get(index + 1).map(String::as_str) {
                            Some("CC0") => "CC0 1.0",
                            Some("CCBY") => "CC BY 2.0 FR",
                            other => {
                                return Err(CliError::License(format!(
                                    "--license 需要 CC0 或 CCBY，收到 {other:?}"
                                )));
                            }
                        },
                        None => {
                            return Err(CliError::License(
                                "--license CC0|CCBY 必填".to_string(),
                            ));
                        }
                    };
                    let mut store = open_store(args)?;
                    let report = importing::import_sentences(
                        &mut store,
                        &path,
                        license,
                        "full-tatoeba",
                    )?;
                    println!("Tatoeba: +{} / ~{}", report.inserted, report.updated);
                    Ok(())
                }
                "starter" => {
                    let mut store = open_store(args)?;
                    starter::install_starter(&mut store, None)?;
                    println!("Starter Pack 安装完成");
                    Ok(())
                }
                other => Err(CliError::License(format!("未知 import 子命令：{other}"))),
            }
        }
        "validate" => {
            let dataset = require_arg(
                args,
                1,
                "validate 需要 dataset：english|japanese|mandarin|sentences",
            )?;
            let path = PathBuf::from(require_arg(args, 2, "validate 需要文件参数")?);
            let content = importing::read_raw(&path)?;
            let count = match dataset {
                "english" => {
                    let items =
                        import::oewn::parse(&[("e.json", content.clone())], &[])?;
                    items.len()
                }
                "japanese" => import::jmdict::parse(&content)?.len(),
                "mandarin" => import::cedict::parse(&content)?.len(),
                "sentences" => import::tatoeba::parse(&content, "validate")?.len(),
                other => return Err(CliError::License(format!("未知 dataset：{other}"))),
            };
            println!("{dataset}: 解析 {count} 条");
            Ok(())
        }
        "stats" => {
            let store = open_store(args)?;
            let counts = store.language_counts()?;
            println!("语言数据统计:");
            for count in counts {
                println!(
                    "  {:>4}: words={} phrases={} sentences={} total={}",
                    count.language.code(),
                    count.words,
                    count.phrases,
                    count.sentences,
                    count.total
                );
            }
            let sources = store.sources()?;
            for source in sources {
                let items = store.count_by_source(&source.id)?;
                println!(
                    "  source {} [{}] items={} attribution={}",
                    source.name,
                    source.license.label(),
                    items,
                    source.attribution
                );
            }
            Ok(())
        }
        "search" => {
            let query = require_arg(args, 1, "search 需要查询词")?;
            let lang = args
                .iter()
                .position(|arg| arg == "--lang")
                .and_then(|index| args.get(index + 1))
                .map(String::as_str)
                .and_then(devtoolbox_core::language::LanguageCode::from_code);
            let store = open_store(args)?;
            let hits = store.search(lang, query, 10)?;
            if hits.is_empty() {
                println!("（无结果）");
            }
            for hit in hits {
                let reading = hit.item.reading.as_deref().unwrap_or("-");
                println!(
                    "  [{}] {} ({}) の={} matched={}",
                    hit.item.language.code(),
                    hit.item.text,
                    reading,
                    hit.item.id,
                    hit.matched
                );
            }
            Ok(())
        }
        other => Err(CliError::License(format!("未知命令：{other}（试试 help）"))),
    }
}

