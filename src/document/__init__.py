from __future__ import annotations

from typing import TYPE_CHECKING

from .parser import cycle_task_mark, make_task_line, match_task, set_task_mark
from .task_state import TaskState, TaskStateRegistry, create_default_registry

if TYPE_CHECKING:
    from .document import Document

__all__ = [
    "TaskState",
    "TaskStateRegistry",
    "create_default_registry",
    "match_task",
    "make_task_line",
    "set_task_mark",
    "cycle_task_mark",
    "Document",
]


def __getattr__(name: str):
    """按需加载 Qt 文档模型，使纯 Markdown 规则不依赖 PySide6。"""
    if name == "Document":
        from .document import Document

        return Document
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
