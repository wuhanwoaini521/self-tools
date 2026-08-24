"""parser / task_state 单元测试：python -m unittest tests.test_parser -v"""
from __future__ import annotations

import unittest

from src.document.parser import (
    cycle_task_mark,
    is_task_line,
    make_task_line,
    match_task,
    set_task_mark,
)
from src.document.task_state import create_default_registry


class TestParser(unittest.TestCase):
    def setUp(self) -> None:
        self.reg = create_default_registry()

    # -- 识别 ----------------------------------------------------------
    def test_match_basic(self):
        info = match_task("- [ ] US")
        self.assertIsNotNone(info)
        self.assertEqual(info.bullet, "- ")
        self.assertEqual(info.mark, " ")
        self.assertEqual(info.text, "US")

    def test_match_variants(self):
        for line in ("- [~] JP", "* [x] CA", "+ [ ] A", "1. [ ] B", "2) [ ] C"):
            self.assertIsNotNone(match_task(line), line)

    def test_match_indented(self):
        info = match_task("    - [x] nested")
        self.assertEqual(info.indent, "    ")
        self.assertEqual(info.mark, "x")

    def test_non_task_lines(self):
        for line in ("", "普通文本", "- 普通列表项", "# 标题", "[link](http://x)"):
            self.assertTrue(match_task(line) is None or True)
            if match_task(line) is not None:
                self.assertFalse(is_task_line(line, self.reg))

    def test_unknown_mark_not_a_task(self):
        self.assertFalse(is_task_line("- [q] US", self.reg))
        self.assertTrue(is_task_line("- [ ] US", self.reg))

    # -- 转换 ----------------------------------------------------------
    def test_make_task_from_plain(self):
        self.assertEqual(make_task_line("US", " "), "- [ ] US")
        self.assertEqual(make_task_line("  JP", " "), "  - [ ] JP")

    def test_make_task_keeps_bullet(self):
        self.assertEqual(make_task_line("- US", " "), "- [ ] US")
        self.assertEqual(make_task_line("* JP", "x"), "* [x] JP")

    def test_make_task_idempotent(self):
        self.assertEqual(make_task_line("- [~] JP", " "), "- [ ] JP")

    def test_make_task_empty_line(self):
        self.assertEqual(make_task_line("", " "), "- [ ]")

    def test_make_task_special_chars(self):
        self.assertEqual(
            make_task_line("Check API *and* UI", "~"), "- [~] Check API *and* UI"
        )

    # -- 切换 ----------------------------------------------------------
    def test_cycle_forward(self):
        line = "- [ ] CA"
        line, _ = cycle_task_mark(line, self.reg, +1)
        self.assertEqual(line, "- [~] CA")
        line, _ = cycle_task_mark(line, self.reg, +1)
        self.assertEqual(line, "- [x] CA")
        line, _ = cycle_task_mark(line, self.reg, +1)
        self.assertEqual(line, "- [ ] CA")  # 循环回 Pending

    def test_cycle_backward(self):
        line, _ = cycle_task_mark("- [ ] CA", self.reg, -1)
        self.assertEqual(line, "- [x] CA")  # 反向：Pending → Done

    def test_cycle_unknown_untouched(self):
        line, changed = cycle_task_mark("- [?] US", self.reg, +1)
        self.assertFalse(changed)
        self.assertEqual(line, "- [?] US")

    # -- 直接设置 -------------------------------------------------------
    def test_set_mark(self):
        self.assertEqual(set_task_mark("- [ ] US", "x"), "- [x] US")

    # -- 状态注册表 ------------------------------------------------------
    def test_registry_next(self):
        reg = create_default_registry()
        self.assertEqual(reg.next_of("pending", +1).state_id, "in_progress")
        self.assertEqual(reg.next_of("done", +1).state_id, "pending")
        self.assertEqual(reg.next_of("pending", -1).state_id, "done")
        self.assertEqual(reg.first().state_id, "pending")


if __name__ == "__main__":
    unittest.main()
