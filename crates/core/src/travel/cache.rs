//! 缓存策略（需求 #十七）。
//!
//! 纯常量与判定函数：Guide 24h / Search 24h / Document 7d。
//! 持久化与读写位于 infrastructure 的 `TravelStore`。

/// 攻略缓存 TTL（秒）：24 小时。
pub const GUIDE_TTL_SECS: i64 = 24 * 60 * 60;
/// 搜索结果缓存 TTL（秒）：24 小时。
pub const SEARCH_TTL_SECS: i64 = 24 * 60 * 60;
/// 文档缓存 TTL（秒）：7 天。
pub const DOCUMENT_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// 判定某条缓存是否仍在有效期。
#[must_use]
pub fn is_fresh(fetched_at: i64, ttl_secs: i64, now: i64) -> bool {
    now.saturating_sub(fetched_at) <= ttl_secs && fetched_at > 0
}

#[cfg(test)]
mod tests {
    use super::{DOCUMENT_TTL_SECS, GUIDE_TTL_SECS, SEARCH_TTL_SECS, is_fresh};

    #[test]
    fn guide_cache_expires_after_24_hours() {
        let now = 1_800_000_000;
        assert!(is_fresh(now - 23 * 3600, GUIDE_TTL_SECS, now));
        assert!(!is_fresh(now - 25 * 3600, GUIDE_TTL_SECS, now));
    }

    #[test]
    fn document_cache_lasts_seven_days() {
        let now = 1_800_000_000;
        assert!(is_fresh(now - 6 * 86_400, DOCUMENT_TTL_SECS, now));
        assert!(!is_fresh(now - 8 * 86_400, DOCUMENT_TTL_SECS, now));
    }

    #[test]
    fn search_cache_uses_day_ttl() {
        let now = 1_800_000_000;
        assert!(is_fresh(now - 86_399, SEARCH_TTL_SECS, now));
        assert!(!is_fresh(now - 86_401, SEARCH_TTL_SECS, now));
    }

    #[test]
    fn zero_timestamp_is_never_fresh() {
        assert!(!is_fresh(0, GUIDE_TTL_SECS, 1_800_000_000));
    }
}
