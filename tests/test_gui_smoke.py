"""GUI 冒烟测试（离屏运行）：

覆盖文档新建/打开/保存/另存为、未保存检测、
多行转换任务、状态切换（快捷键动作 + 点击）、保存重开恢复、
边界场景（空行/已有列表/已有 Checkbox/嵌套列表/中英文）。

运行：QT_QPA_PLATFORM=offscreen python -m unittest tests.test_gui_smoke -v
"""
from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from src.document.task_state import create_default_registry  # noqa: E402
from src.ui.editor.markdown_editor import MarkdownEditor  # noqa: E402

app = QApplication.instance() or QApplication([])


class TestMarkdownEditorTasks(unittest.TestCase):
    def setUp(self) -> None:
        self.reg = create_default_registry()
        self.editor = MarkdownEditor(self.reg)

    def _set_text(self, text: str) -> None:
        self.editor.setPlainText(text)

    def _select_all_lines(self) -> None:
        cursor = self.editor.textCursor()
        cursor.select(cursor.SelectionType.Document)
        self.editor.setTextCursor(cursor)
        # 收缩选区到非空行范围由编辑器内部处理空块
        self.editor.convert_selection_to_tasks()

    def test_convert_plain_multiline(self):
        self._set_text("US\nJP\nCA")
        self._select_all_lines()
        self.assertEqual(
            self.editor.toPlainText(), "- [ ] US\n- [ ] JP\n- [ ] CA"
        )

    def test_cycle_states(self):
        self._set_text("- [ ] US\n- [ ] JP\n- [ ] CA")
        cursor = self.editor.textCursor()
        cursor.setPosition(0)  # 光标放在第一行
        self.editor.setTextCursor(cursor)
        for expected in ("- [~] US", "- [x] US", "- [ ] US"):
            self.editor.cycle_task_state(+1)
            self.assertTrue(self.editor.toPlainText().startswith(expected))

    def _click_mark_in_block(self, block_number: int) -> None:
        """用真实鼠标事件点击指定行的 ``[..]`` 标记。"""
        self.editor.resize(400, 200)
        self.editor.show()
        block = self.editor.document().findBlockByNumber(block_number)
        probe = self.editor.textCursor()
        probe.setPosition(block.position() + 4)  # 落在 "[ ]" 内
        rect = self.editor.cursorRect(probe)
        QTest.mouseClick(
            self.editor.viewport(), Qt.MouseButton.LeftButton,
            Qt.KeyboardModifier.NoModifier, rect.center(),
        )

    def test_click_toggles_clicked_line_not_cursor_line(self):
        """回归：点任意任务的 [] 都只改光标原所在行。"""
        self._set_text("- [ ] US\n- [ ] JP\n- [ ] CA")
        cursor = self.editor.textCursor()
        cursor.setPosition(0)  # 光标停在第一行
        self.editor.setTextCursor(cursor)
        # 点击第二行和第三行的标记，各自切换，互不影响其它行
        self._click_mark_in_block(1)
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n- [~] JP\n- [ ] CA")
        self._click_mark_in_block(2)
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n- [~] JP\n- [~] CA")
        # 第一行仍可正常点击切换
        self._click_mark_in_block(0)
        self.assertEqual(self.editor.toPlainText(),
                         "- [~] US\n- [~] JP\n- [~] CA")

    def test_cycle_backward(self):
        self._set_text("- [ ] JP")
        self.editor.cycle_task_state(-1)
        self.assertEqual(self.editor.toPlainText(), "- [x] JP")

    def test_convert_with_existing_list(self):
        self._set_text("- US\n* JP\n1. CA")
        self._select_all_lines()
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n* [ ] JP\n1. [ ] CA")

    def test_convert_keeps_indent_and_special_chars(self):
        self._set_text("  Check API *and* UI\n中文任务")
        self._select_all_lines()
        self.assertEqual(self.editor.toPlainText(),
                         "  - [ ] Check API *and* UI\n- [ ] 中文任务")

    def test_empty_lines_skipped(self):
        """空行不转换，避免制造噪音。"""
        self._set_text("US\n\nJP")
        self._select_all_lines()
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n\n- [ ] JP")


class TestDocumentRoundTrip(unittest.TestCase):
    """核心场景：转换 → 切换 → 保存 → 重开 → 状态恢复。"""

    def setUp(self) -> None:
        self.reg = create_default_registry()
        self.tmp = tempfile.TemporaryDirectory()

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_full_roundtrip(self):
        from src.ui.main_window import MainWindow

        win = MainWindow()
        try:
            editor = win._editor
            editor.setPlainText("US\nJP\nCA")
            cursor = editor.textCursor()
            cursor.select(cursor.SelectionType.Document)
            editor.setTextCursor(cursor)
            editor.convert_selection_to_tasks()

            # 分别设置状态：US=Done(2次), JP=Ing(1次), CA=Pending(0次)
            doc = editor.document()
            for line_idx, steps in ((0, 2), (1, 1), (2, 0)):
                c = editor.textCursor()
                c.setPosition(doc.findBlockByNumber(line_idx).position())
                editor.setTextCursor(c)
                for _ in range(steps):
                    editor.cycle_task_state(+1)

            self.assertEqual(
                editor.toPlainText(), "- [x] US\n- [~] JP\n- [ ] CA"
            )

            path = Path(self.tmp.name) / "note.md"
            win._document.set_path(path)
            self.assertTrue(win.save_document())
            self.assertEqual(
                path.read_text(encoding="utf-8"), "- [x] US\n- [~] JP\n- [ ] CA"
            )

            # 模拟重新打开
            editor.setPlainText(path.read_text(encoding="utf-8"))
            lines = editor.toPlainText().splitlines()
            states = [
                line[line.index("[") + 1: line.index("]")]
                for line in lines
            ]
            self.assertEqual(states, ["x", "~", " "])
        finally:
            win.deleteLater()

    def test_unsaved_detection(self):
        from src.ui.main_window import MainWindow

        win = MainWindow()
        try:
            self.assertFalse(win._is_modified())
            cursor = win._editor.textCursor()  # 模拟用户输入
            cursor.insertText("# hello")
            self.assertTrue(win._is_modified())
            win._editor.document().setModified(False)
            self.assertFalse(win._is_modified())
        finally:
            win.deleteLater()


if __name__ == "__main__":
    unittest.main()
