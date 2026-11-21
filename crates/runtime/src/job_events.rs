//! AI 运行进度事件总线：桌面 Tauri 事件与网页 SSE 共用同一事件源。
//!
//! 设计约束（阶段 1.3）：
//! - 每个 `trace_id` 一条单调递增的 `seq`（从 1 开始），桌面与所有网页订阅者
//!   看到完全相同的序列；
//! - 每个 trace 保留最近 [`DEFAULT_RING_CAPACITY`] 条事件，断线重连可按游标重放；
//! - 成功/失败必须有且只有一个终态事件（`done` / `failed`）：Agent 自己给出的
//!   终态优先，运行器事后补的终态不会重复；
//! - 事件只含阶段与计数/错误码，不含正文、prompt 或隐藏推理。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use devtoolbox_core::personal_ai::{
    AgentProgress, AgentProgressSink, AgentStage, JobProgressEvent, JobRunSnapshot, JobState,
};
use parking_lot::Mutex;
use tokio::sync::broadcast;

/// 每个 trace 保留的重放事件数。
pub const DEFAULT_RING_CAPACITY: usize = 256;
/// 同时跟踪的运行数上限（含已结束的，用于一段时间内重放/查询）。
const MAX_TRACKED_RUNS: usize = 256;

/// 重放失败的原因（HTTP 层映射为 409 / 404）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayError {
    /// 这个 trace 从未在本进程内出现过（例如服务重启）。
    Unknown,
    /// 请求的游标早于环形缓冲的第一条事件，无法补齐。
    CursorExpired { earliest: u64 },
}

struct RunEntry {
    trace_id: String,
    created_ms: u64,
    inner: Mutex<RunInner>,
}

struct RunInner {
    next_seq: u64,
    terminal: bool,
    events: VecDeque<JobProgressEvent>,
    sender: broadcast::Sender<JobProgressEvent>,
}

impl RunEntry {
    fn new(trace_id: &str, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            trace_id: trace_id.to_string(),
            created_ms: now_millis(),
            inner: Mutex::new(RunInner {
                next_seq: 0,
                terminal: false,
                events: VecDeque::with_capacity(capacity.min(64)),
                sender,
            }),
        }
    }
}

/// 按 trace 扇出的进度事件总线。
pub struct AiJobEvents {
    capacity: usize,
    runs: Mutex<HashMap<String, Arc<RunEntry>>>,
}

impl Default for AiJobEvents {
    fn default() -> Self {
        Self::new(DEFAULT_RING_CAPACITY)
    }
}

impl AiJobEvents {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            runs: Mutex::new(HashMap::new()),
        }
    }

    /// 为该 trace 创建一个会记录事件的 sink（已存在则复用同一条序列）。
    pub fn sink(self: &Arc<Self>, trace_id: &str) -> Arc<JobProgressSink> {
        let _ = self.entry(trace_id);
        Arc::new(JobProgressSink {
            events: Arc::clone(self),
            trace_id: trace_id.to_string(),
        })
    }

    /// 订阅实时事件。订阅先于运行开始时也会建立 trace（等待首条事件）。
    pub fn subscribe(&self, trace_id: &str) -> broadcast::Receiver<JobProgressEvent> {
        let entry = self.entry(trace_id);
        entry.inner.lock().sender.subscribe()
    }

    /// 记录一条进度并扇出；终态之后的事件会被丢弃（保证唯一终态）。
    pub fn record(&self, trace_id: &str, progress: &AgentProgress) -> Option<JobProgressEvent> {
        let entry = self.entry(trace_id);
        let mut inner = entry.inner.lock();
        if inner.terminal {
            return None;
        }
        inner.next_seq += 1;
        let event = JobProgressEvent {
            trace_id: entry.trace_id.clone(),
            seq: inner.next_seq,
            stage: progress.stage,
            detail: progress.detail.clone(),
            error_code: progress.error_code.clone(),
            state: JobProgressEvent::state_for(progress.stage),
            timestamp: now_millis(),
        };
        if event.is_terminal() {
            inner.terminal = true;
        }
        inner.events.push_back(event.clone());
        while inner.events.len() > self.capacity {
            inner.events.pop_front();
        }
        // 在锁内发送：订阅者看到的顺序必须与 seq 一致（发送是非阻塞的）。
        // 没有订阅者时发送失败是正常的（桌面端未打开面板）。
        let _ = inner.sender.send(event.clone());
        Some(event)
    }

    /// 最近状态 + 游标；未知 trace（例如服务重启后）返回 `None`。
    #[must_use]
    pub fn snapshot(&self, trace_id: &str) -> Option<JobRunSnapshot> {
        let entry = self
            .runs
            .lock()
            .get(trace_id)
            .map(Arc::clone)
            .filter(|entry| {
                // 只把真正产生过事件的运行算作“已知”；空条目是等待中的订阅。
                !entry.inner.lock().events.is_empty()
            })?;
        let inner = entry.inner.lock();
        let latest = inner.events.back().cloned();
        Some(JobRunSnapshot {
            trace_id: trace_id.to_string(),
            state: latest
                .as_ref()
                .map_or(JobState::Running, |event| event.state),
            cursor: latest.as_ref().map_or(0, |event| event.seq),
            latest,
        })
    }

    /// 按游标重放。`after = None` 表示“尽力给出缓冲内全部事件”（首次连接）；
    /// 显式游标落后于缓冲起点时返回 [`ReplayError::CursorExpired`]。
    pub fn replay(
        &self,
        trace_id: &str,
        after: Option<u64>,
    ) -> Result<Vec<JobProgressEvent>, ReplayError> {
        let entry = self
            .runs
            .lock()
            .get(trace_id)
            .map(Arc::clone)
            .ok_or(ReplayError::Unknown)?;
        let inner = entry.inner.lock();
        if inner.events.is_empty() {
            return Ok(Vec::new());
        }
        let earliest = inner.events.front().map_or(1, |event| event.seq);
        match after {
            None => Ok(inner.events.iter().cloned().collect()),
            Some(after) if after.saturating_add(1) < earliest => {
                Err(ReplayError::CursorExpired { earliest })
            }
            Some(after) => Ok(inner
                .events
                .iter()
                .filter(|event| event.seq > after)
                .cloned()
                .collect()),
        }
    }

    /// 该 trace 是否已经产生过事件（未知运行 vs. 等待开始的订阅）。
    #[must_use]
    pub fn has_events(&self, trace_id: &str) -> bool {
        self.runs
            .lock()
            .get(trace_id)
            .is_some_and(|entry| !entry.inner.lock().events.is_empty())
    }

    fn entry(&self, trace_id: &str) -> Arc<RunEntry> {
        let mut runs = self.runs.lock();
        if let Some(entry) = runs.get(trace_id) {
            return Arc::clone(entry);
        }
        if runs.len() >= MAX_TRACKED_RUNS {
            evict_one(&mut runs);
        }
        let entry = Arc::new(RunEntry::new(trace_id, self.capacity));
        runs.insert(trace_id.to_string(), Arc::clone(&entry));
        entry
    }
}

/// 淘汰一个已结束的旧 trace；都还在运行时淘汰最早创建的。
fn evict_one(runs: &mut HashMap<String, Arc<RunEntry>>) {
    let victim = runs
        .iter()
        .filter(|(_, entry)| entry.inner.lock().terminal)
        .min_by_key(|(_, entry)| entry.created_ms)
        .or_else(|| runs.iter().min_by_key(|(_, entry)| entry.created_ms))
        .map(|(id, _)| id.clone());
    if let Some(id) = victim {
        runs.remove(&id);
    }
}

/// 绑定单个 trace 的进度 sink（实现 core 的 `AgentProgressSink`）。
pub struct JobProgressSink {
    events: Arc<AiJobEvents>,
    trace_id: String,
}

impl JobProgressSink {
    #[must_use]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// 记录并返回带 `seq` 的稳定事件（没有记录时返回 `None`）。
    pub fn record(&self, progress: &AgentProgress) -> Option<JobProgressEvent> {
        self.events.record(&self.trace_id, progress)
    }

    /// 运行器兜底补终态：Agent 已给出终态时返回 `None`（不会重复）。
    pub fn finish_done(&self) -> Option<JobProgressEvent> {
        self.record(&AgentProgress::new(AgentStage::Done))
    }

    /// 运行器兜底补失败终态（取消、运行前置校验失败等）。
    pub fn finish_failed(
        &self,
        error_code: &'static str,
        detail: impl Into<String>,
    ) -> Option<JobProgressEvent> {
        self.record(&AgentProgress::failed(error_code, detail))
    }
}

impl AgentProgressSink for JobProgressSink {
    fn emit(&self, progress: &AgentProgress) {
        let _ = self.record(progress);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus(capacity: usize) -> Arc<AiJobEvents> {
        Arc::new(AiJobEvents::new(capacity))
    }

    fn drain(receiver: &mut broadcast::Receiver<JobProgressEvent>) -> Vec<JobProgressEvent> {
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }

    #[test]
    fn two_subscribers_see_the_same_monotonic_sequence() {
        let events = bus(16);
        let sink = events.sink("run-1");
        let mut first = events.subscribe("run-1");
        let mut second = events.subscribe("run-1");

        sink.emit(&AgentProgress::new(AgentStage::Preparing));
        sink.emit(&AgentProgress::with_detail(
            AgentStage::Thinking,
            "worker 1",
        ));
        sink.finish_done();

        let first = drain(&mut first);
        let second = drain(&mut second);
        assert_eq!(first, second);
        assert_eq!(
            first.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(first[2].state, JobState::Done);
        assert_eq!(first[2].stage, AgentStage::Done);
    }

    #[test]
    fn terminal_events_are_recorded_exactly_once() {
        let events = bus(16);
        let sink = events.sink("run-1");
        sink.finish_done();
        assert!(sink.finish_done().is_none());
        assert!(
            sink.finish_failed("personal_ai_cancelled", "cancelled")
                .is_none()
        );
        let snapshot = events.snapshot("run-1").expect("snapshot");
        assert_eq!(snapshot.state, JobState::Done);
        assert_eq!(snapshot.cursor, 1);
    }

    #[test]
    fn replay_uses_the_ring_and_reports_an_expired_cursor() {
        let events = bus(3);
        let sink = events.sink("run-1");
        for stage in [
            AgentStage::Preparing,
            AgentStage::Thinking,
            AgentStage::CallingTool,
            AgentStage::Composing,
            AgentStage::Done,
        ] {
            sink.emit(&AgentProgress::new(stage));
        }

        let buffered = events.replay("run-1", None).expect("replay");
        assert_eq!(
            buffered.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
        assert_eq!(
            events
                .replay("run-1", Some(4))
                .expect("replay after 4")
                .iter()
                .map(|e| e.seq)
                .collect::<Vec<_>>(),
            vec![5]
        );
        assert!(events.replay("run-1", Some(5)).expect("replay").is_empty());
        assert_eq!(
            events.replay("run-1", Some(0)),
            Err(ReplayError::CursorExpired { earliest: 3 })
        );
    }

    #[test]
    fn unknown_snapshot_is_none_but_subscription_creates_a_waiting_run() {
        let events = bus(8);
        assert!(events.snapshot("never-ran").is_none());
        let mut receiver = events.subscribe("pending");
        assert!(events.snapshot("pending").is_none());
        assert!(matches!(
            receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert_eq!(events.replay("never-ran", None), Err(ReplayError::Unknown));
    }

    #[test]
    fn a_started_subscription_receives_live_events() {
        let events = bus(8);
        let mut receiver = events.subscribe("run-late");
        let sink = events.sink("run-late");
        let _ = sink.record(&AgentProgress::new(AgentStage::Preparing));
        let event = receiver.try_recv().expect("live event");
        assert_eq!(event.seq, 1);
        assert_eq!(event.stage, AgentStage::Preparing);
    }

    #[test]
    fn cancellation_finish_reaches_every_subscriber_once() {
        let events = bus(8);
        let sink = events.sink("run-1");
        let mut first = events.subscribe("run-1");
        let mut second = events.subscribe("run-1");

        let _ = sink.record(&AgentProgress::new(AgentStage::Thinking));
        // 取消路径：Agent 不会 emit 终态，由运行器补一次。
        sink.finish_failed("personal_ai_cancelled", "AI request was cancelled");

        let first = drain(&mut first);
        let second = drain(&mut second);
        assert_eq!(first, second);
        let terminal = first.last().expect("terminal event");
        assert_eq!(terminal.seq, 2);
        assert_eq!(terminal.state, JobState::Failed);
        assert_eq!(
            terminal.error_code.as_deref(),
            Some("personal_ai_cancelled")
        );
        assert_eq!(
            events.snapshot("run-1").expect("snapshot").state,
            JobState::Failed
        );
    }
}
