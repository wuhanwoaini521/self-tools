"""预览面板测试：后台线程渲染、过期结果丢弃、任务符号替换。

运行：QT_QPA_PLATFORM=offscreen uv run pytest tests/test_preview.py -v
"""
from __future__ import annotations

import os
import time
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtWidgets import QApplication  # noqa: E402

from src.document.task_state import create_default_registry  # noqa: E402
from src.ui.editor.preview import PreviewPane  # noqa: E402

app = QApplication.instance() or QApplication([])


def _wait_until(pred, timeout_ms: float = 5000) -> bool:
    """泵事件循环直到条件满足（后台渲染经信号队列回主线程）。"""
    deadline = time.perf_counter() + timeout_ms / 1000
    while time.perf_counter() < deadline:
        app.processEvents()
        if pred():
            return True
        time.sleep(0.01)
    return False


class TestPreviewPane(unittest.TestCase):
    def setUp(self) -> None:
        self.reg = create_default_registry()
        self.pv = PreviewPane(self.reg)

    def test_async_render_produces_html(self):
        self.pv.render_markdown("# 标题\n\n正文段落")
        self.assertTrue(
            _wait_until(lambda: "标题" in self.pv.toPlainText()),
            "后台渲染结果应最终出现在预览中",
        )

    def test_task_symbols_colored(self):
        self.pv.render_markdown("- [x] 已完成\n- [ ] 待办")
        ok = _wait_until(lambda: "●" in self.pv.toPlainText())
        self.assertTrue(ok)
        html = self.pv.toHtml()
        self.assertIn("●", html)
        self.assertNotIn("[x]", html)

    def test_stale_result_discarded(self):
        """连续两次请求后，只有最后一次的结果生效。"""
        self.pv.render_markdown("第一版内容 alpha")
        self.pv.render_markdown("第二版内容 beta")
        self.assertTrue(_wait_until(lambda: "beta" in self.pv.toPlainText()))
        self.assertEqual(self.pv._generation, 2)

    def test_large_doc_does_not_block_main_thread(self):
        big = ("# 章节\n\n" + "文本内容。" * 50 + "\n") * 2000  # ~2MB
        t0 = time.perf_counter()
        self.pv.render_markdown(big)
        blocking = (time.perf_counter() - t0) * 1000
        self.assertLess(
            blocking, 150,
            f"render_markdown 在主线程耗时 {blocking:.0f}ms，应为近似零的后台派发",
        )
        _wait_until(lambda: len(self.pv.toPlainText()) > 0, timeout_ms=30000)


if __name__ == "__main__":
    unittest.main()
