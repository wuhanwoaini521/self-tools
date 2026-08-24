"""左侧文档栏：最近使用的文档列表。

第一阶段只做「最近文档」；未来多文档 / 工作区功能可在此扩展。
"""
from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QLabel, QListWidget, QListWidgetItem, QVBoxLayout, QWidget


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
    def refresh(self, paths: list[Path], current: Path | None = None) -> None:
        self._list.blockSignals(True)
        self._list.clear()
        for p in paths:
            item = QListWidgetItem(p.name)
            item.setData(Qt.ItemDataRole.UserRole, str(p))
            item.setToolTip(str(p))
            self._list.addItem(item)
            if current is not None and p == current:
                item.setSelected(True)
        self._list.blockSignals(False)

    def _on_item_clicked(self, item: QListWidgetItem) -> None:
        self.openRequested.emit(Path(item.data(Qt.ItemDataRole.UserRole)))
