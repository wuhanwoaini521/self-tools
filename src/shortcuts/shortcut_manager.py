"""快捷键集中管理。

所有编辑器快捷键定义在 ``DEFAULT_SHORTCUTS`` 表中，
未来允许用户自定义时只需把表内容搬到设置里重新 install。
文件级快捷键（Ctrl+N / Ctrl+S 等）由菜单 QAction 承担，
这里只管理「编辑动作」类快捷键。

Qt 会自动把 Ctrl 映射为 macOS 上的 Cmd，因此同一份定义
可在 Windows / Linux / macOS 上工作。
"""
from __future__ import annotations

from dataclasses import dataclass

from PySide6.QtCore import QObject, Qt
from PySide6.QtGui import QKeySequence, QShortcut
from PySide6.QtWidgets import QWidget


@dataclass(frozen=True)
class ShortcutSpec:
    action_id: str   # 稳定 ID（未来用于自定义绑定）
    sequence: str    # 默认按键序列
    text: str        # 显示名称
    tip: str = ""


DEFAULT_SHORTCUTS: list[ShortcutSpec] = [
    ShortcutSpec(
        "convert_to_task", "Ctrl+L", "转换为任务",
        "将选中行 / 当前行转换为多状态任务项",
    ),
    ShortcutSpec(
        "cycle_task_next", "Ctrl+Return", "下一个状态",
        "Pending → Ing → Done → Pending",
    ),
    ShortcutSpec(
        "cycle_task_prev", "Ctrl+Shift+Return", "上一个状态",
        "反向切换任务状态",
    ),
]


class ShortcutManager(QObject):
    """按 spec 表安装 QShortcut，并保留引用便于将来重绑定。"""

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._shortcuts: dict[str, QShortcut] = {}

    def install(
        self,
        widget: QWidget,
        handlers: dict[str, callable],
        specs: list[ShortcutSpec] | None = None,
    ) -> None:
        specs = specs or DEFAULT_SHORTCUTS
        for spec in specs:
            handler = handlers.get(spec.action_id)
            if handler is None:
                continue
            shortcut = QShortcut(QKeySequence(spec.sequence), widget)
            shortcut.setContext(Qt.ShortcutContext.WidgetWithChildrenShortcut)
            shortcut.activated.connect(handler)  # 每个 ID 只安装一次
            self._shortcuts[spec.action_id] = shortcut

    def sequence_of(self, action_id: str) -> str:
        sc = self._shortcuts.get(action_id)
        return sc.key().toString() if sc else ""
