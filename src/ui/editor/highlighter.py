"""Markdown 语法高亮。

覆盖：标题、粗体、斜体、行内代码、围栏代码块、引用、链接、
分隔线，以及多状态任务标记（按状态着色）。
"""
from __future__ import annotations

import re

from PySide6.QtCore import Qt, QRegularExpression
from PySide6.QtGui import (
    QColor,
    QFont,
    QSyntaxHighlighter,
)
from PySide6.QtGui import QTextCharFormat

from ...document.task_state import TaskStateRegistry
from .code_highlight import resolve_lang

_FENCE_RE = re.compile(r"^\s*(```|~~~)\s*([A-Za-z0-9_+#-]*)")
_HEADING_RE = QRegularExpression(r"^#{1,6}\s+.*$")
_BOLD_RE = QRegularExpression(r"\*\*[^*\n]+\*\*|__[^_\n]+__")
_ITALIC_RE = QRegularExpression(r"(?<!\*)\*[^*\n]+\*(?!\*)|(^|\W)_[^_\n]+_(?=\W|$)")
_CODE_RE = QRegularExpression(r"`[^`\n]+`")
_STRIKE_RE = QRegularExpression(r"~~[^~\n]+~~")
_LINK_RE = QRegularExpression(r"\[[^\]\n]*\]\([^)\n]+\)|<https?://[^>\n]+>")
_QUOTE_RE = QRegularExpression(r"^\s*>+\s?.*$")
_HR_RE = QRegularExpression(r"^\s{0,3}(?:-{3,}|\*{3,}|_{3,})\s*$")

# 围栏代码块状态标记：bit0 表示是否在代码块内，高位字节存语言索引，
# 以便逐块着色时知道当前代码块的语言。
STATE_NORMAL = 0
STATE_IN_CODE = 0x01
_LANG_MAX = 0xFF


def _encode_state(in_code: bool, lang_index: int) -> int:
    return (STATE_IN_CODE if in_code else 0) | ((lang_index & _LANG_MAX) << 8)

# 标题按级别的颜色（H1→H6）
_HEADING_COLORS = {
    1: "#dc2626",  # 红
    2: "#ea580c",  # 橙
    3: "#16a34a",  # 绿
    4: "#2563eb",  # 蓝
    5: "#7c3aed",  # 紫
    6: "#0891b2",  # 青
}


def _lang_index(info: str) -> int:
    return _LANG_IDS.setdefault(resolve_lang(info).name, len(_LANG_IDS))


_LANG_IDS: dict[str, int] = {}


class MarkdownHighlighter(QSyntaxHighlighter):
    def __init__(self, document, registry: TaskStateRegistry) -> None:
        super().__init__(document)
        self._registry = registry

    # ------------------------------------------------------------------
    def highlightBlock(self, text: str) -> None:
        prev = max(self.previousBlockState(), 0)
        in_code = bool(prev & STATE_IN_CODE)
        lang_index = (prev >> 8) & _LANG_MAX
        fence_match = _FENCE_RE.match(text)

        if fence_match:
            self.setFormat(0, len(text), self._code_block_format())
            if in_code:
                self.setCurrentBlockState(STATE_NORMAL)  # 关闭围栏
            else:
                idx = _lang_index(fence_match.group(2))
                self.setCurrentBlockState(_encode_state(True, idx))
            return

        if in_code:
            self.setCurrentBlockState(_encode_state(True, lang_index))
            fmt = self._code_block_format()
            self.setFormat(0, len(text), fmt)
            spec = self._spec_for(lang_index)

            def apply(start: int, length: int, color: str) -> None:
                token_fmt = QTextCharFormat(fmt)
                token_fmt.setForeground(QColor(color))
                self.setFormat(start, length, token_fmt)

            from .code_highlight import highlight_code
            highlight_code(text, spec, set_format=apply)
            return

        self.setCurrentBlockState(STATE_NORMAL)
        self._highlight_headings(text)
        self._highlight_quote(text)
        self._highlight_hr(text)
        self._highlight_inline(text)
        self._highlight_task_marks(text)

    @staticmethod
    def _spec_for(lang_index: int):
        from .code_highlight import _LANGS, _GENERIC
        for name, idx in _LANG_IDS.items():
            if idx == lang_index:
                return _LANGS.get(name, _GENERIC)
        return _GENERIC

    # -- 块级 --------------------------------------------------------------
    def _highlight_headings(self, text: str) -> None:
        m = _HEADING_RE.match(text)
        if not m.hasMatch():
            return
        level = min(len(text) - len(text.lstrip("#")), 6)
        fmt = QTextCharFormat()
        fmt.setFontWeight(QFont.Weight.Bold)
        base = self.document().defaultFont().pointSizeF() or 12.0
        factor = {1: 1.5, 2: 1.32, 3: 1.2}.get(level, 1.08)
        fmt.setFontPointSize(base * factor)
        fmt.setForeground(QColor(_HEADING_COLORS[level]))
        self.setFormat(0, len(text), fmt)

    def _highlight_quote(self, text: str) -> None:
        if not _QUOTE_RE.match(text).hasMatch():
            return
        fmt = QTextCharFormat()
        fmt.setForeground(QColor("#8a919f"))
        fmt.setFontItalic(True)
        self.setFormat(0, len(text), fmt)

    def _highlight_hr(self, text: str) -> None:
        if not _HR_RE.match(text).hasMatch():
            return
        fmt = QTextCharFormat()
        fmt.setForeground(QColor("#c3c9d4"))
        self.setFormat(0, len(text), fmt)

    # -- 行内 --------------------------------------------------------------
    def _highlight_inline(self, text: str) -> None:
        for regex, make_fmt in (
            (_CODE_RE, lambda: self._inline_code_format()),
            (_BOLD_RE, lambda: self._bold_format()),
            (_STRIKE_RE, lambda: self._strike_format()),
            (_ITALIC_RE, lambda: self._italic_format()),
            (_LINK_RE, lambda: self._link_format()),
        ):
            it = regex.globalMatch(text)
            while it.hasNext():
                m = it.next()
                self.setFormat(m.capturedStart(), m.capturedLength(), make_fmt())

    # -- 任务标记 -----------------------------------------------------------
    def _highlight_task_marks(self, text: str) -> None:
        from ...document.parser import match_task

        info = match_task(text)
        if info is None:
            return
        state = self._registry.by_mark(info.mark)
        if state is None:
            return
        fmt = QTextCharFormat()
        fmt.setForeground(QColor(state.color))
        fmt.setFontWeight(QFont.Weight.Bold)
        self.setFormat(info.mark_start, info.mark_end - info.mark_start, fmt)

    # -- 格式工厂 -----------------------------------------------------------
    @staticmethod
    def _code_block_format() -> QTextCharFormat:
        fmt = QTextCharFormat()
        fmt.setForeground(QColor("#57606a"))
        fmt.setBackground(QColor("#f0f2f5"))  # 灰色背景区分代码区
        fmt.setFontFamilies(
            ["Cascadia Code", "Consolas", "JetBrains Mono", "Menlo", "monospace"]
        )
        return fmt

    @staticmethod
    def _inline_code_format() -> QTextCharFormat:
        fmt = QTextCharFormat()
        fmt.setForeground(QColor("#b83a5e"))
        fmt.setBackground(QColor("#f3f4f6"))
        fmt.setFontFamilies(["Consolas", "JetBrains Mono", "Menlo", "monospace"])
        return fmt

    @staticmethod
    def _bold_format() -> QTextCharFormat:
        fmt = QTextCharFormat()
        fmt.setFontWeight(QFont.Weight.Bold)
        fmt.setForeground(QColor("#111827"))
        return fmt

    @staticmethod
    def _italic_format() -> QTextCharFormat:
        fmt = QTextCharFormat()
        fmt.setFontItalic(True)
        return fmt

    @staticmethod
    def _strike_format() -> QTextCharFormat:
        fmt = QTextCharFormat()
        fmt.setFontStrikeOut(True)
        fmt.setForeground(QColor("#8a919f"))
        return fmt

    @staticmethod
    def _link_format() -> QTextCharFormat:
        fmt = QTextCharFormat()
        fmt.setForeground(QColor("#3b82f6"))
        fmt.setFontUnderline(True)
        return fmt
