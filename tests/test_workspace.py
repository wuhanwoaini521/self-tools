"""打开文件夹（工作区）功能测试：

* DocumentService.markdown_files_in 递归扫描与跳过规则
* Sidebar 工作区 / 最近文档分组展示与点击信号
* settings 工作区路径存取

运行：QT_QPA_PLATFORM=offscreen uv run pytest tests/test_workspace.py -v
"""
from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication  # noqa: E402

from src.config import settings  # noqa: E402
from src.services.document_service import DocumentService  # noqa: E402
from src.ui.sidebar import Sidebar  # noqa: E402

app = QApplication.instance() or QApplication([])


class TestMarkdownFilesIn(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _touch(self, rel: str) -> Path:
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("# t", encoding="utf-8")
        return p

    def test_recursive_scan_sorted(self):
        a = self._touch("a.md")
        b = self._touch("sub/b.markdown")
        c = self._touch("sub/deep/c.md")
        files = DocumentService.markdown_files_in(self.root)
        self.assertEqual(files, [a, b, c])

    def test_skips_hidden_and_noise_dirs(self):
        keep = self._touch("notes/keep.md")
        self._touch(".hidden/x.md")
        self._touch(".git/x.md")
        self._touch("node_modules/x.md")
        self._touch("__pycache__/x.md")
        files = DocumentService.markdown_files_in(self.root)
        self.assertEqual(files, [keep])

    def test_ignores_non_markdown(self):
        keep = self._touch("a.md")
        self._touch("b.txt")
        self._touch("c.png")
        self.assertEqual(DocumentService.markdown_files_in(self.root), [keep])


class TestSidebarWorkspace(unittest.TestCase):
    def setUp(self) -> None:
        self.sidebar = Sidebar()

    def _paths_at(self) -> list[str]:
        return [
            self.sidebar._list.item(i).data(0x0100)  # Qt.ItemDataRole.UserRole
            for i in range(self.sidebar._list.count())
        ]

    def test_groups_rendered_with_headers(self):
        ws = [Path("/w/a.md"), Path("/w/sub/b.md")]
        recents = [Path("/r/c.md")]
        self.sidebar.refresh(ws, recents, workspace_root=Path("/w"))
        texts = [self.sidebar._list.item(i).text()
                 for i in range(self.sidebar._list.count())]
        self.assertEqual(texts[0], "工作区 · w")
        self.assertEqual(texts[-2], "最近文档")
        # 文件条目携带路径数据，标题不携带
        datas = self._paths_at()
        self.assertEqual(datas[0], None)          # 工作区标题
        self.assertEqual(Path(datas[-1]), Path("/r/c.md"))    # 最近条目

    def test_no_workspace_only_recents(self):
        recents = [Path("/r/c.md")]
        self.sidebar.refresh([], recents)
        texts = [self.sidebar._list.item(i).text()
                 for i in range(self.sidebar._list.count())]
        self.assertEqual(texts, ["最近文档", "c.md"])

    def test_click_file_emits_signal_header_does_not(self):
        from PySide6.QtCore import Qt

        received = []
        self.sidebar.openRequested.connect(received.append)
        self.sidebar.refresh([Path("/w/a.md")], [], workspace_root=Path("/w"))
        # 点击分组标题 → 不发信号
        header = self.sidebar._list.item(0)
        self.sidebar._on_item_clicked(header)
        self.assertEqual(received, [])
        # 点击文件条目 → 发出路径
        file_item = self.sidebar._list.item(1)
        self.assertTrue(file_item.flags() & Qt.ItemFlag.ItemIsSelectable)
        self.sidebar._on_item_clicked(file_item)
        self.assertEqual(received, [Path("/w/a.md")])


class TestWorkspaceSettings(unittest.TestCase):
    def test_roundtrip_and_clear(self):
        settings.set_workspace_path("/some/folder")
        try:
            self.assertEqual(settings.workspace_path(), "/some/folder")
        finally:
            settings.set_workspace_path(None)
        self.assertIsNone(settings.workspace_path())


if __name__ == "__main__":
    unittest.main()
