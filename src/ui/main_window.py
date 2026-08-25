"""主窗口：组装工具栏 / 侧栏 / 编辑器 / 预览 / 状态栏。

MainWindow 只负责「装配与协调」：
* 文件 IO 委托 DocumentService；
* 任务状态逻辑在 document 模块；
* 快捷键由 ShortcutManager 集中安装。
"""
from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QAction, QKeySequence
from PySide6.QtWidgets import (
    QApplication,
    QLabel,
    QMainWindow,
    QMessageBox,
    QSizePolicy,
    QSplitter,
    QToolBar,
    QVBoxLayout,
    QWidget,
)

from ..config import settings
from ..document.document import Document
from ..document.task_state import TaskStateRegistry, create_default_registry
from ..services.document_service import DocumentService
from ..shortcuts.shortcut_manager import DEFAULT_SHORTCUTS, ShortcutManager
from .editor.markdown_editor import MarkdownEditor
from .editor.preview import PreviewPane
from .sidebar import Sidebar


class MainWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle(settings.APP_NAME)
        self.resize(1100, 720)

        self._registry: TaskStateRegistry = create_default_registry()
        self._document = Document(self)
        self._service = DocumentService(self)
        # 工作区：启动时恢复上次打开的文件夹
        self._workspace_root: Path | None = None
        restored = settings.workspace_path()
        if restored and Path(restored).is_dir():
            self._workspace_root = Path(restored)
        self._shortcut_manager = ShortcutManager(self)

        self._build_ui()
        self._build_menus()
        self._install_shortcuts()
        self._connect_signals()

        # 预览刷新去抖
        self._preview_timer = QTimer(self)
        self._preview_timer.setSingleShot(True)
        self._preview_timer.setInterval(300)
        self._preview_timer.timeout.connect(self._refresh_preview)
        # 任务进度统计去抖：大文档全量扫描较慢，避免每次按键都算
        self._status_timer = QTimer(self)
        self._status_timer.setSingleShot(True)
        self._status_timer.setInterval(150)
        self._status_timer.timeout.connect(self._update_task_status)

        self._update_title()
        self._refresh_sidebar()

    # ==================================================================
    # UI 构建
    # ==================================================================
    def _build_ui(self) -> None:
        toolbar = QToolBar("Main")
        toolbar.setMovable(False)
        self.addToolBar(toolbar)

        # 编辑动作统一靠右；文件操作只保留在菜单里
        spacer = QWidget()
        spacer.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        toolbar.addWidget(spacer)
        self.act_new = QAction("新建", self)
        self.act_open = QAction("打开", self)
        self.act_open_folder = QAction("打开文件夹", self)
        self.act_save = QAction("保存", self)
        self.act_convert = QAction("转换为任务", self)
        self.act_cycle = QAction("切换状态", self)
        toolbar.addAction(self.act_convert)
        toolbar.addAction(self.act_cycle)
        toolbar.addSeparator()
        self.act_preview = QAction("预览", self)
        self.act_preview.setCheckable(True)
        toolbar.addAction(self.act_preview)

        # 中部：侧栏 | 编辑器(+预览)
        self._sidebar = Sidebar(self)
        self._editor = MarkdownEditor(self._registry, self)
        self._preview = PreviewPane(self._registry, self)
        self._preview.setVisible(False)

        right_splitter = QSplitter(Qt.Orientation.Horizontal, self)
        right_splitter.addWidget(self._editor)
        right_splitter.addWidget(self._preview)
        right_splitter.setStretchFactor(0, 3)
        right_splitter.setStretchFactor(1, 2)

        splitter = QSplitter(Qt.Orientation.Horizontal, self)
        splitter.addWidget(self._sidebar)
        splitter.addWidget(right_splitter)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)
        splitter.setContentsMargins(8, 8, 0, 0)
        # 给侧栏一个初始宽度，用户可拖动分割条自由调整
        splitter.setSizes([220, 880])

        container = QWidget(self)
        layout = QVBoxLayout(container)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(splitter)
        self.setCentralWidget(container)

        # 状态栏
        self._status_path = QLabel("未命名")
        self._status_tasks = QLabel("")
        self.statusBar().addWidget(self._status_path, 1)
        self.statusBar().addPermanentWidget(self._status_tasks)

    def _build_menus(self) -> None:
        menu_file = self.menuBar().addMenu("文件(&F)")
        menu_file.addAction(self.act_new)
        self.act_open.setText("打开(&O)…")
        menu_file.addAction(self.act_open)
        self.act_open_folder.setText("打开文件夹(&D)…")
        menu_file.addAction(self.act_open_folder)
        menu_file.addSeparator()
        menu_file.addAction(self.act_save)
        self._act_save_as = QAction("另存为(&A)", self)
        self._act_save_as.triggered.connect(lambda _: self.save_document_as())
        menu_file.addAction(self._act_save_as)
        menu_file.addSeparator()
        act_quit = QAction("退出(&Q)", self)
        act_quit.setShortcut(QKeySequence.StandardKey.Quit)
        act_quit.triggered.connect(self.close)
        menu_file.addAction(act_quit)

        menu_view = self.menuBar().addMenu("视图(&V)")
        menu_view.addAction(self.act_preview)

        menu_help = self.menuBar().addMenu("帮助(&H)")
        act_about = QAction("关于(&A)", self)
        act_about.triggered.connect(self._show_about)
        menu_help.addAction(act_about)

        # 菜单动作快捷键（集中定义，跨平台 Ctrl→Cmd 自动映射）
        self.act_new.setShortcut(QKeySequence.StandardKey.New)
        self.act_open.setShortcut(QKeySequence.StandardKey.Open)
        self.act_save.setShortcut(QKeySequence.StandardKey.Save)
        self._act_save_as.setShortcut(QKeySequence.StandardKey.SaveAs)
        self.act_open_folder.setShortcut(QKeySequence("Ctrl+Shift+O"))

    def _install_shortcuts(self) -> None:
        self._shortcut_manager.install(
            self._editor,
            {
                "convert_to_task": self._editor.convert_selection_to_tasks,
                "cycle_task_next": lambda: self._editor.cycle_task_state(step=1),
                "cycle_task_prev": lambda: self._editor.cycle_task_state(step=-1),
            },
            DEFAULT_SHORTCUTS,
        )
        # 同步菜单文字上的快捷键提示
        seq_next = self._shortcut_manager.sequence_of("cycle_task_next")
        seq_conv = self._shortcut_manager.sequence_of("convert_to_task")
        self.act_convert.setToolTip(f"转换为任务 ({seq_conv})")
        self.act_convert.setText(f"转换为任务 ({seq_conv})")
        self.act_cycle.setText(f"切换状态 ({seq_next})")

    def _connect_signals(self) -> None:
        self.act_new.triggered.connect(lambda: self.new_document())
        self.act_open.triggered.connect(lambda: self.open_document())
        self.act_open_folder.triggered.connect(lambda: self.open_folder())
        self.act_save.triggered.connect(lambda: self.save_document())
        self.act_convert.triggered.connect(self._editor.convert_selection_to_tasks)
        self.act_cycle.triggered.connect(lambda: self._editor.cycle_task_state(step=1))
        self.act_preview.toggled.connect(self._toggle_preview)

        self._editor.textChanged.connect(self._on_text_changed)
        self._editor.taskStateChanged.connect(self._update_task_status)
        self._document.pathChanged.connect(lambda _p: self._update_title())
        self._document.pathChanged.connect(lambda _p: self._update_status_path())
        self._sidebar.openRequested.connect(lambda p: self.open_document(path=p))

    # ==================================================================
    # 文档操作
    # ==================================================================
    def new_document(self) -> None:
        if not self._confirm_discard_changes():
            return
        self._editor.clear()
        self._document.reset()
        self._editor.document().setModified(False)
        self._after_content_change()

    def open_document(self, path: Path | None = None, *, checked: bool = True) -> bool:
        if path is None:
            if not self._confirm_discard_changes():
                return False
            path = self._service.pick_open_path()
            if path is None:
                return False
        elif not self._confirm_discard_changes():
            return False
        try:
            text = self._service.read_text(path)
        except OSError as exc:
            QMessageBox.warning(self, "打开失败", f"无法读取文件：\n{exc}")
            return False
        self._editor.blockSignals(True)
        self._editor.setPlainText(text)
        self._editor.blockSignals(False)
        self._editor.document().setModified(False)
        self._document.set_path(path)
        settings.push_recent_file(str(path))
        self._refresh_sidebar()
        self._after_content_change()
        return True

    def open_folder(self) -> bool:
        """选择一个文件夹作为工作区，侧栏列出其中所有 Markdown 文件。"""
        folder = self._service.pick_open_folder()
        if folder is None:
            return False
        self._set_workspace(folder)
        return True

    def _set_workspace(self, folder: Path) -> None:
        self._workspace_root = folder
        settings.set_workspace_path(str(folder))
        self._refresh_sidebar()
        self.statusBar().showMessage(f"工作区：{folder}", 3000)

    def save_document(self) -> bool:
        if self._document.path is None:
            return self.save_document_as()
        try:
            self._service.write_text(self._document.path, self._editor.toPlainText())
        except OSError as exc:
            QMessageBox.warning(self, "保存失败", f"无法写入文件：\n{exc}")
            return False
        self._editor.document().setModified(False)
        settings.push_recent_file(str(self._document.path))
        self._refresh_sidebar()
        self._update_title()
        return True

    def save_document_as(self) -> bool:
        suggested = self._document.display_name if self._document.path else "untitled.md"
        path = self._service.pick_save_path(suggested)
        if path is None:
            return False
        old_path = self._document.path
        self._document.set_path(path)
        if not self.save_document():
            self._document.set_path(old_path)
            return False
        return True

    # ==================================================================
    # 内部辅助
    # ==================================================================
    def _is_modified(self) -> bool:
        return self._editor.document().isModified()

    def _confirm_discard_changes(self) -> bool:
        """有未保存修改时弹窗确认；返回 False 表示用户取消操作。"""
        if not self._is_modified():
            return True
        ret = QMessageBox.question(
            self,
            "未保存的修改",
            "当前文档存在未保存内容，是否保存？",
            QMessageBox.StandardButton.Save
            | QMessageBox.StandardButton.Discard
            | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Save,
        )
        if ret == QMessageBox.StandardButton.Save:
            return self.save_document()
        return ret == QMessageBox.StandardButton.Discard

    def _toggle_preview(self, visible: bool) -> None:
        self._preview.setVisible(visible)
        if visible:
            self._refresh_preview()

    def _refresh_preview(self) -> None:
        if self._preview.isVisible():
            self._preview.render_markdown(self._editor.toPlainText())

    def _on_text_changed(self) -> None:
        self._update_title()
        self._status_timer.start()  # 统计防抖，打字不卡
        self._preview_timer.start()

    def _after_content_change(self) -> None:
        self._update_title()
        self._update_status_path()
        self._update_task_status()
        self._refresh_preview()

    def _update_title(self) -> None:
        modified = "*" if self._is_modified() else ""
        self.setWindowTitle(
            f"{self._document.display_name}{modified} — {settings.APP_NAME}"
        )

    def _update_status_path(self) -> None:
        self._status_path.setText(str(self._document.path or "未命名"))

    def _update_task_status(self) -> None:
        from ..document.parser import iter_task_lines

        total = done = 0
        for _, _info, state in iter_task_lines(
            self._editor.toPlainText(), self._registry
        ):
            total += 1
            if state.state_id == "done":
                done += 1
        self._status_tasks.setText(
            f"任务进度 {done}/{total}" if total else ""
        )

    def _refresh_sidebar(self) -> None:
        workspace_files = (
            self._service.markdown_files_in(self._workspace_root)
            if self._workspace_root is not None else []
        )
        self._sidebar.refresh(
            workspace_files,
            self._service.recent_files(),
            current=self._document.path,
            workspace_root=self._workspace_root,
        )

    def _show_about(self) -> None:
        QMessageBox.about(
            self,
            f"关于 {settings.APP_NAME}",
            f"<b>{settings.APP_NAME}</b><br>"
            "Markdown 笔记与多状态任务清单工具（第一阶段）",
        )

    # ==================================================================
    # 关闭保护
    # ==================================================================
    def closeEvent(self, event) -> None:
        if not self._confirm_discard_changes():
            event.ignore()
            return
        geometry = app_geometry(self)
        if geometry is not None:
            settings.save_window_geometry(geometry)
        super().closeEvent(event)


def app_geometry(window: QMainWindow):
    return window.saveGeometry()
