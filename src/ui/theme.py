"""UI 主题：浅色现代风格（参考 VS Code / Obsidian 的简洁设计）。

颜色集中定义，方便以后做深色主题切换。
"""
from __future__ import annotations

# -- 调色板 ----------------------------------------------------------------
BG = "#ffffff"            # 主背景
BG_SIDEBAR = "#f6f7f9"    # 侧栏背景
BG_HOVER = "#eceef1"      # 悬停
BORDER = "#e4e7eb"        # 分隔线
TEXT = "#1f2328"          # 正文
TEXT_MUTED = "#8a919f"    # 次要文字
ACCENT = "#3b82f6"        # 强调色

FONT_FAMILY = '"Segoe UI", "PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif'
MONO_FAMILY = '"Cascadia Code", Consolas, "JetBrains Mono", "Sarasa Mono SC", Menlo, monospace'

BASE_FONT_SIZE = 10.5     # pt
EDITOR_FONT_SIZE = 12.5   # pt


def app_stylesheet() -> str:
    return f"""
* {{
    font-family: {FONT_FAMILY};
    font-size: {BASE_FONT_SIZE}pt;
    color: {TEXT};
}}
QMainWindow, QDialog {{ background: {BG}; }}

/* ---- 工具栏 ---- */
QToolBar {{
    background: {BG};
    border: none;
    border-bottom: 1px solid {BORDER};
    padding: 6px 10px;
    spacing: 4px;
}}
QToolButton {{
    border: none;
    border-radius: 6px;
    padding: 5px 12px;
    background: transparent;
    color: {TEXT};
}}
QToolButton:hover {{ background: {BG_HOVER}; }}
QToolButton:pressed, QToolButton:checked {{ background: {BG_HOVER}; color: {ACCENT}; }}
QToolBar::separator {{ width: 1px; background: {BORDER}; margin: 4px 8px; }}

/* ---- 菜单 ---- */
QMenuBar {{
    background: {BG};
    border-bottom: 1px solid {BORDER};
    padding: 2px 6px;
}}
QMenuBar::item {{ padding: 4px 10px; border-radius: 4px; background: transparent; }}
QMenuBar::item:selected {{ background: {BG_HOVER}; }}
QMenu {{
    background: {BG};
    border: 1px solid {BORDER};
    border-radius: 8px;
    padding: 6px;
}}
QMenu::item {{ padding: 6px 24px 6px 12px; border-radius: 6px; }}
QMenu::item:selected {{ background: {BG_HOVER}; }}
QMenu::separator {{ height: 1px; background: {BORDER}; margin: 5px 8px; }}

/* ---- 侧栏 ---- */
#Sidebar {{ background: {BG_SIDEBAR}; border-right: 1px solid {BORDER}; }}
#SidebarTitle {{
    color: {TEXT_MUTED};
    font-size: 9pt;
    font-weight: 600;
    letter-spacing: 1px;
    padding: 14px 16px 6px 16px;
}}
QListWidget {{
    background: transparent;
    border: none;
    outline: none;
    padding: 0 8px;
}}
QListWidget::item {{
    border-radius: 6px;
    padding: 6px 10px;
    margin: 1px 0;
    color: {TEXT};
}}
QListWidget::item:hover {{ background: {BG_HOVER}; }}
QListWidget::item:selected {{ background: {BG_HOVER}; color: {ACCENT}; }}

/* ---- 编辑器 / 预览 ---- */
QPlainTextEdit, QTextEdit, QTextBrowser {{
    background: {BG};
    border: none;
    selection-background-color: #d3e4fd;
    selection-color: {TEXT};
}}

/* ---- 状态栏 ---- */
QStatusBar {{
    background: {BG_SIDEBAR};
    border-top: 1px solid {BORDER};
    color: {TEXT_MUTED};
    font-size: 9pt;
}}
QStatusBar::item {{ border: none; }}

/* ---- 对话框按钮 ---- */
QPushButton {{
    background: {BG};
    border: 1px solid {BORDER};
    border-radius: 6px;
    padding: 5px 16px;
}}
QPushButton:hover {{ background: {BG_HOVER}; }}
QPushButton:default {{
    background: {ACCENT};
    border-color: {ACCENT};
    color: #ffffff;
}}
QPushButton:default:hover {{ background: #2563eb; }}

/* ---- 分割条 ---- */
QSplitter::handle {{ background: transparent; }}
QSplitter::handle:horizontal {{ width: 1px; background: {BORDER}; }}
"""


def editor_font_style() -> str:
    return f"{MONO_FAMILY}"
