"""Markdown 高亮测试：标题分级配色、代码块背景、代码语法着色提示。

运行：QT_QPA_PLATFORM=offscreen uv run pytest tests/test_highlighter.py -v
"""
from __future__ import annotations

import os
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtGui import QColor, QTextCharFormat, QTextDocument  # noqa: E402

from src.document.task_state import create_default_registry  # noqa: E402
from src.ui.editor.code_highlight import (  # noqa: E402
    COLOR_COMMENT,
    COLOR_FUNC,
    COLOR_KEYWORD,
    COLOR_NUMBER,
    COLOR_STRING,
    highlight_code,
)
from src.ui.editor.highlighter import MarkdownHighlighter, _HEADING_COLORS  # noqa: E402


def _formats_at(doc: QTextDocument, block_number: int) -> list:
    block = doc.findBlockByNumber(block_number)
    return block.layout().formats()


def _fg_colors(doc: QTextDocument, line: int) -> set[str]:
    """某行所有片段的前景色集合。"""
    colors = set()
    for fr in _formats_at(doc, line):
        c = fr.format.foreground().color().name()
        colors.add(c)
        # 背景存在时也记录（用于代码块断言）
    return colors


def _bg_colors(doc: QTextDocument, line: int) -> set[str]:
    return {
        fr.format.background().color().name()
        for fr in _formats_at(doc, line)
        if fr.format.hasProperty(QTextCharFormat.Property.BackgroundBrush)
    }


class TestHeadingColors(unittest.TestCase):
    def setUp(self) -> None:
        self.reg = create_default_registry()
        self.doc = QTextDocument()
        self.hl = MarkdownHighlighter(self.doc, self.reg)

    def _highlight(self, text: str) -> QTextDocument:
        self.doc.setPlainText(text)
        self.hl.rehighlight()
        return self.doc

    def test_levels_have_distinct_colors(self):
        text = "\n".join(f"{'#' * i} 标题{i}" for i in range(1, 7))
        doc = self._highlight(text)
        for i in range(6):
            colors = _fg_colors(doc, i)
            expected = _HEADING_COLORS[i + 1]
            self.assertIn(expected, colors, f"H{i + 1} 应为 {expected}")

    def test_all_level_colors_unique(self):
        self.assertEqual(len(set(_HEADING_COLORS.values())), 6)

    def test_hash_marks_colored_too(self):
        doc = self._highlight("### 三级")
        self.assertIn(_HEADING_COLORS[3], _fg_colors(doc, 0))


class TestCodeBlock(unittest.TestCase):
    def setUp(self) -> None:
        self.reg = create_default_registry()
        self.doc = QTextDocument()
        self.hl = MarkdownHighlighter(self.doc, self.reg)

    def _highlight(self, text: str) -> QTextDocument:
        self.doc.setPlainText(text)
        self.hl.rehighlight()
        return self.doc

    def test_code_block_has_gray_background(self):
        doc = self._highlight("```python\nx = 1\n```")
        for line in range(3):
            bgs = _bg_colors(doc, line)
            self.assertTrue(bgs, f"第 {line} 行应有背景色")

    def test_normal_text_has_no_background(self):
        doc = self._highlight("普通文本")
        self.assertEqual(_bg_colors(doc, 0), set())

    def test_python_keywords_strings_comments(self):
        code = 'def foo(n):\n    s = "hi"  # 注释\n'
        doc = self._highlight(f"```python\n{code}```")
        self.assertIn(COLOR_KEYWORD, _fg_colors(doc, 1))   # def
        self.assertIn(COLOR_FUNC, _fg_colors(doc, 1))      # foo(
        self.assertNotIn(COLOR_KEYWORD, _fg_colors(doc, 2))  # 本行无关键字
        self.assertIn(COLOR_STRING, _fg_colors(doc, 2))    # "hi"
        self.assertIn(COLOR_COMMENT, _fg_colors(doc, 2))   # # 注释

    def test_unknown_language_still_basic_hints(self):
        doc = self._highlight('```\n"str" 123\n```')
        self.assertIn(COLOR_STRING, _fg_colors(doc, 1))
        self.assertIn(COLOR_NUMBER, _fg_colors(doc, 1))

    def test_state_restored_after_block(self):
        doc = self._highlight("```python\na = 1\n```\n# 标题")
        # 代码块结束后标题恢复正常高亮，且无代码块背景
        self.assertNotIn(_HEADING_COLORS[1], _fg_colors(doc, 2))  # 围栏行
        self.assertIn(_HEADING_COLORS[1], _fg_colors(doc, 3))
        self.assertEqual(_bg_colors(doc, 3), set())


class TestTokenizerDirect(unittest.TestCase):
    def test_sql_case_insensitive_keywords(self):
        seen: dict[int, str] = {}

        def collect(s: int, e: int, color: str) -> None:
            seen[s] = color

        highlight_code("SELECT id FROM t", __import__(
            "src.ui.editor.code_highlight", fromlist=["resolve_lang"]
        ).resolve_lang("sql"), set_format=collect)
        self.assertEqual(seen.get(0), COLOR_KEYWORD)      # SELECT
        self.assertEqual(seen.get(10), COLOR_KEYWORD)     # FROM

    def test_string_with_escapes(self):
        seen: dict[int, str] = {}

        def collect(s: int, e: int, color: str) -> None:
            seen[s] = color

        highlight_code(r'"a\"b"', __import__(
            "src.ui.editor.code_highlight", fromlist=["resolve_lang"]
        ).resolve_lang("python"), set_format=collect)
        self.assertEqual(seen.get(0), COLOR_STRING)


if __name__ == "__main__":
    unittest.main()
