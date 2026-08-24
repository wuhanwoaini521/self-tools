"""简易 Markdown 预览面板。

第一阶段策略：把任务标记替换为状态符号（○ ◐ ●）后，
优先用 python-markdown 渲染；未安装时回退到 Qt 内置
Markdown 渲染。保持纯展示，不做所见即所得。
"""
from __future__ import annotations

from PySide6.QtGui import QFont
from PySide6.QtWidgets import QTextBrowser

from ...document.parser import match_task
from ...document.task_state import TaskStateRegistry

try:  # 可选依赖：有则渲染质量更好，无则回退 Qt 内置
    import markdown as _markdown  # type: ignore
except ImportError:
    _markdown = None


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


class PreviewPane(QTextBrowser):
    def __init__(self, registry: TaskStateRegistry, parent=None) -> None:
        super().__init__(parent)
        self._registry = registry
        self.setOpenExternalLinks(True)
        font = QFont()
        font.setFamilies(["Segoe UI", "PingFang SC", "Microsoft YaHei",
                          "Noto Sans CJK SC", "sans-serif"])
        font.setPointSize(11)
        self.setFont(font)
        self.document().setDocumentMargin(24)

    def render_markdown(self, text: str) -> None:
        md_text = _replace_marks_with_symbols(text, self._registry, rich_html=_markdown is not None)
        if _markdown is not None:
            html = _markdown.markdown(md_text, extensions=["fenced_code", "tables"])
            html = (
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
                "</style>" + html
            )
            self.setHtml(html)
        else:
            self.setMarkdown(md_text)
