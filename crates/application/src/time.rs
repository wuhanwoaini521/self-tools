//! 进程级时间戳工具（application 层）。
//!
//! Gate 7.5：`now_unix` 不再从基础设施导入；应用层自行提供（`std::time`）
//! 以避免反向依赖。仅在领域状态确实复杂时才需要 Clock Port——当前不满足。

/// 当前时间戳（Unix epoch 秒）。
pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}