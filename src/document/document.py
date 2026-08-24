"""当前文档模型：只保存路径等元数据，文本内容留在编辑器中。

保持轻量：第一阶段的「文档」就是磁盘上的一个 Markdown 文件。
"""
from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QObject, Signal

UNTITLED_NAME = "未命名"


class Document(QObject):
    """描述当前打开的文档（路径 + 是否已存在于磁盘）。"""

    pathChanged = Signal(object)  # Path | None

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._path: Path | None = None

    # -- 属性 -------------------------------------------------------------
    @property
    def path(self) -> Path | None:
        return self._path

    @property
    def display_name(self) -> str:
        return self._path.name if self._path else UNTITLED_NAME

    @property
    def exists_on_disk(self) -> bool:
        return self._path is not None and self._path.exists()

    # -- 操作 -------------------------------------------------------------
    def set_path(self, path: Path | None) -> None:
        if path != self._path:
            self._path = path
            self.pathChanged.emit(path)

    def reset(self) -> None:
        self.set_path(None)
