"""应用入口。

职责最小化：初始化 QApplication、应用主题、启动主窗口。
"""
from __future__ import annotations

import sys

from PySide6.QtGui import QFont
from PySide6.QtWidgets import QApplication, QMainWindow

from .config.settings import APP_NAME, ORGANIZATION, restore_window_geometry
from .ui.main_window import MainWindow
from .ui.theme import app_stylesheet


def main() -> int:
    app = QApplication(sys.argv)
    app.setOrganizationName(ORGANIZATION)
    app.setApplicationName(APP_NAME)
    app.setStyle("Fusion")
    app.setStyleSheet(app_stylesheet())

    default_font = QFont()
    default_font.setPointSize(10)
    app.setFont(default_font)

    window = MainWindow()
    geometry = restore_window_geometry()
    if geometry is not None:
        window.restoreGeometry(geometry)
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
