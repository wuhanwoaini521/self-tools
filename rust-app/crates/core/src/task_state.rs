use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 一个 Markdown checkbox 状态的展示与循环定义。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskState {
    pub state_id: String,
    pub mark: String,
    pub label: String,
    pub symbol: String,
    pub color: String,
    pub order: u16,
}

impl TaskState {
    #[must_use]
    pub fn markdown(&self) -> String {
        format!("[{}]", self.mark)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RegistryError {
    #[error("task state id is duplicated: {0}")]
    DuplicateId(String),
    #[error("task state mark is duplicated: {0}")]
    DuplicateMark(String),
    #[error("task state cycle cannot be empty")]
    EmptyCycle,
}

/// 有限任务状态集合；构建完成后只读，避免运行期间隐式共享可变状态。
#[derive(Clone, Debug)]
pub struct TaskStateRegistry {
    states: Vec<TaskState>,
    by_id: HashMap<String, usize>,
    by_mark: HashMap<String, usize>,
    cycle: Vec<usize>,
}

impl TaskStateRegistry {
    /// 创建 registry。`in_cycle` 为 false 的状态可用于展示但不会被快捷键轮换。
    pub fn new(states: Vec<(TaskState, bool)>) -> Result<Self, RegistryError> {
        let mut entries = Vec::with_capacity(states.len());
        let mut by_id = HashMap::new();
        let mut by_mark = HashMap::new();
        let mut cycle = Vec::new();

        for (state, in_cycle) in states {
            if by_id.contains_key(&state.state_id) {
                return Err(RegistryError::DuplicateId(state.state_id));
            }
            if by_mark.contains_key(&state.mark) {
                return Err(RegistryError::DuplicateMark(state.mark));
            }
            let index = entries.len();
            by_id.insert(state.state_id.clone(), index);
            by_mark.insert(state.mark.clone(), index);
            if in_cycle {
                cycle.push(index);
            }
            entries.push(state);
        }

        if cycle.is_empty() {
            return Err(RegistryError::EmptyCycle);
        }

        Ok(Self {
            states: entries,
            by_id,
            by_mark,
            cycle,
        })
    }

    #[must_use]
    pub fn states(&self) -> Vec<&TaskState> {
        let mut states = self.states.iter().collect::<Vec<_>>();
        states.sort_by_key(|state| state.order);
        states
    }

    #[must_use]
    pub fn get(&self, state_id: &str) -> Option<&TaskState> {
        self.by_id.get(state_id).map(|index| &self.states[*index])
    }

    #[must_use]
    pub fn by_mark(&self, mark: &str) -> Option<&TaskState> {
        self.by_mark.get(mark).map(|index| &self.states[*index])
    }

    #[must_use]
    pub fn first(&self) -> &TaskState {
        &self.states[self.cycle[0]]
    }

    #[must_use]
    pub fn cycle(&self) -> Vec<&TaskState> {
        self.cycle
            .iter()
            .map(|index| &self.states[*index])
            .collect()
    }

    #[must_use]
    pub fn next_of(&self, state_id: &str, step: isize) -> Option<&TaskState> {
        let state_index = *self.by_id.get(state_id)?;
        let cycle_index = self.cycle.iter().position(|index| *index == state_index)?;
        let len = self.cycle.len() as isize;
        let next_index = (cycle_index as isize + step).rem_euclid(len) as usize;
        Some(&self.states[self.cycle[next_index]])
    }
}

/// 当前 Python 应用的默认行为：Pending → Ing → Done。
#[must_use]
pub fn default_registry() -> TaskStateRegistry {
    // 常量定义在编译期受控；此处不可能为空或重复。
    TaskStateRegistry::new(vec![
        (
            TaskState {
                state_id: "pending".to_owned(),
                mark: " ".to_owned(),
                label: "Pending".to_owned(),
                symbol: "○".to_owned(),
                color: "#98a2b3".to_owned(),
                order: 0,
            },
            true,
        ),
        (
            TaskState {
                state_id: "in_progress".to_owned(),
                mark: "~".to_owned(),
                label: "Ing".to_owned(),
                symbol: "◐".to_owned(),
                color: "#3b82f6".to_owned(),
                order: 1,
            },
            true,
        ),
        (
            TaskState {
                state_id: "done".to_owned(),
                mark: "x".to_owned(),
                label: "Done".to_owned(),
                symbol: "●".to_owned(),
                color: "#22c55e".to_owned(),
                order: 2,
            },
            true,
        ),
    ])
    .expect("default task states are unique and have a cycle")
}

#[cfg(test)]
mod tests {
    use super::default_registry;

    #[test]
    fn cycles_in_both_directions() {
        let registry = default_registry();
        assert_eq!(
            registry.next_of("pending", 1).map(|s| &s.state_id),
            Some(&"in_progress".to_owned())
        );
        assert_eq!(
            registry.next_of("pending", -1).map(|s| &s.state_id),
            Some(&"done".to_owned())
        );
    }
}
