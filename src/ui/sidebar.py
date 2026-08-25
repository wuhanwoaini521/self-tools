"""左侧文档栏：工作区文件 + 最近文档。

点击任意条目发出 openRequested 信号；分组标题不可点击。
未来多文档 / 工作区功能可在此扩展。
"""
from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QLabel, QListWidget, QListWidgetItem, QVBoxLayout, QWidget


def _header_item(text: str) -> QListWidgetItem:
    item = QListWidgetItem(text)
    item.setFlags(Qt.ItemFlag.NoItemFlags)  # 标题不可选中 / 点击
    item.setForeground(Qt.GlobalColor.gray)
    return item


class Sidebar(QWidget):
    openRequested = Signal(Path)

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setObjectName("Sidebar")
        self.setMinimumWidth(180)
        self.setMaximumWidth(280)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        title = QLabel("DOCUMENTS")
        title.setObjectName("SidebarTitle")
        layout.addWidget(title)

        self._list = QListWidget(self)
        self._list.itemClicked.connect(self._on_item_clicked)
        layout.addWidget(self._list, 1)

    # ------------------------------------------------------------------
    def refresh(
        self,
        workspace: list[Path],
        recents: list[Path],
        current: Path | None = None,
        workspace_root: Path | None = None,
    ) -> None:
        """重建列表：工作区（可选）+ 最近文档两组。"""
        self._list.blockSignals(True)
        try:
            self._list.clear()
            if workspace_root is not None:
                self._list.addItem(_header_item(f"工作区 · {workspace_root.name}"))
                for p in workspace:
                    self._add_file_item(p, current)
                if not workspace:
                    empty = QListWidgetItem("（无 Markdown 文件）")
                    empty.setFlags(Qt.ItemFlag.NoItemFlags)
                    self._list.addItem(empty)
            if recents:
                self._list.addItem(_header_item("最近文档"))
                for p in recents:
                    self._add_file_item(p, current)
        finally:
            self._list.blockSignals(False)

    def _add_file_item(self, path: Path, current: Path | None) -> None:
        item = QListWidgetItem(path.name)
        item.setData(Qt.ItemDataRole.UserRole, str(path))
        item.setToolTip(str(path))
        self._list.addItem(item)
        if current is not None and path == current:
            item.setSelected(True)

    def _on_item_clicked(self, item: QListWidgetItem) -> None:
        data = item.data(Qt.ItemDataRole.UserRole)
        if data:  # 分组标题没有 UserRole 数据，忽略
            self.openRequested.emit(Path(data))
