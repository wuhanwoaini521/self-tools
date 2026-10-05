//! 共享组合根：把全部 store 与应用服务装配到一处，桌面端与网页端共用。
//!
//! 之前两端各写一套装配，结果网页端长期缺一半接口（AI / Travel / 知识库 /
//! 语言导入…），每次都要人工补，且补不全。现在只有这一份。

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use devtoolbox_application::geography::GeographyQueryPort;
use devtoolbox_application::history::HistoryQueryPort;
use devtoolbox_application::language::{LanguageService, LanguageStorePort};
use devtoolbox_application::learning::{LearningService, LearningStorePort};
use devtoolbox_application::news::{NewsIngestPort, NewsPort, NewsService};
use devtoolbox_application::rss::RssRepositoryPort;
use devtoolbox_application::study_board::{StudyBoardService, StudyBoardStorePort};
use devtoolbox_application::travel::TravelAiPort;
use devtoolbox_application::travel::session::TravelSessionRegistry;
use devtoolbox_application::workflows::ports::SettingsStorePort;
use devtoolbox_core::knowledge::KnowledgeBudget;
use devtoolbox_core::server::SessionTrust;
use devtoolbox_core::settings::AppSettings;
use devtoolbox_infrastructure::history::HistoryDuckDbRepository;
use devtoolbox_infrastructure::knowledge_runtime::KnowledgeRuntime;
use devtoolbox_infrastructure::language::LanguageStore;
use devtoolbox_infrastructure::news_store::NewsRepository;
use devtoolbox_infrastructure::study_board::StudyBoardSqliteStore;
use devtoolbox_infrastructure::travel::TravelStore;
use devtoolbox_infrastructure::{FeedRepository, GeographyStore, LearningStore, feed_client};

use crate::composition::{
    CourseStoreAdapter, GeographyQueryAdapter, LanguageStoreAdapter, NewsRepositoryAdapter,
    RssRepositoryAdapter, SettingsStoreAdapter, StudyBoardStoreAdapter,
};
use crate::history_enrichment::{self, SettingsLoader};
use crate::history_query::HistoryQueryAdapter;
use crate::server::ServerRuntime;
use crate::travel_ai::TravelAiAdapter;
use devtoolbox_application::history::enrichment::ports::EnrichmentRunnerPort;
use devtoolbox_infrastructure::ports::LearningStoreAdapter;

/// 桌面 / 网页共用的运行时。
pub struct AppCore {
    pub config_dir: std::path::PathBuf,
    pub client: reqwest::Client,
    /// 设置读取器：每次调用读最新 settings.json。
    pub settings_loader: SettingsLoader,
    pub settings_store: devtoolbox_infrastructure::SettingsStore,

    pub rss_repository: Arc<dyn RssRepositoryPort>,
    pub travel_store: Arc<Mutex<TravelStore>>,
    pub travel_registry: Arc<TravelSessionRegistry>,
    pub travel_ai: Arc<dyn TravelAiPort>,
    pub history_repo: Arc<HistoryDuckDbRepository>,
    pub history_port: Arc<dyn HistoryQueryPort>,
    pub history_enrichment: Arc<dyn EnrichmentRunnerPort>,
    pub language_store: Arc<Mutex<LanguageStore>>,
    pub language_port: Arc<dyn LanguageStorePort>,
    pub course_store: Arc<CourseStoreAdapter>,
    pub geography_store: Arc<Mutex<GeographyStore>>,
    pub geography_port: Arc<dyn GeographyQueryPort>,
    pub learning_store: Arc<dyn LearningStorePort>,
    pub learning: Arc<LearningService>,
    /// 具体类型（server 的 router 需要；与 `news` 端口指向同一实例）。
    pub news_service: Arc<NewsService>,
    pub news: Arc<dyn NewsPort>,
    pub rss_service: Arc<devtoolbox_application::rss::RssService>,
    pub rss_ingest: Arc<dyn devtoolbox_application::rss::RssIngestPort>,
    pub news_ingest: Arc<dyn NewsIngestPort>,
    pub knowledge: Arc<KnowledgeRuntime>,
    pub server: Arc<ServerRuntime>,
    /// 学习板存储端口（`config/study_boards.db`）：agent 工具、桌面命令与网页端
    /// 共用同一份 —— 此前学习板只存在于浏览器 localStorage，换设备就没了。
    pub study_board_store: Arc<dyn StudyBoardStorePort>,
    /// 学习板用例服务（列表 / 读取 / 幂等保存 / 快照登记）。
    pub study_board: Arc<StudyBoardService>,
    /// 英语跟读发音评分（V13 W2）：目标句由服务端查库得到，没有转写就没有分数。
    pub speaking: Arc<devtoolbox_application::language::course::SpeakingService>,
}

impl AppCore {
    /// 从数据目录装配全部能力。
    ///
    /// `history_db` 由调用方给出（桌面端从 Tauri 资源解析，网页端用同一份 duckdb 路径）。
    /// 失败一律返回 `Err`，**不静默降级** —— 半装配的运行时比启动失败更难排查。
    pub fn build(
        config_dir: &Path,
        history_db: std::path::PathBuf,
        trust: SessionTrust,
    ) -> Result<Self, String> {
        // 旧库迁移（feeds.kind='news' → news.db）。幂等，但必须在打开 FeedRepository 之前。
        devtoolbox_infrastructure::migrate_news_from_rss(
            &config_dir.join("dashboard.db"),
            &config_dir.join("news.db"),
        )
        .map_err(|error| error.to_string())?;

        let settings_store = devtoolbox_infrastructure::SettingsStore::new(config_dir);
        let settings = SettingsStoreAdapter::new(settings_store.clone());
        // 每次调用读最新 settings.json（两端一致，不缓存启动时的快照）。
        let settings_loader: SettingsLoader = {
            let settings = settings.shared();
            Arc::new(move || settings.load().map_err(|error| error.to_string()))
        };
        let current_settings: AppSettings = settings_loader()?;
        let client = feed_client().map_err(|error| error.to_string())?;

        let rss_repository: Arc<dyn RssRepositoryPort> =
            Arc::new(RssRepositoryAdapter::new(Arc::new(Mutex::new(
                FeedRepository::open(config_dir.join("dashboard.db"))
                    .map_err(|error| error.to_string())?,
            ))));
        let travel_store = Arc::new(Mutex::new(
            TravelStore::open(config_dir.join("travel.db")).map_err(|error| error.to_string())?,
        ));
        let travel_registry = Arc::new(TravelSessionRegistry::new());
        let travel_ai: Arc<dyn TravelAiPort> = Arc::new(TravelAiAdapter::new(
            client.clone(),
            Arc::clone(&settings_loader),
            Arc::clone(&travel_store),
        ));

        let history_repo = Arc::new(
            HistoryDuckDbRepository::open(&history_db).map_err(|error| error.to_string())?,
        );
        let history_port: Arc<dyn HistoryQueryPort> =
            Arc::new(HistoryQueryAdapter::new(Arc::clone(&history_repo)));
        let history_enrichment: Arc<dyn EnrichmentRunnerPort> = history_enrichment::build_runner(
            client.clone(),
            Arc::clone(&settings_loader),
            config_dir,
            Arc::clone(&history_port),
        )
        .map_err(|error| error.to_string())?;

        let language_store = Arc::new(Mutex::new(
            LanguageStore::open(config_dir.join("language.db"))
                .map_err(|error| error.to_string())?,
        ));
        let language_port: Arc<dyn LanguageStorePort> =
            Arc::new(LanguageStoreAdapter::new(Arc::clone(&language_store)));
        let course_store = Arc::new(CourseStoreAdapter::new(Arc::clone(&language_store)));

        let geography_store = Arc::new(Mutex::new(
            GeographyStore::open(config_dir.join("geography.db"))
                .map_err(|error| error.to_string())?,
        ));
        let geography_port: Arc<dyn GeographyQueryPort> =
            Arc::new(GeographyQueryAdapter::new(Arc::clone(&geography_store)));

        let learning_store: Arc<dyn LearningStorePort> =
            Arc::new(LearningStoreAdapter::new(Arc::new(Mutex::new(
                LearningStore::open(config_dir.join("learning.db"))
                    .map_err(|error| error.to_string())?,
            ))));
        let learning = Arc::new(LearningService::new(Arc::clone(&learning_store)));

        // News：只开一次库，读写共用同一个 NewsService（desktop 的既有做法）。
        let news_store = Arc::new(Mutex::new(
            NewsRepository::open(config_dir.join("news.db")).map_err(|error| error.to_string())?,
        ));
        let news_service = Arc::new(NewsService::new(Arc::new(NewsRepositoryAdapter::new(
            Arc::clone(&news_store),
        ))));
        let news: Arc<dyn NewsPort> = Arc::clone(&news_service) as Arc<dyn NewsPort>;
        let news_ingest: Arc<dyn NewsIngestPort> =
            Arc::new(devtoolbox_application::news::NewsIngestService::new(
                Arc::clone(&news_service),
                crate::composition::FeedFetcherAdapter::new(client.clone()),
            ));
        let rss_service = Arc::new(devtoolbox_application::rss::RssService::new(Arc::clone(
            &rss_repository,
        )));
        let rss_ingest: Arc<dyn devtoolbox_application::rss::RssIngestPort> =
            Arc::new(devtoolbox_application::rss::RssIngestService::new(
                Arc::clone(&rss_service),
                crate::composition::FeedFetcherAdapter::new(client.clone()),
            ));

        let knowledge = Arc::new(
            KnowledgeRuntime::build(
                config_dir,
                Arc::clone(&settings_loader),
                KnowledgeBudget::default(),
                Arc::new(LanguageService::new(Arc::clone(&language_port))),
            )
            .map_err(|error| error.to_string())?,
        );
        for note in knowledge.startup_sync() {
            eprintln!("[knowledge] startup sync: {note}");
        }

        let server = ServerRuntime::build(config_dir, Arc::clone(&settings_loader), trust)
            .unwrap_or_else(|error| {
                eprintln!("[server] runtime unavailable: {error}");
                ServerRuntime::assemble(
                    config_dir,
                    Arc::clone(&settings_loader),
                    trust,
                    Vec::new(),
                    Vec::new(),
                    Arc::new(crate::server::DisabledServiceControl),
                )
                .expect("empty server runtime")
            });

        // 学习板：存不住就退到内存库并说明原因，绝不静默丢数据。
        let study_board_store: Arc<dyn StudyBoardStorePort> =
            Arc::new(StudyBoardStoreAdapter::new(Arc::new(
                StudyBoardSqliteStore::open(config_dir.join("study_boards.db")).unwrap_or_else(
                    |error| {
                        eprintln!("[study-board] store unavailable: {error}");
                        StudyBoardSqliteStore::open_in_memory()
                            .expect("in-memory study board store")
                    },
                ),
            )));
        let study_board = Arc::new(StudyBoardService::new(Arc::clone(&study_board_store)));

        // 跟读评分：与课程读写共用同一份 language.db（course_store 适配的是同一个 store）。
        let course_store_port: Arc<dyn devtoolbox_application::language::course::CourseStorePort> =
            course_store.clone();
        let speaking = Arc::new(
            devtoolbox_application::language::course::SpeakingService::new(course_store_port),
        );

        // 供两端复用的学习视图语言 / AI 装配（未配置时 None，不编造）。
        let _ = current_settings;

        Ok(Self {
            config_dir: config_dir.to_path_buf(),
            client,
            settings_loader,
            settings_store,
            rss_repository,
            travel_store,
            travel_registry,
            travel_ai,
            history_repo,
            history_port,
            history_enrichment,
            language_store,
            language_port,
            course_store,
            geography_store,
            geography_port,
            learning_store,
            learning,
            news_service,
            news,
            rss_service,
            rss_ingest,
            news_ingest,
            knowledge,
            server: Arc::new(server),
            study_board_store,
            study_board,
            speaking,
        })
    }
}
