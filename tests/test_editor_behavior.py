"""Undo / Redo 与编辑行为边界测试。

确保多状态任务操作不会破坏正常文本编辑：
* 转换 / 切换各为一步可撤销操作；
* 撤销后文本完整恢复（含换行）；
* 复制粘贴等普通编辑不受影响。
"""
from __future__ import annotations

import os
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication  # noqa: E402

from src.document.task_state import create_default_registry  # noqa: E402
from src.ui.editor.markdown_editor import MarkdownEditor  # noqa: E402

app = QApplication.instance() or QApplication([])


class TestUndoRedo(unittest.TestCase):
    def setUp(self) -> None:
        self.editor = MarkdownEditor(create_default_registry())

    def _select_all(self) -> None:
        cursor = self.editor.textCursor()
        cursor.select(cursor.SelectionType.Document)
        self.editor.setTextCursor(cursor)

    def test_convert_is_single_undo_step(self):
        self.editor.setPlainText("US\nJP\nCA")
        self._select_all()
        self.editor.convert_selection_to_tasks()
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n- [ ] JP\n- [ ] CA")
        self.editor.undo()
        self.assertEqual(self.editor.toPlainText(), "US\nJP\nCA")
        self.editor.redo()
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n- [ ] JP\n- [ ] CA")

    def test_cycle_is_single_undo_step(self):
        self.editor.setPlainText("- [ ] US")
        self.editor.cycle_task_state(+1)
        self.assertEqual(self.editor.toPlainText(), "- [~] US")
        self.editor.undo()
        self.assertEqual(self.editor.toPlainText(), "- [ ] US")

    def test_undo_restores_newlines(self):
        """回归：块替换曾丢失换行符。"""
        self.editor.setPlainText("A\nB\nC\nD")
        self._select_all()
        self.editor.convert_selection_to_tasks()
        self.editor.undo()
        self.assertEqual(self.editor.toPlainText(), "A\nB\nC\nD")

    def test_normal_editing_unaffected(self):
        self.editor.setPlainText("- [ ] US")
        cursor = self.editor.textCursor()
        cursor.movePosition(cursor.MoveOperation.End)
        cursor.insertText(" extra")
        self.editor.setTextCursor(cursor)
        self.assertEqual(self.editor.toPlainText(), "- [ ] US extra")

    def test_paste_plain_text(self):
        self.editor.setPlainText("- [x] JP")
        cursor = self.editor.textCursor()
        cursor.select(cursor.SelectionType.Document)
        cursor.insertText("CA")  # 粘贴覆盖
        self.assertEqual(self.editor.toPlainText(), "CA")

    def test_nested_list_conversion(self):
        self.editor.setPlainText("- US\n  - JP\n- CA")
        self._select_all()
        self.editor.convert_selection_to_tasks()
        self.assertEqual(self.editor.toPlainText(),
                         "- [ ] US\n  - [ ] JP\n- [ ] CA")

    def test_enter_continues_task_list(self):
        self.editor.setPlainText("- [ ] US")
        cursor = self.editor.textCursor()
        cursor.movePosition(cursor.MoveOperation.End)
        self.editor.setTextCursor(cursor)
        self.editor.keyPressEvent(_return_event())
        self.assertEqual(self.editor.toPlainText(), "- [ ] US\n- [ ] ")
        self.assertEqual(self.editor.textCursor().blockNumber(), 1)

    def test_enter_on_plain_text_unchanged(self):
        self.editor.setPlainText("hello")
        cursor = self.editor.textCursor()
        cursor.movePosition(cursor.MoveOperation.End)
        self.editor.setTextCursor(cursor)
        self.editor.keyPressEvent(_return_event())
        self.assertEqual(self.editor.toPlainText(), "hello\n")


def _return_event():
    from PySide6.QtCore import Qt, QEvent
    from PySide6.QtGui import QKeyEvent
    return QKeyEvent(
        QEvent.Type.KeyPress, Qt.Key.Key_Return,
        Qt.KeyboardModifier.NoModifier,
    )


if __name__ == "__main__":
    unittest.main()