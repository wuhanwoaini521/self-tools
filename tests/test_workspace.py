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

    def _texts(self) -> list[str]:
        t = self.sidebar._tree
        return [t.topLevelItem(i).text(0) for i in range(t.topLevelItemCount())]

    def test_workspace_tree_structure(self):
        ws = [
            Path("/w/a.md"),
            Path("/w/sub/b.md"),
            Path("/w/sub/deep/c.md"),
        ]
        self.sidebar.refresh(ws, [], workspace_root=Path("/w"))
        top = self.sidebar._tree.topLevelItem(0)
        self.assertEqual(top.text(0), "工作区 · w")
        # 根下：a.md + 文件夹 sub
        root_names = [top.child(i).text(0) for i in range(top.childCount())]
        self.assertEqual(root_names, ["a.md", "sub"])
        # sub 下：b.md + deep；deep 下：c.md
        sub = top.child(1)
        self.assertEqual([sub.child(i).text(0) for i in range(sub.childCount())],
                         ["b.md", "deep"])
        deep = sub.child(1)
        self.assertEqual(deep.child(0).text(0), "c.md")
        # 文件夹与文件可区分：文件夹有目录图标且加粗，文件是文档图标
        from src.ui.sidebar import KIND_FILE, KIND_FOLDER
        self.assertFalse(top.child(0).icon(0).isNull())
        self.assertEqual(top.child(1).data(0, 0x0100 + 1), KIND_FOLDER)
        self.assertTrue(top.child(1).font(0).bold())
        self.assertFalse(top.child(0).font(0).bold())
        self.assertEqual(top.child(0).data(0, 0x0100 + 1), KIND_FILE)
        self.assertFalse(sub.child(0).icon(0).isNull())

    def test_groups_rendered_with_headers(self):
        ws = [Path("/w/a.md")]
        recents = [Path("/r/c.md")]
        self.sidebar.refresh(ws, recents, workspace_root=Path("/w"))
        texts = self._texts()
        self.assertEqual(texts[0], "工作区 · w")
        self.assertEqual(texts[-1], "最近文档")

    def test_no_workspace_only_recents(self):
        recents = [Path("/r/c.md")]
        self.sidebar.refresh([], recents)
        texts = self._texts()
        self.assertEqual(texts[0], "最近文档")
        recent_section = self.sidebar._tree.topLevelItem(0)
        self.assertEqual(recent_section.child(0).text(0), "c.md")

    def test_click_file_emits_signal_header_does_not(self):
        received = []
        self.sidebar.openRequested.connect(received.append)
        self.sidebar.refresh([Path("/w/a.md")], [], workspace_root=Path("/w"))
        # 点击分组标题 → 切换展开而非发信号
        header = self.sidebar._tree.topLevelItem(0)
        expanded_before = header.isExpanded()
        self.sidebar._on_item_clicked(header, 0)
        self.assertEqual(received, [])
        self.assertNotEqual(header.isExpanded(), expanded_before)
        # 点击文件条目 → 发出路径
        file_items = self.sidebar._iter_file_items()
        self.assertEqual(len(file_items), 1)
        self.sidebar._on_item_clicked(file_items[0], 0)
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
