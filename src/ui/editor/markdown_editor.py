"""Markdown 源码编辑器。

在 QPlainTextEdit 基础上提供：
* 选中多行 / 当前行 → 转换为多状态任务项（convert_selection_to_tasks）
* 状态循环切换，支持向前 / 向后（cycle_task_state）
* 点击任务标记 ``[ ]`` 区域直接切换状态（mousePressEvent）
* 回车自动延续列表 / 任务前缀
"""
from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QFont, QKeyEvent, QMouseEvent, QTextBlock, QTextCursor
from PySide6.QtWidgets import QPlainTextEdit

from ...document.parser import (
    cycle_task_mark,
    is_task_line,
    make_task_line,
    match_task,
)
from ...document.task_state import TaskStateRegistry
from .highlighter import MarkdownHighlighter


class MarkdownEditor(QPlainTextEdit):
    """以 Markdown 为核心的源码编辑器。"""

    taskStateChanged = Signal()   # 任一任务状态发生变化

    def __init__(self, registry: TaskStateRegistry, parent=None) -> None:
        super().__init__(parent)
        self._registry = registry
        self._setup_font()
        self.setTabStopDistance(4 * self.fontMetrics().horizontalAdvance(" "))
        self.setLineWrapMode(QPlainTextEdit.LineWrapMode.WidgetWidth)
        # 高亮器跟随文档生命周期
        self._highlighter = MarkdownHighlighter(self.document(), registry)

    def _setup_font(self) -> None:
        font = QFont()
        font.setFamilies(
            ["Cascadia Code", "Consolas", "JetBrains Mono", "Menlo",
             "Sarasa Mono SC", "monospace"]
        )
        font.setStyleHint(QFont.StyleHint.Monospace)
        font.setPointSize(12)
        self.setFont(font)

    # ==================================================================
    # 任务操作（供快捷键 / 工具栏调用）
    # ==================================================================
    def _target_blocks(self) -> list[QTextBlock]:
        """当前选中范围覆盖的所有文本块；无选区时为光标所在块。"""
        cursor = self.textCursor()
        if cursor.hasSelection():
            start_block = self.document().findBlock(cursor.selectionStart())
            end_block = self.document().findBlock(max(cursor.selectionEnd(), cursor.selectionStart()))
            blocks = []
            block = start_block
            while block.isValid():
                blocks.append(block)
                if block == end_block:
                    break
                block = block.next()
            return [b for b in blocks if b.isValid() and b.length() > 1]
        block = cursor.block()
        return [block] if block.isValid() and block.length() > 1 else []

    @staticmethod
    def _block_replace(block: QTextBlock, new_text: str) -> None:
        """替换整行内容但保留段落分隔符（换行）。"""
        bc = QTextCursor(block)
        bc.setPosition(block.position())
        bc.setPosition(block.position() + block.length() - 1,
                       QTextCursor.MoveMode.KeepAnchor)
        bc.insertText(new_text)

    def convert_selection_to_tasks(self) -> None:
        """把选中行（或当前行）转换为默认状态的任务项。"""
        blocks = self._target_blocks()
        if not blocks:
            return
        default_mark = self._registry.first().mark
        cursor = self.textCursor()
        cursor.beginEditBlock()
        try:
            for block in blocks:
                new_text = make_task_line(block.text(), default_mark)
                if new_text != block.text():
                    self._block_replace(block, new_text)
        finally:
            cursor.endEditBlock()
            self.setTextCursor(cursor)
        self.taskStateChanged.emit()

    def cycle_task_state(self, step: int = 1) -> None:
        """对选中行 / 当前行中的任务项做状态循环切换。"""
        blocks = self._target_blocks()
        changed = False
        cursor = self.textCursor()
        cursor.beginEditBlock()
        try:
            for block in blocks:
                new_text, did = cycle_task_mark(block.text(), self._registry, step)
                if not did:
                    continue
                changed = True
                self._block_replace(block, new_text)
        finally:
            cursor.endEditBlock()
            self.setTextCursor(cursor)
        if changed:
            self.taskStateChanged.emit()

    # ==================================================================
    # 鼠标点击：点中 [x] 标记区域即切换状态
    # ==================================================================
    def mousePressEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton \
                and self._move_cursor_to_clicked_mark(event):
            self.cycle_task_state(step=1)
            event.accept()
            return
        super().mousePressEvent(event)

    def mouseDoubleClickEvent(self, event: QMouseEvent) -> None:
        if self._move_cursor_to_clicked_mark(event):
            self.cycle_task_state(step=1)
            event.accept()
            return
        super().mouseDoubleClickEvent(event)

    def _move_cursor_to_clicked_mark(self, event: QMouseEvent) -> bool:
        """若点击位置落在任务标记 ``[..]`` 上，把光标移到该行并返回 True。

        必须先移动光标再切换状态：否则 cycle_task_state 会作用在
        光标原所在行，导致点任意任务都只改同一处。
        """
        cursor = self.cursorForPosition(event.position().toPoint())
        block = cursor.block()
        info = match_task(block.text())
        if info is None:
            return False
        if self._registry.by_mark(info.mark) is None:
            return False
        col = cursor.positionInBlock()
        if not (info.mark_start - 1 <= col <= info.mark_end + 1):
            return False
        # 清除选区并把光标定位到点击的块内，后续 cycle 才作用于该行
        cursor.setPosition(cursor.position(),
                           QTextCursor.MoveMode.MoveAnchor)
        self.setTextCursor(cursor)
        return True

    # ==================================================================
    # 键盘：回车自动延续列表 / 任务前缀
    # ==================================================================
    def keyPressEvent(self, event: QKeyEvent) -> None:
        if event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter) \
                and event.modifiers() == Qt.KeyboardModifier.NoModifier:
            continuation = self._continuation_prefix()
            if continuation is not None:
                cur = self.textCursor()
                cur.insertText("\n" + continuation)
                self.ensureCursorVisible()
                event.accept()
                return
        super().keyPressEvent(event)

    def _continuation_prefix(self) -> str | None:
        """回车后应延续的前缀；普通文本行返回 None 走默认行为。"""
        line = self.textCursor().block().text()
        info = match_task(line)
        if info is not None and info.bullet is not None:
            state = self._registry.by_mark(info.mark)
            mark = state.mark if state else self._registry.first().mark
            if not info.text:
                return ""  # 空任务项上回车 → 结束列表
            return f"{info.indent}{info.bullet}[{mark}] "
        if info is not None:  # 无符号的裸任务行不延续
            return None
        from ...document.parser import _LIST_PREFIX_RE
        m = _LIST_PREFIX_RE.match(line)
        if m is not None and m.group("bullet"):
            indent, bullet = m.group("indent"), m.group("bullet")
            if not line[len(indent) + len(bullet):].strip():
                return ""  # 空列表项 → 结束列表
            return f"{indent}{bullet}"
        return None
