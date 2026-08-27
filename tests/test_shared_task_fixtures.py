"""Python 与 Rust 共用的 Markdown 任务规则契约。"""
from __future__ import annotations

import json
import unittest
from pathlib import Path

from src.document.parser import cycle_task_mark, is_task_line, make_task_line, match_task, set_task_mark
from src.document.task_state import create_default_registry


FIXTURE_PATH = Path(__file__).resolve().parents[1] / "rust-app" / "tests" / "fixtures" / "task_rules.json"


class TestSharedTaskFixtures(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixtures = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        cls.registry = create_default_registry()

    def test_make_task_line(self) -> None:
        for case in self.fixtures["make_task_line"]:
            with self.subTest(case=case):
                self.assertEqual(make_task_line(case["line"], case["mark"]), case["expected"])

    def test_set_task_mark(self) -> None:
        for case in self.fixtures["set_task_mark"]:
            with self.subTest(case=case):
                self.assertEqual(set_task_mark(case["line"], case["mark"]), case["expected"])

    def test_cycle_task_mark(self) -> None:
        for case in self.fixtures["cycle_task_mark"]:
            with self.subTest(case=case):
                self.assertEqual(
                    cycle_task_mark(case["line"], self.registry, case["step"]),
                    (case["expected"], case["changed"]),
                )

    def test_match_task(self) -> None:
        for case in self.fixtures["match_task"]:
            with self.subTest(case=case):
                info = match_task(case["line"])
                self.assertEqual(info.mark if info else None, case["mark"])
                self.assertEqual(info.mark_start if info else None, case["mark_start"])
                self.assertEqual(info.mark_end if info else None, case["mark_end"])
                self.assertEqual(is_task_line(case["line"], self.registry), case["is_known_task"])
