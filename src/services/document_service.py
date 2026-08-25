"""文档文件读写与最近文档服务。

所有磁盘 IO 集中在这里，UI 层不直接接触文件系统。
"""
from __future__ import annotations

from pathlib import Path

from PySide6.QtWidgets import QFileDialog, QWidget

from ..config import settings

MARKDOWN_FILTER = "Markdown (*.md *.markdown);;文本文件 (*.txt);;所有文件 (*)"

# 扫描工作区时跳过的目录名（版本库 / 依赖 / 隐藏目录等）
SKIP_DIRS = {".git", ".hg", ".svn", ".venv", "venv", "node_modules", "__pycache__", ".idea", ".vscode"}
MARKDOWN_SUFFIXES = {".md", ".markdown"}


class DocumentService:
    """封装打开 / 保存 / 另存为 / 最近文档等文档相关操作。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        self._parent = parent

    # -- 磁盘 IO -----------------------------------------------------------
    def read_text(self, path: Path) -> str:
        return Path(path).read_text(encoding="utf-8")

    def write_text(self, path: Path, text: str) -> None:
        Path(path).write_text(text, encoding="utf-8")

    # -- 对话框 ------------------------------------------------------------
    def pick_open_path(self) -> Path | None:
        path, _ = QFileDialog.getOpenFileName(
            self._parent, "打开文档", "", MARKDOWN_FILTER
        )
        return Path(path) if path else None

    def pick_open_folder(self) -> Path | None:
        path = QFileDialog.getExistingDirectory(self._parent, "打开文件夹（工作区）")
        return Path(path) if path else None

    def pick_save_path(self, suggested_name: str = "untitled.md") -> Path | None:
        path, _ = QFileDialog.getSaveFileName(
            self._parent, "保存文档", suggested_name, MARKDOWN_FILTER
        )
        if not path:
            return None
        p = Path(path)
        if p.suffix == "":  # 无后缀时默认 .md
            p = p.with_suffix(".md")
        return p

    # -- 最近文档 ----------------------------------------------------------
    def recent_files(self) -> list[Path]:
        existing = []
        for f in settings.recent_files():
            p = Path(f)
            if p.exists():
                existing.append(p)
            else:
                settings.remove_recent_file(f)
        return existing

    # -- 工作区 ------------------------------------------------------------
    @staticmethod
    def markdown_files_in(folder: Path) -> list[Path]:
        """递归收集文件夹下的 Markdown 文件，按相对路径排序。

        跳过 SKIP_DIRS 中的目录及隐藏目录。
        """
        folder = Path(folder)
        results: list[Path] = []
        stack = [folder]
        while stack:
            current = stack.pop()
            try:
                entries = sorted(current.iterdir(), key=lambda p: p.name.lower())
            except OSError:
                continue
            for entry in entries:
                if entry.is_dir():
                    if entry.name in SKIP_DIRS or entry.name.startswith("."):
                        continue
                    stack.append(entry)
                elif entry.suffix.lower() in MARKDOWN_SUFFIXES:
                    results.append(entry)
        # 按相对路径稳定排序，侧栏展示更可预测
        return sorted(results, key=lambda p: str(p.relative_to(folder)).lower())
