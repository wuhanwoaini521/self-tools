//! 数据集许可模型（#6/#76/#77）。
//!
//! 禁止只保存 `license = "CC"`；必须展开四个能力位，且数据包可判 `CommercialSafe`。

use serde::{Deserialize, Serialize};

/// 许可证种类。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseKind {
    PublicDomain,
    Cc0,
    CcBy,
    CcBySa,
    CcByNc,
    Custom,
    Unknown,
}

impl LicenseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PublicDomain => "Public Domain",
            Self::Cc0 => "CC0 1.0",
            Self::CcBy => "CC BY 4.0",
            Self::CcBySa => "CC BY-SA 4.0",
            Self::CcByNc => "CC BY-NC",
            Self::Custom => "Custom",
            Self::Unknown => "Unknown",
        }
    }
}

/// 许可能力位展开（#6）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SourceLicense {
    pub kind: LicenseKind,
    /// 使用/分发时要求署名。
    pub attribution_required: bool,
    /// 允许商业使用。
    pub commercial_use_allowed: bool,
    /// 允许再分发。
    pub redistribution_allowed: bool,
    /// 派生作品要求相同方式共享。
    pub share_alike_required: bool,
}

impl Default for SourceLicense {
    fn default() -> Self {
        Self {
            kind: LicenseKind::Unknown,
            attribution_required: false,
            commercial_use_allowed: false,
            redistribution_allowed: false,
            share_alike_required: false,
        }
    }
}

impl SourceLicense {
    /// 常见许可的预设（用于常量定义与测试；请与 `docs/language/DATA_SOURCES.md` 保持一致）。
    #[must_use]
    pub const fn public_domain() -> Self {
        Self {
            kind: LicenseKind::PublicDomain,
            attribution_required: false,
            commercial_use_allowed: true,
            redistribution_allowed: true,
            share_alike_required: false,
        }
    }

    #[must_use]
    pub const fn cc0() -> Self {
        Self {
            kind: LicenseKind::Cc0,
            attribution_required: false,
            commercial_use_allowed: true,
            redistribution_allowed: true,
            share_alike_required: false,
        }
    }

    #[must_use]
    pub const fn cc_by() -> Self {
        Self {
            kind: LicenseKind::CcBy,
            attribution_required: true,
            commercial_use_allowed: true,
            redistribution_allowed: true,
            share_alike_required: false,
        }
    }

    #[must_use]
    pub const fn cc_by_sa() -> Self {
        Self {
            kind: LicenseKind::CcBySa,
            attribution_required: true,
            commercial_use_allowed: true,
            redistribution_allowed: true,
            share_alike_required: true,
        }
    }

    #[must_use]
    pub const fn cc_by_nc() -> Self {
        Self {
            kind: LicenseKind::CcByNc,
            attribution_required: true,
            commercial_use_allowed: false,
            redistribution_allowed: true,
            share_alike_required: false,
        }
    }

    #[must_use]
    pub const fn custom() -> Self {
        Self {
            kind: LicenseKind::Custom,
            attribution_required: true,
            commercial_use_allowed: true,
            redistribution_allowed: true,
            share_alike_required: false,
        }
    }

    /// 许可是否完全未知（未声明）——导入前必须校验（#76）。
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        self.kind == LicenseKind::Unknown
    }

    /// 是否可进入默认的商业安全数据包（#77）。
    /// 非商业（NC）许可的数据默认排除；Unknown 一律排除。
    #[must_use]
    pub fn is_commercial_safe(&self) -> bool {
        !self.is_unknown()
            && self.commercial_use_allowed
            && self.redistribution_allowed
            && self.kind != LicenseKind::CcByNc
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.kind.label()
    }
}

/// 数据集来源（#5）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageSource {
    pub id: String,
    pub name: String,
    pub homepage: String,
    pub download_source: String,
    pub dataset_version: String,
    pub downloaded_at: Option<i64>,
    pub license: SourceLicense,
    pub license_url: Option<String>,
    pub attribution: String,
    pub commercial_use: bool,
    pub redistribution: bool,
    pub notes: Option<String>,
}

/// 数据包清单（#43）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DatasetManifest {
    pub id: String,
    pub name: String,
    pub language: String,
    pub version: String,
    pub downloaded_at: Option<i64>,
    pub source_id: String,
    pub checksum: Option<String>,
    pub raw_file: Option<String>,
    pub record_count: i64,
    pub importer_version: i64,
    pub imported_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_license_is_not_commercial_safe() {
        let license = SourceLicense::default();
        assert!(license.is_unknown());
        assert!(!license.is_commercial_safe());
    }

    #[test]
    fn nc_license_never_commercial_safe() {
        assert!(!SourceLicense::cc_by_nc().is_commercial_safe());
        assert!(SourceLicense::cc_by().is_commercial_safe());
        assert!(SourceLicense::public_domain().is_commercial_safe());
        assert!(SourceLicense::cc0().is_commercial_safe());
        assert!(SourceLicense::cc_by_sa().is_commercial_safe());
    }

    #[test]
    fn attribution_flags_match_expected() {
        assert!(SourceLicense::cc_by_sa().attribution_required);
        assert!(SourceLicense::cc_by_sa().share_alike_required);
        assert!(!SourceLicense::public_domain().attribution_required);
        assert!(!SourceLicense::cc_by_nc().commercial_use_allowed);
    }
}
