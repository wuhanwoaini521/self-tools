//! Memory 写入 gate（V6 §14-§20/§70）——纯函数，确定性，可单测。
//!
//! 三条硬规则：
//! 1. **模型永远不能产生 ACTIVE**：工具调用路径最多 Candidate；
//! 2. **用户确认**（UI 点击或显式保存意图）是 ACTIVE 的唯一来源；
//! 3. **secret 类内容永不入库**（§70：提示改用 credential store）。

use std::sync::LazyLock;

use regex::Regex;

use super::model::{MemoryDraft, MemorySourceType, MemoryStatus};

/// Memory 正文上限（超出拒绝：Memory 不是文档存储，V6 §10/§11）。
pub const MEMORY_MAX_CONTENT_CHARS: usize = 400;
/// 置信度下限（低于此值的候选不值得打扰用户）。
pub const MEMORY_MIN_CONFIDENCE: f32 = 0.2;

/// 写入意图（来自谁）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteIntent {
    /// 模型工具调用（只可能产生 Candidate）。
    ModelTool,
    /// 用户显式表达保存意图（对话里说「记住…」）。
    ExplicitUserIntent,
    /// 用户在 UI 上点击确认。
    UiConfirmation,
}

impl WriteIntent {
    /// 该意图是否允许直接落 ACTIVE（V6 §15）。
    #[must_use]
    pub fn grants_active(self) -> bool {
        matches!(
            self,
            WriteIntent::UiConfirmation | WriteIntent::ExplicitUserIntent
        )
    }
}

/// 写入 gate 结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteDecision {
    pub status: MemoryStatus,
    /// 是否需要在 UI 上请求确认（V6 §25）。
    pub needs_confirmation: bool,
}

/// 决定写入状态：**绝不**让模型路径越权（V6 §113）。
#[must_use]
pub fn resolve_status(intent: WriteIntent) -> WriteDecision {
    match intent {
        WriteIntent::ModelTool => WriteDecision {
            status: MemoryStatus::Candidate,
            needs_confirmation: true,
        },
        // 用户明确说「记住」：仍先落 Candidate 并在 UI 上给出确认卡（§14 允许二选一，
        // 本实现选择更保守的一支：写入即需一次确认动作）。
        WriteIntent::ExplicitUserIntent => WriteDecision {
            status: MemoryStatus::Candidate,
            needs_confirmation: true,
        },
        WriteIntent::UiConfirmation => WriteDecision {
            status: MemoryStatus::Active,
            needs_confirmation: false,
        },
    }
}

/// 敏感内容类别（用于受控错误文案，不记录命中文本）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretKind {
    PrivateKey,
    ApiKey,
    Password,
    Token,
    CredentialPath,
}

impl SecretKind {
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            SecretKind::PrivateKey => "疑似私钥内容",
            SecretKind::ApiKey => "疑似 API key",
            SecretKind::Password => "疑似密码/口令",
            SecretKind::Token => "疑似访问令牌",
            SecretKind::CredentialPath => "疑似凭据文件路径",
        }
    }
}

/// 检测 secret 类内容（V6 §70）。命中 → 拒绝写入，返回类别（不回显内容）。
#[must_use]
pub fn detect_secret(content: &str) -> Option<SecretKind> {
    let text = content.trim();
    if text.is_empty() {
        return None;
    }
    if PRIVATE_KEY.is_match(text) {
        return Some(SecretKind::PrivateKey);
    }
    if API_KEY.is_match(text) {
        return Some(SecretKind::ApiKey);
    }
    if PASSWORD.is_match(text) {
        return Some(SecretKind::Password);
    }
    if TOKEN.is_match(text)
        || TOKEN_JWT.is_match(text)
        || TOKEN_NAMED.is_match(text)
        || CONNECTION_STRING.is_match(text)
    {
        return Some(SecretKind::Token);
    }
    if CREDENTIAL_PATH.is_match(text) {
        return Some(SecretKind::CredentialPath);
    }
    None
}

/// 检测用户是否显式表达了长期保存意图（V6 §14/§26）。
///
/// 仅用于把「记住…」类消息标记为 `EXPLICIT_USER` 候选与生成确认卡；
/// **不**据此直接写库（仍走确认）。
#[must_use]
pub fn detect_explicit_save_intent(message: &str) -> bool {
    let text = message.trim();
    if text.is_empty() {
        return false;
    }
    EXPLICIT_INTENT.is_match(text)
}

/// 从「记住：X」类消息中提取待保存正文（无匹配 → None）。
#[must_use]
pub fn extract_save_content(message: &str) -> Option<String> {
    let text = message.trim();
    let captures = EXPLICIT_INTENT_CAPTURE.captures(text)?;
    let content = captures.get(1)?.as_str().trim();
    let cleaned = content.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '：' | ':' | '。' | '.' | ',' | '，' | '"' | '\'' | '“' | '”'
            )
    });
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

/// 由草案推导来源类型（有显式意图 → `EXPLICIT_USER`）。
#[must_use]
pub fn source_type_for(draft: &MemoryDraft, explicit_intent: bool) -> MemorySourceType {
    if explicit_intent {
        MemorySourceType::ExplicitUser
    } else {
        draft.source_type
    }
}

/// 草案校验（长度 / 空 / 置信度 / secret）。
pub fn validate_draft(draft: &MemoryDraft) -> Result<(), String> {
    let content = draft.content.trim();
    if content.is_empty() {
        return Err("记忆内容为空".to_string());
    }
    if content.chars().count() > MEMORY_MAX_CONTENT_CHARS {
        // 不回显实际长度（拒绝时不回显输入特征）。
        return Err(format!(
            "记忆内容过长（上限 {MEMORY_MAX_CONTENT_CHARS} 字符）：Memory 用于少量长期信息，请改为保存文档"
        ));
    }
    if draft.confidence < MEMORY_MIN_CONFIDENCE {
        return Err(format!(
            "置信度过低（{} < {MEMORY_MIN_CONFIDENCE}），不写入记忆",
            draft.confidence
        ));
    }
    if let Some(kind) = detect_secret(content) {
        return Err(format!(
            "{}：禁止写入 Personal Memory，请改用凭据存储（credential store）",
            kind.reason()
        ));
    }
    // `metadata` 是自由 JSON：展平后同样过 secret 门（§70），
    // 否则凭据可以借结构化字段绕过 `content` 检测落库。
    let flattened = flatten_json(&draft.metadata);
    if let Some(kind) = detect_secret(&flattened) {
        return Err(format!(
            "{}：禁止写入 Personal Memory（metadata 字段），请改用凭据存储（credential store）",
            kind.reason()
        ));
    }
    Ok(())
}

/// 把任意 JSON 值展平为可检测文本（键名 + 叶子值）。
///
/// secret 门必须覆盖结构化字段：`MemoryDraft.metadata` 是调用方自由填的 JSON，
/// 只检测 `content` 会留下「把凭据塞进 metadata」的旁路。
fn flatten_json(value: &serde_json::Value) -> String {
    let mut out = String::new();
    fn walk(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::Null => {}
            serde_json::Value::Bool(inner) => out.push_str(&inner.to_string()),
            serde_json::Value::Number(inner) => out.push_str(&inner.to_string()),
            serde_json::Value::String(inner) => {
                out.push_str(inner);
                out.push(' ');
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    out.push_str(key);
                    out.push(' ');
                    walk(item, out);
                }
            }
        }
    }
    walk(value, &mut out);
    out
}

static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)-----BEGIN [A-Z ]*PRIVATE KEY-----|ssh-rsa AAAA|ssh-ed25519 AAAA")
        .expect("private key regex")
});

static API_KEY: LazyLock<Regex> = LazyLock::new(|| {
    // 前缀覆盖主流云厂 / 模型厂 / CI / 消息平台；另加「长度 ≥ 20 的连续
    // base62 串」通用形态兜底未收录的新厂商（误报由用户确认环节吸收）。
    Regex::new(concat!(
        r"\b(",
        r"sk-[A-Za-z0-9_\-]{16,}", // OpenAI / Anthropic(sk-ant-) / DeepSeek …
        r"|AKIA[0-9A-Z]{12,}",     // AWS access key id
        r"|ghp_[A-Za-z0-9]{20,}",  // GitHub classic PAT
        r"|github_pat_[A-Za-z0-9_]{20,}", // GitHub fine-grained PAT
        r"|AIza[0-9A-Za-z_\-]{30,}", // Google API key
        r"|xoxb-[A-Za-z0-9\-]{10,}", // Slack bot token
        r"|dashscope-[A-Za-z0-9_\-]{8,}", // 阿里云百炼
        r"|[A-Za-z0-9]{32,}",      // 通用高熵 token（base62 连续串）
        r")\b",
    ))
    .expect("api key regex")
});

static PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    // 分隔符可选（中文场景「密码是hunter2」/「密码 hunter2」无标点同样命中），
    // 「为」纳入中文谓词。
    Regex::new(
        r"(?i)(password|passwd|pwd|密码|口令)[\s:=>＝是为]{0,4}\S{4,}|\bpass(word)?\s+is\s+\S{4,}",
    )
    .expect("password regex")
});

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._\-]{12,}").expect("bearer token regex")
});

static TOKEN_JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{4,}")
        .expect("jwt regex")
});

static TOKEN_NAMED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(access|refresh|api|auth)[_-]?(token|key|secret)\s*[:=]\s*\S{8,}")
        .expect("named token regex")
});

static CONNECTION_STRING: LazyLock<Regex> = LazyLock::new(|| {
    // `scheme://user:pass@host` 形态（数据库 / Redis / 消息队列连接串）。
    Regex::new(
        r"(?i)\b(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqp|mssql)://[^\s/:@]+:[^\s/@]+@",
    )
    .expect("connection string regex")
});

static CREDENTIAL_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(id_rsa|id_ed25519|\.ssh[/\\]|\.aws[/\\]|\.gnupg[/\\]|credentials\.json|\.env\b|keychain|\.p12\b|\.pfx\b|\.pem\b)",
    )
    .expect("credential path regex")
});

/// 「记住…」意图（V6 §14）。
static EXPLICIT_INTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(请?记住|帮我记住|牢记|记住这|remember (this|that)|note (this|that) down)")
        .expect("explicit intent regex")
});

static EXPLICIT_INTENT_CAPTURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:请?记住|帮我记住|牢记|remember (?:this|that)|note (?:this|that) down)\s*[:：,，]?\s*(.+)$",
    )
    .expect("explicit intent capture regex")
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::model::{MemoryCategory, MemorySensitivity};

    #[test]
    fn model_tool_can_never_reach_active() {
        for intent in [WriteIntent::ModelTool, WriteIntent::ExplicitUserIntent] {
            let decision = resolve_status(intent);
            assert_eq!(decision.status, MemoryStatus::Candidate);
            assert!(decision.needs_confirmation);
            assert!(!intent.grants_active() || intent == WriteIntent::ExplicitUserIntent);
        }
        let ui = resolve_status(WriteIntent::UiConfirmation);
        assert_eq!(ui.status, MemoryStatus::Active);
        assert!(!ui.needs_confirmation);
        assert!(WriteIntent::UiConfirmation.grants_active());
        assert!(!WriteIntent::ModelTool.grants_active());
    }

    #[test]
    fn secrets_are_rejected() {
        for (sample, expected) in [
            (
                "-----BEGIN RSA PRIVATE KEY-----\nMIIE",
                SecretKind::PrivateKey,
            ),
            ("api key 是 sk-abcdefghijklmnopqrstuvwx", SecretKind::ApiKey),
            ("密码: hunter2xyz", SecretKind::Password),
            ("password = topsecret", SecretKind::Password),
            (
                "token: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcd",
                SecretKind::Token,
            ),
            ("~/.ssh/id_rsa 在这里", SecretKind::CredentialPath),
            ("aws credentials.json 位置", SecretKind::CredentialPath),
        ] {
            assert_eq!(detect_secret(sample), Some(expected), "sample: {sample}");
        }
        // 审查补漏：中文无标点 / 未收录厂商 / 连接串 / fine-grained PAT。
        for (sample, expected) in [
            ("我的登录密码是hunter2xyz", SecretKind::Password),
            ("口令为 hunter2xyz", SecretKind::Password),
            ("AKIAIOSFODNN7EXAMPLE", SecretKind::ApiKey),
            (
                "github_pat_11ABCDEFG0abcdefghijkl_1234567890abcdefghijklmnopqrstuvwxyz",
                SecretKind::ApiKey,
            ),
            ("xoxb-1234567890-abcdefghijkl", SecretKind::ApiKey),
            (
                "连接串 postgres://app:s3cret@db.internal:5432/prod",
                SecretKind::Token,
            ),
            ("refresh_token=abcdef1234567890", SecretKind::Token),
            ("a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6", SecretKind::ApiKey),
        ] {
            assert_eq!(detect_secret(sample), Some(expected), "sample: {sample}");
        }
        for ok in [
            "我喜欢历史类旅行",
            "Docker 数据放在 /Volumes/Data/docker",
            "家中服务器是 macOS",
            "习惯每周整理一次资料",
            "笔记路径 notes/reading-notes.md 与 读书 文章",
        ] {
            assert_eq!(detect_secret(ok), None, "sample: {ok}");
        }
    }

    #[test]
    fn draft_metadata_is_secret_checked() {
        // metadata 是自由 JSON：展平后必须过同一道门（§70）。
        let mut draft = MemoryDraft::new(MemoryCategory::Environment, "家中服务器是 macOS");
        draft.metadata =
            serde_json::json!({"note": "api_key", "value": "sk-abcdefghijklmnopqrstuvwx"});
        let error = validate_draft(&draft).expect_err("metadata 里的 secret 必须被拒");
        assert!(error.contains("metadata"), "{error}");

        let mut clean = MemoryDraft::new(MemoryCategory::Environment, "家中服务器是 macOS");
        clean.metadata = serde_json::json!({"source": "ui", "nested": {"tags": ["home", "mac"]}});
        assert!(validate_draft(&clean).is_ok(), "普通 metadata 不得被误拒");
    }

    #[test]
    fn draft_validation_blocks_empty_long_and_secret() {
        let empty = MemoryDraft::new(MemoryCategory::Preference, "   ");
        assert!(validate_draft(&empty).unwrap_err().contains("为空"));

        let long = MemoryDraft::new(
            MemoryCategory::Preference,
            "x".repeat(MEMORY_MAX_CONTENT_CHARS + 1),
        );
        assert!(validate_draft(&long).unwrap_err().contains("过长"));

        let secret = MemoryDraft::new(MemoryCategory::ProjectFact, "密码: hunter2xyz");
        assert!(
            validate_draft(&secret)
                .unwrap_err()
                .contains("credential store")
        );

        let low =
            MemoryDraft::new(MemoryCategory::Preference, "喜欢历史旅行").with_confidence(0.05);
        assert!(validate_draft(&low).unwrap_err().contains("置信度"));

        let ok = MemoryDraft::new(MemoryCategory::Preference, "喜欢历史旅行")
            .with_sensitivity(MemorySensitivity::Normal);
        assert!(validate_draft(&ok).is_ok());
    }

    #[test]
    fn explicit_intent_detection_and_extraction() {
        assert!(detect_explicit_save_intent(
            "记住：我的 Docker 数据都放在 /Volumes/Data/docker"
        ));
        assert!(detect_explicit_save_intent("请记住我喜欢历史旅行"));
        assert!(detect_explicit_save_intent(
            "Remember this: my server runs macOS"
        ));
        assert!(!detect_explicit_save_intent("今天想吃寿司"));
        assert!(!detect_explicit_save_intent("我刚才打开那个页面了吗"));

        assert_eq!(
            extract_save_content("记住：我的 Docker 数据都放在 /Volumes/Data/docker。"),
            Some("我的 Docker 数据都放在 /Volumes/Data/docker".to_string())
        );
        assert_eq!(
            extract_save_content("请记住我喜欢历史旅行"),
            Some("我喜欢历史旅行".to_string())
        );
        assert_eq!(extract_save_content("记住"), None);
        assert_eq!(extract_save_content("今天想吃寿司"), None);
    }

    #[test]
    fn explicit_intent_marks_source() {
        let draft = MemoryDraft::new(MemoryCategory::Preference, "喜欢历史旅行");
        assert_eq!(
            source_type_for(&draft, true),
            MemorySourceType::ExplicitUser
        );
        assert_eq!(
            source_type_for(&draft, false),
            MemorySourceType::ConversationCandidate
        );
    }
}
