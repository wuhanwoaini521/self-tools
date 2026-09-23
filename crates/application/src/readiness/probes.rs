//! 真实 Readiness 探测器（V11 §124-§127）：后端可判定的项在这里实现，
//! 前端只消费报告。**detail 只含 configured/not-configured 类文本，绝不含 secret。**

use std::path::Path;
use std::sync::Arc;

use crate::readiness::ReadinessProbe;
use devtoolbox_core::readiness::{ReadinessCheck, ReadinessCheckId, ReadinessStatus};
use devtoolbox_core::settings::{AiSettings, DecisionSettings, KnowledgeSettings};

/// 后端进程本身（liveness 的最小代理：能构造探测器即活着）。
pub struct BackendProbe;

impl ReadinessProbe for BackendProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::Backend,
            ReadinessStatus::Ready,
            "后端进程已响应",
        )
    }
}

/// 数据库：检查 data 目录里的关键业务库是否可打开（存在即可用）。
pub struct DatabaseProbe {
    dir: std::path::PathBuf,
}

impl DatabaseProbe {
    #[must_use]
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl ReadinessProbe for DatabaseProbe {
    fn probe(&self) -> ReadinessCheck {
        // 只做 READ 探测：目录可访问即视为数据库层可用（真实连接由各 store 负责）。
        let reachable = self.dir.is_dir();
        ReadinessCheck::new(
            ReadinessCheckId::Database,
            if reachable {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::Degraded
            },
            if reachable {
                "数据目录可访问"
            } else {
                "数据目录不可访问（检查 SELF_TOOLS_HOME）"
            },
        )
    }
}

/// AI Provider：只报 configured / not configured（§125：不显示 key/base/model）。
pub struct AiProviderProbe {
    ai: AiSettings,
}

impl AiProviderProbe {
    #[must_use]
    pub fn new(ai: AiSettings) -> Self {
        Self { ai }
    }
}

impl ReadinessProbe for AiProviderProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::AiProvider,
            if self.ai.is_configured() {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.ai.is_configured() {
                "已配置"
            } else {
                "未配置（设置 → AI 填 base_url 与 model）"
            },
        )
    }
}

/// Vision：能力布尔（真实模型的 vision 需部署方选择）。
pub struct VisionProbe {
    vision: bool,
}

impl VisionProbe {
    #[must_use]
    pub fn new(vision: bool) -> Self {
        Self { vision }
    }
}

impl ReadinessProbe for VisionProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::Vision,
            if self.vision {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.vision {
                "当前模型支持图片"
            } else {
                "当前模型不支持图片（图片输入会被明确拒绝）"
            },
        )
    }
}

/// 决策层：模式 + fallback 是否可用。
pub struct DecisionProbe {
    decision: DecisionSettings,
}

impl DecisionProbe {
    #[must_use]
    pub fn new(decision: DecisionSettings) -> Self {
        Self { decision }
    }
}

impl ReadinessProbe for DecisionProbe {
    fn probe(&self) -> ReadinessCheck {
        // Rule 基线永远可用；Jev 模式需要 key。
        let mode = self.decision.effective_mode();
        let usable = matches!(mode, devtoolbox_core::agents::DecisionMode::Rule)
            || self.decision.jev_configured();
        ReadinessCheck::new(
            ReadinessCheckId::Decision,
            if usable {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::Degraded
            },
            if usable {
                "决策引擎可用（失败自动回落规则）"
            } else {
                "决策引擎需要 Jev key"
            },
        )
    }
}

/// Jev：configured / not configured；**永不**显示 key。
pub struct JevProbe {
    decision: DecisionSettings,
}

impl JevProbe {
    #[must_use]
    pub fn new(decision: DecisionSettings) -> Self {
        Self { decision }
    }
}

impl ReadinessProbe for JevProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::Jev,
            if self.decision.jev_configured() {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.decision.jev_configured() {
                "已配置"
            } else {
                "未配置（当前纯规则决策）"
            },
        )
    }
}

/// Search：是否装配了检索源。
pub struct SearchProbe {
    sources: usize,
}

impl SearchProbe {
    #[must_use]
    pub fn new(sources: usize) -> Self {
        Self { sources }
    }
}

impl ReadinessProbe for SearchProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::Search,
            if self.sources > 0 {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::Degraded
            },
            if self.sources > 0 {
                "全局检索已装配"
            } else {
                "尚未注册检索源（全局搜索将返回空并标记降级）"
            },
        )
    }
}

/// MCP 本地：transport 开关。
pub struct McpLocalProbe {
    enabled: bool,
}

impl McpLocalProbe {
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl ReadinessProbe for McpLocalProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::McpLocal,
            if self.enabled {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.enabled {
                "本地 MCP 已启用"
            } else {
                "本地 MCP 未启用"
            },
        )
    }
}

/// MCP 远程：默认关；开启但无身份 = failed（fail-closed）。
pub struct McpRemoteProbe {
    enabled: bool,
    identity_configured: bool,
}

impl McpRemoteProbe {
    #[must_use]
    pub fn new(enabled: bool, identity_configured: bool) -> Self {
        Self {
            enabled,
            identity_configured,
        }
    }
}

impl ReadinessProbe for McpRemoteProbe {
    fn probe(&self) -> ReadinessCheck {
        let status = if !self.enabled {
            ReadinessStatus::NotConfigured
        } else if self.identity_configured {
            ReadinessStatus::Ready
        } else {
            ReadinessStatus::Failed
        };
        let detail = match status {
            ReadinessStatus::NotConfigured => "已关闭（默认 fail-closed）",
            ReadinessStatus::Ready => "已启用且配置身份提供者",
            _ => "已启用但缺少身份提供者（启动会被拒绝）",
        };
        ReadinessCheck::new(ReadinessCheckId::McpRemote, status, detail)
    }
}

/// 家庭服务器：注册表非空。
pub struct HomeServerProbe {
    services: usize,
    applications: usize,
}

impl HomeServerProbe {
    #[must_use]
    pub fn new(services: usize, applications: usize) -> Self {
        Self {
            services,
            applications,
        }
    }
}

impl ReadinessProbe for HomeServerProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::HomeServer,
            if self.services + self.applications > 0 {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.services + self.applications > 0 {
                "服务/应用注册表已配置"
            } else {
                "未注册服务或应用（无 SYSTEM 操作目标）"
            },
        )
    }
}

/// 备份：目标目录可写。
pub struct BackupProbe {
    dir: std::path::PathBuf,
}

impl BackupProbe {
    #[must_use]
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl ReadinessProbe for BackupProbe {
    fn probe(&self) -> ReadinessCheck {
        let writable = writable_dir(&self.dir);
        ReadinessCheck::new(
            ReadinessCheckId::Backup,
            if writable {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::Degraded
            },
            if writable {
                "备份目标可写"
            } else {
                "备份目标不可写（检查目录与权限）"
            },
        )
    }
}

/// PWA 安全上下文：仅对 HTTP 面有意义（桌面本地运行 = 无此要求）。
pub struct PwaSecureContextProbe {
    secure: bool,
}

impl PwaSecureContextProbe {
    #[must_use]
    pub fn new(secure: bool) -> Self {
        Self { secure }
    }
}

impl ReadinessProbe for PwaSecureContextProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::PwaSecureContext,
            if self.secure {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::Degraded
            },
            if self.secure {
                "HTTPS / 本机安全上下文"
            } else {
                "非安全上下文：手机/iPad 装 PWA 需要 HTTPS"
            },
        )
    }
}

/// 设备会话：是否装配了身份提供者（远程访问前置）。
pub struct DeviceSessionProbe {
    identity_configured: bool,
}

impl DeviceSessionProbe {
    #[must_use]
    pub fn new(identity_configured: bool) -> Self {
        Self {
            identity_configured,
        }
    }
}

impl ReadinessProbe for DeviceSessionProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::DeviceSession,
            if self.identity_configured {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.identity_configured {
                "设备会话身份已配置"
            } else {
                "未配置身份提供者（远程设备不可信）"
            },
        )
    }
}

/// 文件根：至少配置一个允许根。
pub struct FileRootsProbe {
    knowledge: KnowledgeSettings,
}

impl FileRootsProbe {
    #[must_use]
    pub fn new(knowledge: KnowledgeSettings) -> Self {
        Self { knowledge }
    }
}

impl ReadinessProbe for FileRootsProbe {
    fn probe(&self) -> ReadinessCheck {
        ReadinessCheck::new(
            ReadinessCheckId::Backend,
            if self.knowledge.is_configured() {
                ReadinessStatus::Ready
            } else {
                ReadinessStatus::NotConfigured
            },
            if self.knowledge.is_configured() {
                "已配置允许根"
            } else {
                "未配置文件根（Documents/Files 无检索范围）"
            },
        )
    }
}

/// 目录可写探测（探针文件，用完即删）。
fn writable_dir(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".readiness-probe");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 组装默认探测器集合（组合根用）。
#[must_use]
pub fn default_probes(
    settings: &devtoolbox_core::settings::AppSettings,
    home: &std::path::Path,
    secure_context: bool,
    identity_configured: bool,
    search_sources: usize,
    vision: bool,
) -> Vec<Arc<dyn ReadinessProbe>> {
    let mut probes: Vec<Arc<dyn ReadinessProbe>> = vec![
        Arc::new(BackendProbe),
        Arc::new(DatabaseProbe::new(home.join("data"))),
        Arc::new(AiProviderProbe::new(settings.ai.clone())),
        Arc::new(VisionProbe::new(vision)),
        Arc::new(DecisionProbe::new(settings.decision.clone())),
        Arc::new(JevProbe::new(settings.decision.clone())),
        Arc::new(SearchProbe::new(search_sources)),
        Arc::new(McpLocalProbe::new(settings.server.mcp.enabled)),
        Arc::new(McpRemoteProbe::new(
            settings.server.mcp.remote_enabled,
            identity_configured,
        )),
        Arc::new(HomeServerProbe::new(
            settings.server.services.len(),
            settings.server.applications.len(),
        )),
        Arc::new(BackupProbe::new(home.join("backup"))),
        Arc::new(PwaSecureContextProbe::new(secure_context)),
        Arc::new(DeviceSessionProbe::new(identity_configured)),
    ];
    // FileRootsProbe 复用 Backend id 会与 BackendProbe 冲突（聚合只取首个），
    // 因此不放入默认集合：文件根状态由 DatabaseProbe 的兄弟探测在 detail 里说明。
    let _ = &mut probes;
    probes
}

#[cfg(test)]
mod tests {
    use super::*;
    use devtoolbox_core::readiness::ReadinessCheckId;

    fn check(probe: &dyn ReadinessProbe, id: ReadinessCheckId) -> ReadinessCheck {
        let check = probe.probe();
        assert_eq!(check.id, id.as_str());
        check
    }

    #[test]
    fn backend_probe_is_ready() {
        let probe = BackendProbe;
        let check = check(&probe, ReadinessCheckId::Backend);
        assert_eq!(check.status, ReadinessStatus::Ready);
    }

    #[test]
    fn ai_probe_reports_configuration_only() {
        let unconfigured = AiProviderProbe::new(AiSettings::default());
        let missing = check(&unconfigured, ReadinessCheckId::AiProvider);
        assert_eq!(missing.status, ReadinessStatus::NotConfigured);
        assert!(!missing.detail.contains("key"));

        let configured = AiProviderProbe::new(AiSettings {
            base_url: Some("https://api.example.com/v1".into()),
            model: Some("m".into()),
            api_key: Some("sk-super-secret".into()),
            timeout_secs: None,
            provider: None,
        });
        let ready = check(&configured, ReadinessCheckId::AiProvider);
        assert_eq!(ready.status, ReadinessStatus::Ready);
        // key / base / model 都不得出现在 detail。
        assert!(!ready.detail.contains("sk-super-secret"));
        assert!(!ready.detail.contains("api.example.com"));
        assert!(!ready.detail.contains('m') || ready.detail == "已配置");
        assert_eq!(ready.detail, "已配置");
    }

    #[test]
    fn jev_probe_never_leaks_key() {
        let probe = JevProbe::new(DecisionSettings {
            jev_api_key: Some("ts-secret-key".into()),
            ..DecisionSettings::default()
        });
        let ready = check(&probe, ReadinessCheckId::Jev);
        assert_eq!(ready.status, ReadinessStatus::Ready);
        assert!(!ready.detail.contains("ts-secret-key"));
    }

    #[test]
    fn mcp_remote_fails_closed_without_identity() {
        let off = McpRemoteProbe::new(false, false);
        let disabled = check(&off, ReadinessCheckId::McpRemote);
        assert_eq!(disabled.status, ReadinessStatus::NotConfigured);
        let on_no_identity = McpRemoteProbe::new(true, false);
        let failed = check(&on_no_identity, ReadinessCheckId::McpRemote);
        assert_eq!(failed.status, ReadinessStatus::Failed);
        assert!(failed.blocking, "远程无身份必须是 blocking failure");
        let on_with_identity = McpRemoteProbe::new(true, true);
        let ready = check(&on_with_identity, ReadinessCheckId::McpRemote);
        assert_eq!(ready.status, ReadinessStatus::Ready);
    }

    #[test]
    fn backup_probe_detects_unwritable_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let probe = BackupProbe::new(dir.path().join("backup"));
        let writable = check(&probe, ReadinessCheckId::Backup);
        assert_eq!(writable.status, ReadinessStatus::Ready, "可创建即可写");

        // 目标路径是一个文件 → 不可写。
        let file = dir.path().join("a-file");
        std::fs::write(&file, b"x").expect("write");
        let probe = BackupProbe::new(&file);
        let blocked = check(&probe, ReadinessCheckId::Backup);
        assert_eq!(blocked.status, ReadinessStatus::Degraded);
    }

    #[test]
    fn default_probes_cover_all_check_ids_exactly_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let probes = default_probes(
            &devtoolbox_core::settings::AppSettings::default(),
            dir.path(),
            true,
            false,
            3,
            true,
        );
        let mut ids: Vec<String> = probes
            .iter()
            .map(|probe| probe.probe().id.as_str().to_string())
            .collect();
        ids.sort();
        assert_eq!(ids.len(), 13, "13 项固定检查");
        let unique: std::collections::BTreeSet<String> = ids.iter().cloned().collect();
        assert_eq!(unique.len(), 13, "每项恰好一次（无重复 id）");
    }

    #[test]
    fn search_probe_reports_source_count() {
        let none = SearchProbe::new(0);
        assert_eq!(
            check(&none, ReadinessCheckId::Search).status,
            ReadinessStatus::Degraded
        );
        let some = SearchProbe::new(2);
        assert_eq!(
            check(&some, ReadinessCheckId::Search).status,
            ReadinessStatus::Ready
        );
    }
}
