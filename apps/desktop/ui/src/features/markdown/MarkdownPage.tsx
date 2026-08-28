import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { devtoolboxMarkdown } from "../../markdown-decorations";
import {
  CaretDown, CaretRight, Check, CheckCircle, CheckSquare, Circle, CloudArrowUp, Code,
  DotsThree, FloppyDisk, FolderOpen, MagnifyingGlass, Plus,
  SidebarSimple, SplitHorizontal, Target, X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { AppSettings, DocumentDto, WorkspaceFile } from "../../types";
import { errorMessage, fileName, isTauriRuntime } from "../../utils";

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

const focusDocument = [
  "# Focus Mode Command Center",
  "",
  "DevToolbox is designed to help you write, organize, and complete Markdown",
  "tasks with clarity and speed.",
  "",
  "## Goals",
  "- Eliminate context switching",
  "- Surface what matters now",
  "- Turn tasks into progress",
  "",
  "## Core Principles",
  "1. Focus on a single document",
  "2. See your tasks. Finish your tasks.",
  "3. Keyboard first. Always.",
  "",
  "---",
  "",
  "## Using Focus Mode",
  "Focus Mode hides distractions and maximizes your writing surface.",
  "Press F11 or click the Focus Mode button in the command bar.",
  "",
  "## Task Workflow",
  "- [x] Define the task and desired outcome",
  "- [x] Break the work into actionable steps",
  "- [~] Implement the core functionality",
  "- [ ] Add examples and docs",
  "- [ ] Test thoroughly",
  "- [ ] Review and refine",
  "",
  "## Keyboard Shortcuts",
  "| Shortcut | Action |",
  "|----------|--------|",
  "| F11 | Toggle Focus Mode |",
  "| Ctrl+B | Toggle Sidebar |",
  "| Ctrl+\\\\ | Toggle Task Outline |",
  "| Ctrl+Enter | Toggle Task Status |",
  "| Ctrl+Shift+F | Find in Document |",
  "",
  "## Tips & Best Practices",
  "- Keep tasks small and verifiable",
  "- Update status as you go",
  "- Use headings to structure your flow",
  "",
  "## Snippets",
  "",
  "Set a task mark directly in the editor:",
  "",
  "```rust",
  "let done = \"- [x] Ship the feature\";",
  "```",
  "",
  "> State cycles: Pending → In Progress → Done.",
  "> Every task stays valid Markdown.",
].join("\n");

function taskStatus(marker: string): TaskStatus { return marker.toLowerCase() === "x" ? "done" : marker === "~" ? "progress" : "todo"; }

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
    <nav className="task-tabs" aria-label="任务过滤">{([{ key: "all", label: "All" }, { key: "todo", label: "Todo" }, { key: "progress", label: "In Progress" }, { key: "done", label: "Done" }] as const).map((tab) => <button key={tab.key} className={filter === tab.key ? "selected" : ""} onClick={() => setFilter(tab.key)}>{tab.label}<b>{counts[tab.key]}</b></button>)}</nav>
    <div className="task-file"><Code size={17} weight="bold" />{fileName}</div>
    <section className="task-list"><div className="task-list-title"><CaretDown size={15} />Tasks <b>{tasks.length}</b></div>{visible.map((task) => <button className={"outline-task " + task.status} key={task.line} onClick={() => onCycle(task.line)}>{task.status === "done" ? <CheckSquare size={19} weight="fill" /> : task.status === "progress" ? <Target size={20} weight="bold" /> : <Circle size={19} />}<span>{task.text}</span><small>{task.status === "done" ? "@done" : task.status === "progress" ? "@in-progress" : "@todo"}</small></button>)}</section>
    <footer><kbd>⌘\\</kbd> Toggle Task Outline <kbd>↵</kbd> Toggle Status</footer>
  </aside>;
}

export function MarkdownPage({ settings, onSettingsChange, setNotice, active, intent, initialWorkspace }: { settings: AppSettings; onSettingsChange: (next: AppSettings) => void; setNotice: (message: string) => void; active: boolean; intent: MarkdownIntent | null; initialWorkspace: string | null | undefined }) {
  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const [text, setText] = useState(focusDocument);
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
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());
  const workspaceInitRef = useRef(false);

  const refreshWorkspace = useCallback(async (folder: string | null) => { if (!folder) { setWorkspaceFiles([]); return; } setWorkspaceFiles(await invoke<WorkspaceFile[]>("list_workspace", { path: folder })); }, []);

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

  /** 页面从隐藏切回可见时,让 CodeMirror 重新测量(保持挂载复用状态)。 */
  useEffect(() => {
    if (!active) return;
    const frame = requestAnimationFrame(() => { editorRef.current?.view?.requestMeasure(); });
    return () => cancelAnimationFrame(frame);
  }, [active]);

  const persist = useCallback(async (target = path, content = text) => {
    if (!target) {
      const selected = await save({ defaultPath: "04-focus-mode.md", filters: [{ name: "Markdown", extensions: ["md", "markdown"] }] });
      if (!selected) return;
      setPath(selected);
      await persist(selected, content);
      return;
    }
    try {
      await invoke("write_document", { path: target, text: content });
      const next = { ...settings, recent_files: [target, ...settings.recent_files.filter((item) => item !== target)].slice(0, 10) };
      onSettingsChange(next);
      setDirty(false); setNotice("Saved " + target.split(/[\\/]/).pop());
    } catch (error) { setNotice(errorMessage(error)); }
  }, [path, settings, text, onSettingsChange, setNotice]);

  const loadPath = useCallback(async (selected: string) => {
    try {
      if (dirty && !window.confirm("当前文档有未保存的修改，仍然打开新文档吗？")) return;
      const document = await invoke<DocumentDto>("read_document", { path: selected });
      setPath(document.path); setText(document.text); setDirty(false);
    } catch (error) { setNotice(errorMessage(error)); }
  }, [dirty, setNotice]);

  const chooseDocument = async () => { const selected = await open({ multiple: false, directory: false, filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }] }); if (typeof selected === "string") await loadPath(selected); };
  const chooseWorkspace = async () => { const selected = await open({ multiple: false, directory: true }); if (typeof selected !== "string") return; await refreshWorkspace(selected); setWorkspace(selected); const next = { ...settings, workspace_path: selected }; onSettingsChange(next); setNotice("已切换工作区"); };
  const newDocument = () => { setPath(null); setText(focusDocument); setDirty(false); };
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
      const result = await invoke<string[]>("cycle_task_lines", { lines: [line], step: 1 });
      const from = lines.slice(0, lineNumber).reduce((offset, current) => offset + current.length + 1, 0);
      editor.dispatch({ changes: { from, to: from + line.length, insert: result[0] }, selection: { anchor: from }, userEvent: "input.task-cycle" }); editor.focus();
    } catch (error) { setNotice(errorMessage(error)); }
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!active) return;
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && event.key.toLowerCase() === "s") { event.preventDefault(); void persist(); }
      if (modifier && event.key.toLowerCase() === "f") { event.preventDefault(); setPaletteOpen(true); }
      if (modifier && event.key.toLowerCase() === "b") { event.preventDefault(); setSidebarVisible((value) => !value); }
      if (modifier && event.key === "\\") { event.preventDefault(); setTasksVisible((value) => !value); }
      if (event.key === "F11") { event.preventDefault(); setFocusMode((value) => !value); }
      if (event.key === "Escape") setZenMode(false);
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
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

  const documentTitle = path?.split(/[\\/]/).pop() ?? "04-focus-mode.md";
  const classes = ["focus-shell", "markdown-page", focusMode ? "focus-mode" : "", zenMode ? "zen-mode" : "", !sidebarVisible ? "sidebar-hidden" : "", !tasksVisible ? "tasks-hidden" : ""].filter(Boolean).join(" ");
  return <main className={classes}>
    <header className="command-bar">
      <div className="command-actions">
        <button onClick={newDocument}><Plus size={18} />New</button><button onClick={() => void chooseDocument()}><FolderOpen size={18} />Open</button><button onClick={() => void chooseWorkspace()} title="打开文件夹（选择工作区）"><FolderOpen size={18} />Folder</button><button onClick={() => void persist()}><FloppyDisk size={17} />Save</button><button onClick={() => setPaletteOpen(true)}><MagnifyingGlass size={18} />Find</button>
        <button onClick={() => setFilter((value) => value === "all" ? "todo" : "all")}><CheckCircle size={18} />Tasks: {filter === "all" ? "All" : "Todo"}<CaretDown size={15} /></button><button onClick={() => void persist()}><CloudArrowUp size={18} />Sync</button><i className="sync-dot" />
      </div>
      <div className="view-actions"><button className={focusMode ? "active" : ""} onClick={() => setFocusMode((value) => !value)}><Target size={18} />Focus Mode <kbd>F11</kbd></button><button className={zenMode ? "active" : ""} onClick={() => setZenMode((value) => !value)}><Code size={18} />Zen Mode <kbd>⌘ K Z</kbd></button><button className={tasksVisible ? "active" : ""} onClick={() => setTasksVisible((value) => !value)}><SplitHorizontal size={18} />Split</button></div>
    </header>
    <section className="focus-workbench">
      {sidebarVisible ? <WorkspaceTree files={workspaceFiles} path={path} workspaceName={workspace ? fileName(workspace) : "打开文件夹…"} expanded={expandedFolders} onToggleFolder={toggleFolder} onOpen={(filePath) => void loadPath(filePath)} onChooseWorkspace={() => void chooseWorkspace()} headings={headings} onJump={jumpToLine} /> : null}
      <section className="editor-workbench">
        <div className="editor-tabs"><button className="editor-tab active"><Code size={17} weight="bold" />{documentTitle}<X size={15} /></button><button className="new-tab" onClick={newDocument}><Plus size={17} /></button></div>
        <header className="editor-meta"><div><span>{workspace ? fileName(workspace) : "docs"}</span><CaretRight size={14} /><Code size={15} weight="bold" /><strong>{documentTitle}</strong></div><div><span>{text.trim().split(/\s+/).filter(Boolean).length.toLocaleString()} words</span><i /><span>{dirty ? "Unsaved" : "Live"} <b /></span><button title="More editor actions"><DotsThree size={20} /></button></div></header>
        <CodeMirror ref={editorRef} className="focus-editor" height="100%" extensions={[markdown(), ...devtoolboxMarkdown()]} value={text} onChange={setContent} onKeyDown={(event: ReactKeyboardEvent<HTMLDivElement>) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); const line = editorRef.current?.view?.state.doc.lineAt(editorRef.current.view.state.selection.main.from).number; if (line) void cycleTask(line - 1); } }} basicSetup={{ lineNumbers: true, foldGutter: false, highlightActiveLine: false, highlightActiveLineGutter: false }} indentWithTab aria-label="Focus Mode Markdown editor" />
        <footer className="editor-status"><div><SidebarSimple size={18} />Toggle Sidebar</div><div><span>Ln 1, Col 1</span><span>Spaces: 2</span><span>UTF-8</span><span>LF</span><span>Markdown</span><span><Check size={16} />{tasks.length} tasks</span></div></footer>
      </section>
      {tasksVisible ? <TaskOutline fileName={documentTitle} tasks={tasks} filter={filter} setFilter={setFilter} onCycle={(line) => void cycleTask(line)} /> : null}
    </section>
    {paletteOpen ? <div className="palette-backdrop" onMouseDown={() => setPaletteOpen(false)}><section className="command-palette" onMouseDown={(event) => event.stopPropagation()}><header><MagnifyingGlass size={20} /><input autoFocus placeholder="Find a command…" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") setPaletteOpen(false); }} /></header>{["New document", "Open document", "Save document", "Open folder", "Toggle focus mode", "Toggle task outline"].filter((item) => item.toLowerCase().includes(paletteQuery.toLowerCase())).map((item) => <button key={item} onClick={() => { setPaletteOpen(false); if (item === "New document") newDocument(); else if (item === "Open document") void chooseDocument(); else if (item === "Open folder") void chooseWorkspace(); else if (item === "Save document") void persist(); }}>{item}</button>)}</section></div> : null}
  </main>;
}
