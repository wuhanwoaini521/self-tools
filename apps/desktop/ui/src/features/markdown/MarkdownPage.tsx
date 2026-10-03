import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
// `open` 与 DOM 全局的 `window.open` 同名会撞车，别名导入避免误用。
import { open as openNativeDialogApi, save } from "@tauri-apps/plugin-dialog";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { devtoolboxMarkdown } from "../../markdown-decorations";
import {
  CaretDown, CaretRight, Check, CheckCircle, CheckSquare, Circle, CloudArrowUp, Code,
  DotsThree, FloppyDisk, FolderOpen, ListChecks, MagnifyingGlass, Plus,
  SidebarSimple, SplitHorizontal, Target, X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { AppSettings, DocumentDto, WorkspaceFile } from "../../types";
import { errorMessage, fileName, isTauriRuntime } from "../../utils";
import { useLayout } from "../../layout";
import { workspaceClient } from "../../workspaceClient";
import { markdownClient } from "./markdownClient";
import { matchesMarkdownShortcut, shortcutLabel } from "./shortcuts";

/** ============================================================
 * Markdown Feature：编辑、工作区文件树、任务大纲、快捷键。
 * 从单文件应用阶段整体迁入,行为保持不变;仅品牌区上移到 App 外壳。
 * ============================================================ */

type TaskStatus = "todo" | "progress" | "done";
type TaskFilter = "all" | TaskStatus;
type OutlineTask = { line: number; text: string; status: TaskStatus };
type FileTreeNode = { name: string; path: string; isFile: boolean; children: FileTreeNode[] };
type OutlineHeading = { line: number; level: number; number: string; text: string };

/** 外壳下发的跨 Feature 意图(如 Home 页「打开笔记」)。 */
export type MarkdownIntent = { type: "open" | "new"; path?: string; nonce: number };

function taskStatus(marker: string): TaskStatus { return marker.toLowerCase() === "x" ? "done" : marker === "~" ? "progress" : "todo"; }

function clampWidth(value: number, min: number, max: number) { return Math.min(max, Math.max(min, value)); }

/** 把工作区扫描结果(relative_path 含目录层级)构建为文件夹优先排序的文件树 */
function buildFileTree(files: WorkspaceFile[]): FileTreeNode[] {
  const root: FileTreeNode = { name: "", path: "", isFile: false, children: [] };
  for (const file of files) {
    const segments = file.relative_path.replaceAll("\\", "/").split("/").filter(Boolean);
    let current = root;
    segments.forEach((segment, index) => {
      const isFile = index === segments.length - 1;
      let next = current.children.find((child) => child.name === segment && child.isFile === isFile);
      if (!next) {
        next = { name: segment, path: isFile ? file.path : (current.path ? current.path + "/" : "") + segment, isFile, children: [] };
        current.children.push(next);
      }
      current = next;
    });
  }
  const sortNodes = (nodes: FileTreeNode[]) => {
    nodes.sort((a, b) => (a.isFile === b.isFile ? a.name.localeCompare(b.name) : a.isFile ? 1 : -1));
    for (const node of nodes) if (!node.isFile) sortNodes(node.children);
  };
  sortNodes(root.children);
  return root.children;
}

/** 解析当前文档的 ATX 标题(带层级编号),供侧栏 Outline 点击跳转 */
function outlineHeadings(text: string): OutlineHeading[] {
  const counters = new Array<number>(7).fill(0);
  return text.split(/\r?\n/).flatMap((line, index) => {
    const match = /^ {0,3}(#{1,6})\s+(.*)$/.exec(line);
    if (!match) return [];
    const level = match[1].length;
    counters[level] += 1;
    for (let deeper = level + 1; deeper <= 6; deeper++) counters[deeper] = 0;
    const number = counters.slice(1, level + 1).filter((count) => count > 0).join(".");
    return [{ line: index, level, number, text: match[2].trim() || "Untitled" }];
  });
}

function WorkspaceTree({ files, path, workspaceName, expanded, onToggleFolder, onOpen, onChooseWorkspace, headings, onJump }: { files: WorkspaceFile[]; path: string | null; workspaceName: string; expanded: Set<string>; onToggleFolder: (folderPath: string) => void; onOpen: (filePath: string) => void; onChooseWorkspace: () => void; headings: OutlineHeading[]; onJump: (line: number) => void }) {
  const tree = useMemo(() => buildFileTree(files), [files]);
  const rows: { node: FileTreeNode; depth: number }[] = [];
  const walk = (nodes: FileTreeNode[], depth: number) => { for (const node of nodes) { rows.push({ node, depth }); if (!node.isFile && expanded.has(node.path)) walk(node.children, depth + 1); } };
  walk(tree, 0);
  return <aside className="command-sidebar">
    <header className="project-heading"><span>Project</span><button title="项目选项"><DotsThree size={19} /></button></header>
    <button className="project-name" onClick={onChooseWorkspace} title="打开其他文件夹">{workspaceName}</button>
    <div className="tree-scroll">
      {rows.length === 0
        ? <div className="tree-empty"><p>当前没有打开的文件夹。<br />选择一个文件夹后，这里会列出其中所有 Markdown 文件。</p><button className="tree-folder" onClick={onChooseWorkspace}><FolderOpen size={17} />打开文件夹</button></div>
        : rows.map(({ node, depth }) => node.isFile
          ? <button key={node.path} className={"tree-doc" + (path === node.path ? " selected" : "")} style={{ paddingLeft: 9 + depth * 14 }} onClick={() => onOpen(node.path)}><Code size={15} weight="bold" />{node.name}<i /></button>
          : <button key={node.path} className="tree-folder" style={{ paddingLeft: 7 + depth * 14 }} onClick={() => onToggleFolder(node.path)} title={expanded.has(node.path) ? "收起文件夹" : "展开文件夹"}>{expanded.has(node.path) ? <CaretDown size={15} /> : <CaretRight size={15} />}<FolderOpen size={17} />{node.name}</button>)}
    </div>
    <section className="document-outline">
      <p>Outline</p>
      {headings.length === 0 ? <span className="outline-empty">暂无标题</span> : headings.map((heading) => <button key={heading.line} title="跳转到标题" onClick={() => onJump(heading.line)}><small>{heading.number}</small>{heading.text}</button>)}
    </section>
    <footer className="sidebar-footer"><SidebarSimple size={18} />Toggle Sidebar</footer>
  </aside>;
}

function TaskOutline({ fileName, tasks, filter, setFilter, onCycle }: { fileName: string; tasks: OutlineTask[]; filter: TaskFilter; setFilter: (filter: TaskFilter) => void; onCycle: (line: number) => void }) {
  const counts = useMemo(() => ({ all: tasks.length, todo: tasks.filter((task) => task.status === "todo").length, progress: tasks.filter((task) => task.status === "progress").length, done: tasks.filter((task) => task.status === "done").length }), [tasks]);
  const visible = filter === "all" ? tasks : tasks.filter((task) => task.status === filter);
  return <aside className="task-outline">
    <header><div><p>Task Outline</p></div><button title="收起任务大纲"><CaretDown size={17} /></button></header>
    <nav className="task-tabs" aria-label="任务过滤">{([
      { key: "all", label: "All", icon: <ListChecks size={17} weight="bold" /> },
      { key: "todo", label: "Todo", icon: <Circle size={17} /> },
      { key: "progress", label: "In Progress", icon: <Target size={17} weight="fill" /> },
      { key: "done", label: "Done", icon: <CheckSquare size={17} weight="fill" /> },
    ] as const).map((tab) => <button key={tab.key} className={filter === tab.key ? "selected" : ""} onClick={() => setFilter(tab.key)} title={`${tab.label}（${counts[tab.key]}）`} aria-label={`${tab.label}（${counts[tab.key]}）`}><span aria-hidden="true">{tab.icon}</span><b>{counts[tab.key]}</b></button>)}</nav>
    <div className="task-file"><Code size={17} weight="bold" />{fileName}</div>
    <section className="task-list"><div className="task-list-title"><CaretDown size={15} />Tasks <b>{tasks.length}</b></div>{visible.map((task) => <button className={"outline-task " + task.status} key={task.line} onClick={() => onCycle(task.line)}>{task.status === "done" ? <CheckSquare size={19} weight="fill" /> : task.status === "progress" ? <Target size={20} weight="bold" /> : <Circle size={19} />}<span>{task.text}</span></button>)}</section>
    <footer><kbd>⌘\\</kbd> Toggle Task Outline <kbd>↵</kbd> Toggle Status</footer>
  </aside>;
}

export function MarkdownPage({ settings, onSettingsChange, setNotice, active, intent, initialWorkspace }: { settings: AppSettings; onSettingsChange: (next: AppSettings) => void; setNotice: (message: string) => void; active: boolean; intent: MarkdownIntent | null; initialWorkspace: string | null | undefined }) {
  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const [text, setText] = useState("");
  const [path, setPath] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<string | null>(null);
  const [workspaceFiles, setWorkspaceFiles] = useState<WorkspaceFile[]>([]);
  const [filter, setFilter] = useState<TaskFilter>("all");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [dirty, setDirty] = useState(false);
  const [focusMode, setFocusMode] = useState(true);
  const [zenMode, setZenMode] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [tasksVisible, setTasksVisible] = useState(true);
  const zenChordDeadline = useRef(0);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());
  const workspaceInitRef = useRef(false);
  // 三栏宽度：侧栏 / 任务大纲可在拖拽句柄上调整(会话内记忆)。
  const [sidebarWidth, setSidebarWidth] = useState(312);
  const [outlineWidth, setOutlineWidth] = useState(373);
  const resizingRef = useRef<{ type: "sidebar" | "outline"; startX: number; startWidth: number } | null>(null);
  /** 正在拖拽的分隔条(驱动 active 高亮,拖动期间保持可见)。 */
  const [resizingBar, setResizingBar] = useState<"sidebar" | "outline" | null>(null);

  const refreshWorkspace = useCallback(async (folder: string | null) => { if (!folder) { setWorkspaceFiles([]); return; } setWorkspaceFiles(await workspaceClient.list(folder)); }, []);

  // 设置加载完成后,一次性初始化工作区(undefined = 设置尚未就绪)。
  useEffect(() => {
    if (initialWorkspace === undefined || workspaceInitRef.current) return;
    workspaceInitRef.current = true;
    setWorkspace(initialWorkspace);
    void refreshWorkspace(initialWorkspace);
  }, [initialWorkspace, refreshWorkspace]);

  const toggleFolder = useCallback((folderPath: string) => {
    setExpandedFolders((previous) => { const next = new Set(previous); if (next.has(folderPath)) next.delete(folderPath); else next.add(folderPath); return next; });
  }, []);

  const startResize = useCallback((type: "sidebar" | "outline") => (event: ReactMouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    resizingRef.current = { type, startX: event.clientX, startWidth: type === "sidebar" ? sidebarWidth : outlineWidth };
    setResizingBar(type);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, [sidebarWidth, outlineWidth]);

  useEffect(() => {
    const onMove = (event: MouseEvent) => {
      const state = resizingRef.current;
      if (!state) return;
      const delta = event.clientX - state.startX;
      if (state.type === "sidebar") setSidebarWidth(clampWidth(state.startWidth + delta, 180, 480));
      else setOutlineWidth(clampWidth(state.startWidth - delta, 200, 560));
    };
    const onUp = () => {
      if (!resizingRef.current) return;
      resizingRef.current = null;
      setResizingBar(null);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  // 栏宽变化后让 CodeMirror 重新测量(隐藏/显示同名机制)。
  useEffect(() => { requestAnimationFrame(() => editorRef.current?.view?.requestMeasure()); }, [sidebarWidth, outlineWidth]);

  /** 页面从隐藏切回可见时,让 CodeMirror 重新测量(保持挂载复用状态)。 */
  useEffect(() => {
    if (!active) return;
    const frame = requestAnimationFrame(() => { editorRef.current?.view?.requestMeasure(); });
    return () => cancelAnimationFrame(frame);
  }, [active]);

  // E2E-only cursor positioning keeps keyboard assertions deterministic while
  // still sending the tested key through CodeMirror's real key handler.
  useEffect(() => {
    if (import.meta.env.VITE_TAURI_E2E !== "1") return;
    window.__DEVTOOLBOX_E2E_SET_EDITOR_LINE__ = (text: string) => {
      const editor = editorRef.current?.view;
      if (!editor) return false;
      for (let number = 1; number <= editor.state.doc.lines; number += 1) {
        const line = editor.state.doc.line(number);
        if (!line.text.includes(text)) continue;
        editor.dispatch({ selection: { anchor: line.from }, scrollIntoView: true });
        editor.focus();
        return true;
      }
      return false;
    };
    return () => { delete window.__DEVTOOLBOX_E2E_SET_EDITOR_LINE__; };
  }, []);

  const persist = useCallback(async (target = path, content = text) => {
    if (!target) {
      const selected = await save({ defaultPath: "untitled.md", filters: [{ name: "Markdown", extensions: ["md", "markdown"] }] });
      if (!selected) return;
      setPath(selected);
      await persist(selected, content);
      return;
    }
    if (import.meta.env.VITE_TAURI_E2E === "1" && target === window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__?.path) {
      window.__DEVTOOLBOX_E2E_SAVED_DOCUMENT__ = { path: target, content };
      setDirty(false);
      setNotice("Saved " + target.split(/[\\/]/).pop());
      return;
    }
    try {
      await markdownClient.write(target, content);
      const next = { ...settings, recent_files: [target, ...settings.recent_files.filter((item) => item !== target)].slice(0, 10) };
      onSettingsChange(next);
      setDirty(false); setNotice("Saved " + target.split(/[\\/]/).pop());
    } catch (error) { setNotice(errorMessage(error)); }
  }, [path, settings, text, onSettingsChange, setNotice]);

  const loadPath = useCallback(async (selected: string) => {
    try {
      if (dirty && !window.confirm("当前文档有未保存的修改，仍然打开新文档吗？")) return;
      const document = await markdownClient.read(selected);
      setPath(document.path); setText(document.text); setDirty(false);
    } catch (error) { setNotice(errorMessage(error)); }
  }, [dirty, setNotice]);

  /**
   * 打开原生文件/文件夹对话框。
   *
   * `plugin-dialog` 是 Tauri 原生能力，浏览器里不存在。此前 `open()` 的失败
   * （`window.__TAURI_INTERNALS__` 缺失导致 TypeError）既没有 try/catch 也没有
   * 任何反馈 —— 点「打开文件夹」**毫无反应**，用户无从判断是坏了还是自己操作错了。
   *
   * 这里统一兜住：非桌面运行时直接返回可读原因，由调用方展示。
   */
  const openNativeDialog = async (options: {
    directory: boolean;
    filters?: { name: string; extensions: string[] }[];
  }): Promise<{ path?: string; reason?: string }> => {
    if (!isTauriRuntime()) {
      return { reason: "网页端无法访问本地文件系统" };
    }
    try {
      const selected = await openNativeDialogApi({
        multiple: false,
        directory: options.directory,
        ...(options.filters ? { filters: options.filters } : {}),
      });
      return typeof selected === "string" ? { path: selected } : {};
    } catch (error) {
      return { reason: errorMessage(error) };
    }
  };

  const chooseDocument = async () => {
    const testDocument = import.meta.env.VITE_TAURI_E2E === "1"
      ? window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__
      : undefined;
    if (testDocument !== undefined) {
      if (!testDocument) return;
      setPath(testDocument.path);
      setText(testDocument.content);
      setDirty(false);
      return;
    }
    const { path: selected, reason } = await openNativeDialog({
      directory: false,
      filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }],
    });
    if (reason) {
      setNotice(`${reason}：打开本地文件需要使用桌面应用`);
      return;
    }
    if (selected) await loadPath(selected);
  };
  const chooseWorkspace = async () => {
    const { path: selected, reason } = await openNativeDialog({ directory: true });
    if (reason) {
      setNotice(`${reason}：打开本地文件夹需要使用桌面应用`);
      return;
    }
    if (!selected) return;
    await refreshWorkspace(selected);
    setWorkspace(selected);
    const next = { ...settings, workspace_path: selected };
    onSettingsChange(next);
    setNotice("已切换工作区");
  };
  const newDocument = () => { setPath(null); setText(""); setDirty(false); };
  /** 关闭当前标签：回到未命名空文档，未保存的修改先确认。 */
  const closeDocument = () => {
    if (dirty && !window.confirm("当前文档有未保存的修改，仍然关闭吗？")) return;
    setPath(null); setText(""); setDirty(false);
  };
  const setContent = (next: string) => { setText(next); setDirty(true); if (settings.auto_save && path) void persist(path, next); };
  const tasks = useMemo<OutlineTask[]>(() => text.split(/\r?\n/).flatMap((line, index) => { const match = /^\s*(?:(?:[-*+]|\d+[.)])\s+)?\[([ ~x])\]\s*(.*)$/.exec(line); return match ? [{ line: index, text: match[2] || "Untitled task", status: taskStatus(match[1]) }] : []; }), [text]);
  const headings = useMemo(() => outlineHeadings(text), [text]);

  /** 侧栏 Outline 点击跳转：定位到标题行并滚动到可见区 */
  const jumpToLine = useCallback((lineNumber: number) => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const target = editor.state.doc.line(lineNumber + 1);
    editor.dispatch({ selection: { anchor: target.from }, scrollIntoView: true });
    editor.focus();
  }, []);

  const cycleTask = async (lineNumber: number) => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const source = editor.state.doc.toString(); const lines = source.split("\n"); const line = lines[lineNumber];
    if (line === undefined) return;
    try {
      const e2eDocument = import.meta.env.VITE_TAURI_E2E === "1"
        ? window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__
        : undefined;
      const result = e2eDocument && path === e2eDocument.path
        ? [line.replace(/\[([ ~x])\]/, (_match, state: string) => `[${state === " " ? "~" : state === "~" ? "x" : " "}]`)]
        : await markdownClient.cycleTaskLines([line], 1);
      const from = lines.slice(0, lineNumber).reduce((offset, current) => offset + current.length + 1, 0);
      editor.dispatch({ changes: { from, to: from + line.length, insert: result[0] }, selection: { anchor: from }, userEvent: "input.task-cycle" }); editor.focus();
    } catch (error) { setNotice(errorMessage(error)); }
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!active) return;
      if (zenChordDeadline.current > 0) {
        // WebDriver's NULL key releases held modifiers between key chords;
        // it is not a user keystroke and must not cancel the pending chord.
        if (event.key.charCodeAt(0) === 0xe000) return;
        const isZenChordFinish =
          Date.now() <= zenChordDeadline.current &&
          event.key.toLowerCase() === "z" &&
          !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
        zenChordDeadline.current = 0;
        if (isZenChordFinish) {
          event.preventDefault();
          setZenMode((value) => !value);
          return;
        }
      }
      if (matchesMarkdownShortcut("toggleZenMode", event)) {
        event.preventDefault();
        zenChordDeadline.current = Date.now() + 1_200;
        return;
      }
      if (matchesMarkdownShortcut("save", event)) { event.preventDefault(); void persist(); }
      else if (matchesMarkdownShortcut("commandPalette", event)) { event.preventDefault(); setPaletteOpen(true); }
      else if (matchesMarkdownShortcut("toggleSidebar", event)) { event.preventDefault(); setSidebarVisible((value) => !value); }
      else if (matchesMarkdownShortcut("toggleTaskOutline", event)) { event.preventDefault(); setTasksVisible((value) => !value); }
      else if (matchesMarkdownShortcut("toggleFocusMode", event)) { event.preventDefault(); setFocusMode((value) => !value); }
      else if (matchesMarkdownShortcut("cycleTask", event)) {
        event.preventDefault();
        const editor = editorRef.current?.view;
        const line = editor?.state.doc.lineAt(editor.state.selection.main.from).number;
        if (line) void cycleTask(line - 1);
      }
      else if (event.key === "Escape") {
        zenChordDeadline.current = 0;
        setZenMode(false);
        setPaletteOpen(false);
      }
    };
    // Global editor shortcuts must work even when CodeMirror or a toolbar
    // control stops the bubbling phase.
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [persist, active]);

  /** 外壳意图:Home 页打开笔记 / 新建笔记 */
  useEffect(() => {
    if (!intent) return;
    if (intent.type === "open" && intent.path) void loadPath(intent.path);
    if (intent.type === "new") newDocument();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [intent?.nonce]);

  /** 工作区加载后默认展开顶层文件夹 */
  useEffect(() => {
    if (!workspaceFiles.length) return;
    const rootFolders = new Set(workspaceFiles.map((file) => file.relative_path.replaceAll("\\", "/").split("/")[0]));
    setExpandedFolders((previous) => { const next = new Set(previous); for (const folder of rootFolders) next.add(folder); return next; });
  }, [workspaceFiles]);

  /** 打开文件时自动展开其所在目录链 */
  useEffect(() => {
    if (!path || !workspace) return;
    const file = workspaceFiles.find((item) => item.path === path);
    if (!file) return;
    const segments = file.relative_path.replaceAll("\\", "/").split("/").slice(0, -1);
    setExpandedFolders((previous) => { const next = new Set(previous); let accumulated = ""; for (const segment of segments) { accumulated = accumulated ? accumulated + "/" + segment : segment; next.add(accumulated); } return next; });
  }, [path, workspace, workspaceFiles]);

  const documentTitle = path?.split(/[\\/]/).pop() ?? "untitled.md";
  const layout = useLayout();
  const classes = ["focus-shell", "markdown-page", focusMode ? "focus-mode" : "", zenMode ? "zen-mode" : "", !sidebarVisible ? "sidebar-hidden" : "", !tasksVisible ? "tasks-hidden" : ""].filter(Boolean).join(" ");
  // 三栏宽度全部由状态驱动:Zen 或全隐藏时回退到单列撑满整宽,
  // 其余情况按可见列给出显式 grid-template-columns,编辑器永远为 minmax(0,1fr)。
  // 窄屏(≤900px)放不下 312+373 的两侧栏 → 强制单列,否则编辑器被挤成 0 宽、
  // 标签条溢出视口。
  const narrow = layout.device !== "desktop";
  const workbenchStyle: CSSProperties = zenMode || narrow
    ? { gridTemplateColumns: "minmax(0, 1fr)" }
    : sidebarVisible
      ? tasksVisible
        ? { gridTemplateColumns: `${sidebarWidth}px minmax(0, 1fr) ${outlineWidth}px` }
        : { gridTemplateColumns: `${sidebarWidth}px minmax(0, 1fr)` }
      : tasksVisible
        ? { gridTemplateColumns: `minmax(0, 1fr) ${outlineWidth}px` }
        : { gridTemplateColumns: "minmax(0, 1fr)" };
  return <main className={classes}>
    <header className="command-bar">
      <div className="command-actions">
        <button onClick={newDocument}><Plus size={18} />New</button><button onClick={() => void chooseDocument()}><FolderOpen size={18} />Open</button><button onClick={() => void chooseWorkspace()} title="打开文件夹（选择工作区）"><FolderOpen size={18} />Folder</button><button onClick={() => void persist()}><FloppyDisk size={17} />Save</button><button onClick={() => setPaletteOpen(true)}><MagnifyingGlass size={18} />Find</button>
        <button onClick={() => setFilter((value) => value === "all" ? "todo" : "all")}><CheckCircle size={18} />Tasks: {filter === "all" ? "All" : "Todo"}<CaretDown size={15} /></button><button onClick={() => void persist()}><CloudArrowUp size={18} />Sync</button><i className="sync-dot" />
      </div>
      <div className="view-actions"><button className={focusMode ? "active" : ""} onClick={() => setFocusMode((value) => !value)}><Target size={18} />Focus Mode <kbd>{shortcutLabel("toggleFocusMode")}</kbd></button><button className={zenMode ? "active" : ""} onClick={() => setZenMode((value) => !value)}><Code size={18} />Zen Mode <kbd>{shortcutLabel("toggleZenMode")}</kbd></button><button className={tasksVisible ? "active" : ""} onClick={() => setTasksVisible((value) => !value)}><SplitHorizontal size={18} />Split</button></div>
    </header>
    <section className="focus-workbench" style={workbenchStyle}>
      {sidebarVisible ? <WorkspaceTree files={workspaceFiles} path={path} workspaceName={workspace ? fileName(workspace) : "打开文件夹…"} expanded={expandedFolders} onToggleFolder={toggleFolder} onOpen={(filePath) => void loadPath(filePath)} onChooseWorkspace={() => void chooseWorkspace()} headings={headings} onJump={jumpToLine} /> : null}
      {!zenMode && sidebarVisible ? <div className={"wb-resizer" + (resizingBar === "sidebar" ? " active" : "")} style={{ left: sidebarWidth - 3 }} title="拖动调整宽度，双击还原" onMouseDown={startResize("sidebar")} onDoubleClick={() => setSidebarWidth(312)} /> : null}
      <section className="editor-workbench">
        <div className="editor-tabs"><button className="editor-tab active"><Code size={17} weight="bold" />{documentTitle}<span title="关闭文档"><X size={15} onClick={(event) => { event.stopPropagation(); closeDocument(); }} /></span></button><button className="new-tab" aria-label="新建标签" title="新建标签" onClick={newDocument}><Plus size={17} /></button></div>
        <header className="editor-meta"><div><span>{workspace ? fileName(workspace) : "docs"}</span><CaretRight size={14} /><Code size={15} weight="bold" /><strong>{documentTitle}</strong></div><div><span>{text.trim().split(/\s+/).filter(Boolean).length.toLocaleString()} words</span><i /><span>{dirty ? "Unsaved" : "Live"} <b /></span><button title="More editor actions"><DotsThree size={20} /></button></div></header>
        <CodeMirror ref={editorRef} className="focus-editor" height="100%" extensions={[markdown(), ...devtoolboxMarkdown()]} value={text} onChange={setContent} basicSetup={{ lineNumbers: true, foldGutter: false, highlightActiveLine: false, highlightActiveLineGutter: false }} indentWithTab aria-label="Focus Mode Markdown editor" />
        <footer className="editor-status"><div><SidebarSimple size={18} />Toggle Sidebar</div><div><span>Ln 1, Col 1</span><span>Spaces: 2</span><span>UTF-8</span><span>LF</span><span>Markdown</span><span><Check size={16} />{tasks.length} tasks</span></div></footer>
      </section>
      {!zenMode && tasksVisible ? <div className={"wb-resizer" + (resizingBar === "outline" ? " active" : "")} style={{ right: outlineWidth - 3 }} title="拖动调整宽度，双击还原" onMouseDown={startResize("outline")} onDoubleClick={() => setOutlineWidth(373)} /> : null}
      {tasksVisible ? <TaskOutline fileName={documentTitle} tasks={tasks} filter={filter} setFilter={setFilter} onCycle={(line) => void cycleTask(line)} /> : null}
    </section>
    {paletteOpen ? <div className="palette-backdrop" onMouseDown={() => setPaletteOpen(false)}><section className="command-palette" role="dialog" aria-modal="true" aria-label="Command palette" onMouseDown={(event) => event.stopPropagation()}><header><MagnifyingGlass size={20} /><input autoFocus placeholder="Find a command…" aria-label="Filter commands" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") setPaletteOpen(false); }} /></header>{["New document", "Open document", "Save document", "Open folder", "Toggle focus mode", "Toggle task outline"].filter((item) => item.toLowerCase().includes(paletteQuery.toLowerCase())).map((item) => <button key={item} onClick={() => { setPaletteOpen(false); if (item === "New document") newDocument(); else if (item === "Open document") void chooseDocument(); else if (item === "Open folder") void chooseWorkspace(); else if (item === "Save document") void persist(); else if (item === "Toggle focus mode") setFocusMode((value) => !value); else if (item === "Toggle task outline") setTasksVisible((value) => !value); }}>{item}</button>)}</section></div> : null}
  </main>;
}
