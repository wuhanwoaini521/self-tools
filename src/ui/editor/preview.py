"""简易 Markdown 预览面板。

第一阶段策略：把任务标记替换为状态符号（○ ◐ ●）后，
优先用 mistune（C 加速，大文档不卡）渲染；未安装时回退到 Qt 内置
Markdown 渲染。保持纯展示，不做所见即所得。
"""
from __future__ import annotations

import threading

from PySide6.QtCore import Signal
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QTextBrowser

from ...document.parser import match_task
from ...document.task_state import TaskStateRegistry

try:  # 可选依赖：有则渲染质量更好更快，无则回退 Qt 内置
    import mistune as _mistune  # type: ignore
except ImportError:
    _mistune = None

# 预创建渲染器（进程内复用，避免每次重建解析器）
_RENDER = _mistune.create_markdown(plugins=["table", "strikethrough"]) if _mistune else None


def _replace_marks_with_symbols(
    text: str,
    registry: TaskStateRegistry,
    rich_html: bool,
) -> str:
    """把 ``- [x] foo`` 替换为 ``- ● foo``（HTML 模式下带颜色）。"""
    out = []
    for line in text.splitlines():
        info = match_task(line)
        state = registry.by_mark(info.mark) if info else None
        if info is not None and state is not None:
            if rich_html:
                symbol = (
                    f'<strong><span style="color:{state.color}">'
                    f"{state.symbol}</span></strong>"
                )
            else:
                symbol = state.symbol
            line = f"{line[:info.mark_start]}{symbol}{line[info.mark_end:]}"
        out.append(line)
    return "\n".join(out)


_PREVIEW_STYLE = (
    "<style>"
    "body{font-family:'Segoe UI','PingFang SC','Microsoft YaHei',sans-serif;"
    "font-size:14px;line-height:1.65;color:#1f2328;max-width:760px;}"
    "h1,h2,h3{color:#111827;margin:0.8em 0 0.4em;}"
    "code{background:#f3f4f6;border-radius:4px;padding:1px 5px;"
    "font-family:Consolas,Menlo,monospace;font-size:13px;}"
    "pre{background:#f6f7f9;border-radius:8px;padding:12px;overflow-x:auto;}"
    "pre code{background:none;padding:0;}"
    "blockquote{border-left:3px solid #d1d5db;margin-left:0;"
    "padding-left:12px;color:#8a919f;}"
    "a{color:#3b82f6;}"
    "</style>"
)


class PreviewPane(QTextBrowser):
    # 后台渲染完成后投递回主线程：(代数, html 或 None 表示走 Qt 内置回退)
    _htmlReady = Signal(int, object)

    def __init__(self, registry: TaskStateRegistry, parent=None) -> None:
        super().__init__(parent)
        self._registry = registry
        self._generation = 0          # 每次请求递增，丢弃过期渲染结果
        self._htmlReady.connect(self._apply_html)
        self.setOpenExternalLinks(True)
        font = QFont()
        font.setFamilies(["Segoe UI", "PingFang SC", "Microsoft YaHei",
                          "Noto Sans CJK SC", "sans-serif"])
        font.setPointSize(11)
        self.setFont(font)
        self.document().setDocumentMargin(24)

    def render_markdown(self, text: str) -> None:
        """后台线程渲染，主线程仅 setHtml，大文档不阻塞 UI。"""
        if not _RENDER:
            # 无 mistune 时成本可忽略，直接同步走 Qt 内置渲染
            md_text = _replace_marks_with_symbols(text, self._registry, rich_html=False)
            self.setMarkdown(md_text)
            return
        self._generation += 1
        gen = self._generation
        registry = self._registry

        def work() -> None:
            md_text = _replace_marks_with_symbols(text, registry, rich_html=True)
            html = _PREVIEW_STYLE + _RENDER(md_text)
            self._htmlReady.emit(gen, html)

        threading.Thread(target=work, daemon=True, name="preview-render").start()

    def _apply_html(self, generation: int, html: object) -> None:
        if generation != self._generation:
            return  # 已有更新的渲染请求，丢弃过期结果
        self.setHtml(html)  # type: ignore[arg-type]
