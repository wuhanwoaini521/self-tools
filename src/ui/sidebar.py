"""左侧文档栏：工作区文件树 + 最近文档。

* 工作区按目录层级以树状展示，点击文件发出 openRequested；
* 分组标题与文件夹节点不可选中，点击标题展开/收起；
* 未来多文档 / 工作区功能可在此扩展。
"""
from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QLabel, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget


class Sidebar(QWidget):
    openRequested = Signal(Path)

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self.setObjectName("Sidebar")
        self.setMinimumWidth(160)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        title = QLabel("DOCUMENTS")
        title.setObjectName("SidebarTitle")
        layout.addWidget(title)

        self._tree = QTreeWidget(self)
        self._tree.setHeaderHidden(True)
        self._tree.setIndentation(14)
        self._tree.setExpandsOnDoubleClick(False)
        self._tree.itemClicked.connect(self._on_item_clicked)
        layout.addWidget(self._tree, 1)

    # ------------------------------------------------------------------
    def refresh(
        self,
        workspace: list[Path],
        recents: list[Path],
        current: Path | None = None,
        workspace_root: Path | None = None,
    ) -> None:
        """重建树：工作区（可选，含目录层级）+ 最近文档两组。"""
        self._tree.blockSignals(True)
        try:
            self._tree.clear()
            current_item: QTreeWidgetItem | None = None
            if workspace_root is not None:
                section = self._add_section(f"工作区 · {workspace_root.name}")
                folder_nodes: dict[tuple[str, ...], QTreeWidgetItem] = {}
                for p in workspace:
                    parent = section
                    parts = p.relative_to(workspace_root).parts
                    for i, folder in enumerate(parts[:-1]):
                        key = parts[: i + 1]
                        node = folder_nodes.get(key)
                        if node is None:
                            node = self._add_folder(parent, folder)
                            folder_nodes[key] = node
                        parent = node
                    item = self._add_file(parent, p)
                    if current is not None and p == current:
                        item.setSelected(True)
                        current_item = item
                section.setExpanded(True)
                if not workspace:
                    self._add_placeholder(section, "（无 Markdown 文件）")
            if recents:
                section = self._add_section("最近文档")
                for p in recents:
                    self._add_file(section, p)
                section.setExpanded(True)
            if current_item is not None:
                self._tree.scrollToItem(current_item)
        finally:
            self._tree.blockSignals(False)

    # -- 构建辅助 ----------------------------------------------------------
    def _add_section(self, text: str) -> QTreeWidgetItem:
        """分组标题：可展开/收起但不可选中。"""
        item = QTreeWidgetItem(self._tree, [text])
        item.setFlags(Qt.ItemFlag.ItemIsEnabled)
        return item

    def _add_folder(self, parent: QTreeWidgetItem, name: str) -> QTreeWidgetItem:
        """文件夹节点：仅用于组织层级，点击切换展开。"""
        item = QTreeWidgetItem(parent, [name])
        item.setFlags(Qt.ItemFlag.ItemIsEnabled)
        return item

    def _add_placeholder(self, parent: QTreeWidgetItem, text: str) -> None:
        item = QTreeWidgetItem(parent, [text])
        item.setFlags(Qt.ItemFlag.NoItemFlags)
        item.setForeground(0, Qt.GlobalColor.gray)

    def _add_file(self, parent: QTreeWidgetItem, path: Path) -> QTreeWidgetItem:
        item = QTreeWidgetItem(parent, [path.name])
        item.setData(0, Qt.ItemDataRole.UserRole, str(path))
        item.setToolTip(0, str(path))
        return item

    # -- 交互 --------------------------------------------------------------
    def _on_item_clicked(self, item: QTreeWidgetItem, _col: int) -> None:
        if item.childCount() > 0:
            # 标题 / 文件夹：点击切换展开状态
            item.setExpanded(not item.isExpanded())
            return
        data = item.data(0, Qt.ItemDataRole.UserRole)
        if data:  # 只有文件条目携带路径
            self.openRequested.emit(Path(data))

    def _iter_file_items(self) -> list[QTreeWidgetItem]:
        """遍历所有携带路径数据的叶子条目（供测试 / 调试）。"""
        result: list[QTreeWidgetItem] = []

        def walk(item: QTreeWidgetItem) -> None:
            for i in range(item.childCount()):
                child = item.child(i)
                if child.data(0, Qt.ItemDataRole.UserRole):
                    result.append(child)
                walk(child)

        for i in range(self._tree.topLevelItemCount()):
            top = self._tree.topLevelItem(i)
            if top.data(0, Qt.ItemDataRole.UserRole):
                result.append(top)
            walk(top)
        return result
