"""应用级配置与常量。

统一通过 QSettings（QStandardPaths 选定的用户配置目录）读写，
跨平台可用。未来所有设置项都应集中在这里提供存取方法。
"""
from __future__ import annotations

from PySide6.QtCore import QByteArray, QSettings, QStandardPaths

ORGANIZATION = "ox-tools"
APP_NAME = "OxNote"

RECENT_LIMIT = 10


def qsettings() -> QSettings:
    return QSettings(ORGANIZATION, APP_NAME)


def app_config_dir() -> str:
    """用户配置目录（跨平台），用于未来存放日志、插件等。"""
    return QStandardPaths.writableLocation(QStandardPaths.AppConfigLocation)


# -- 最近文档 -------------------------------------------------------------
def recent_files(limit: int = RECENT_LIMIT) -> list[str]:
    files: list[str] = qsettings().value("recent/files", []) or []
    if isinstance(files, str):  # QSettings 单值时可能返回 str
        files = [files]
    return [f for f in files][:limit]


def push_recent_file(path: str) -> None:
    files = recent_files(limit=0)
    files = [f for f in files if f != path]
    files.insert(0, path)
    qsettings().setValue("recent/files", files[:RECENT_LIMIT])


def remove_recent_file(path: str) -> None:
    files = [f for f in recent_files(limit=0) if f != path]
    qsettings().setValue("recent/files", files)


# -- 窗口几何 -------------------------------------------------------------
def save_window_geometry(geometry: QByteArray) -> None:
    qsettings().setValue("window/geometry", geometry)


def restore_window_geometry() -> QByteArray | None:
    value = qsettings().value("window/geometry")
    return value if isinstance(value, QByteArray) else None
