"""任务状态模型。

所有 Checkbox 状态集中在一个注册表（TaskStateRegistry）中管理，
包括：状态 ID、Markdown 表示、显示名称、符号、颜色、循环顺序。

未来增加 Blocked / Failed / Skipped 等状态时，只需要在
``create_default_registry`` 中多 register 一条，业务代码无需修改。
"""
from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class TaskState:
    """单个任务状态的完整描述。"""

    state_id: str      # 程序内部 ID，如 "pending"
    mark: str          # 方括号内的 Markdown 标记，如 " " / "~" / "x"
    label: str         # 显示名称，如 "Pending"
    symbol: str        # UI 符号，如 "○" / "◐" / "●"
    color: str         # 十六进制颜色，如 "#22c55e"
    order: int         # 排序权重

    @property
    def markdown(self) -> str:
        """该状态对应的 Markdown 片段，如 ``[ ]``。"""
        return f"[{self.mark}]"


class TaskStateRegistry:
    """集中管理全部可用任务状态及默认循环顺序。"""

    def __init__(self) -> None:
        self._by_id: dict[str, TaskState] = {}
        self._by_mark: dict[str, TaskState] = {}
        self._cycle: list[str] = []  # 循环顺序中的 state_id

    # -- 注册 -------------------------------------------------------------
    def register(self, state: TaskState, in_cycle: bool = True) -> None:
        self._by_id[state.state_id] = state
        self._by_mark[state.mark] = state
        if in_cycle and state.state_id not in self._cycle:
            self._cycle.append(state.state_id)

    # -- 查询 -------------------------------------------------------------
    @property
    def states(self) -> list[TaskState]:
        return sorted(self._by_id.values(), key=lambda s: s.order)

    def get(self, state_id: str) -> TaskState | None:
        return self._by_id.get(state_id)

    def by_mark(self, mark: str) -> TaskState | None:
        return self._by_mark.get(mark)

    def first(self) -> TaskState:
        """循环顺序中的第一个状态，即新任务的默认状态。"""
        return self._by_id[self._cycle[0]]

    def cycle(self) -> list[TaskState]:
        return [self._by_id[sid] for sid in self._cycle]

    def next_of(self, state_id: str, step: int = 1) -> TaskState:
        """返回循环顺序中向前（step=1）或向后（step=-1）的状态。"""
        try:
            idx = self._cycle.index(state_id)
        except ValueError:
            idx = -1
        return self._by_id[self._cycle[(idx + step) % len(self._cycle)]]


def create_default_registry() -> TaskStateRegistry:
    """创建默认注册表：Pending → Ing → Done。

    未来扩展示例：
        reg.register(TaskState("blocked", "!", "Blocked", "✕",
                               "#ef4444", 3), in_cycle=False)
        reg.register(TaskState("skipped", "/", "Skipped", "⊘",
                               "#a78bfa", 4))
    """
    reg = TaskStateRegistry()
    reg.register(TaskState(
        state_id="pending", mark=" ", label="Pending",
        symbol="○", color="#98a2b3", order=0,
    ))
    reg.register(TaskState(
        state_id="in_progress", mark="~", label="Ing",
        symbol="◐", color="#3b82f6", order=1,
    ))
    reg.register(TaskState(
        state_id="done", mark="x", label="Done",
        symbol="●", color="#22c55e", order=2,
    ))
    return reg
