import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { devtoolboxMarkdown } from "./markdown-decorations";
import { oneDark } from "@codemirror/theme-one-dark";
import {
  CaretDown, CaretRight, Check, CheckCircle, CheckSquare, Circle, CloudArrowUp, Code,
  DotsThree, FileText, FloppyDisk, FolderOpen, Gear, MagnifyingGlass, Plus,
  SidebarSimple, SplitHorizontal, Target, X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { AppSettings, DocumentDto, WorkspaceFile } from "./types";

type TaskStatus = "todo" | "progress" | "done";
type TaskFilter = "all" | TaskStatus;
type OutlineTask = { line: number; text: string; status: TaskStatus };

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

const sampleFiles = ["01-getting-started.md", "02-commands.md", "03-configuration.md", "04-focus-mode.md", "05-keyboard-shortcuts.md"];
const defaultSettings: AppSettings = { schema_version: 1, recent_files: [], workspace_path: null, theme_mode: "dark", editor_font_size: 14, auto_save: false, markdown_default_view: "split" };

function isTauriRuntime() { return "__TAURI_INTERNALS__" in window; }
function errorMessage(error: unknown) { return typeof error === "string" ? error : error && typeof error === "object" && "message" in error ? String(error.message) : "操作失败，请查看开发者日志。"; }
function fileName(value: string) { return value.split(/[\\/]/).filter(Boolean).pop() || value; }
function taskStatus(marker: string): TaskStatus { return marker.toLowerCase() === "x" ? "done" : marker === "~" ? "progress" : "todo"; }

function WorkspaceTree({ files, path, onOpen, onNew, onChooseWorkspace }: { files: WorkspaceFile[]; path: string | null; onOpen: (filePath: string) => void; onNew: () => void; onChooseWorkspace: () => void }) {
  const visible = files.length ? files.slice(0, 10).map((file) => ({ label: file.relative_path.replaceAll("\\", "/"), path: file.path })) : sampleFiles.map((label) => ({ label, path: label }));
  return <aside className="command-sidebar">
    <header className="project-heading"><span>Project</span><button title="项目选项"><DotsThree size={19} /></button></header>
    <button className="project-name" onClick={onChooseWorkspace}>DevToolbox Docs</button>
    <div className="tree-scroll">
      <button className="tree-folder" onClick={onChooseWorkspace}><CaretRight size={15} /><FolderOpen size={17} />.devtoolbox</button>
      <button className="tree-folder open" onClick={onChooseWorkspace}><CaretDown size={15} /><FolderOpen size={17} />docs</button>
      <div className="tree-files">{visible.map((file) => <button key={file.path} className={(path ? path.endsWith(file.label) : file.label === "04-focus-mode.md") ? "selected" : ""} onClick={() => file.path.includes(":") || file.path.includes("\\") ? onOpen(file.path) : onNew()}><Code size={15} weight="bold" />{file.label}<i /></button>)}</div>
      <button className="tree-folder"><CaretRight size={15} /><FolderOpen size={17} />Guides</button>
      <div className="tree-subfiles"><span><Code size={15} weight="bold" />markdown-tasks.md</span><span><Code size={15} weight="bold" />productivity.md</span></div>
      <button className="tree-folder"><CaretRight size={15} /><FolderOpen size={17} />assets</button>
      {["CHANGELOG.md", "README.md", "ROADMAP.md", "TODO.md"].map((name) => <span className="tree-root-file" key={name}><Code size={15} weight="bold" />{name}</span>)}
    </div>
    <section className="document-outline"><p>Outline</p>{["Focus Mode Command Center", "Goals", "Core Principles", "Using Focus Mode", "Task Workflow", "Keyboard Shortcuts", "Tips & Best Practices"].map((heading, index) => <button key={heading}><small>{index === 0 ? "1" : index < 3 ? "1." + index : String(index - 1)}</small>{heading}</button>)}</section>
    <footer className="sidebar-footer"><SidebarSimple size={18} />Toggle Sidebar</footer>
  </aside>;
}

function TaskOutline({ tasks, filter, setFilter, onCycle }: { tasks: OutlineTask[]; filter: TaskFilter; setFilter: (filter: TaskFilter) => void; onCycle: (line: number) => void }) {
  const counts = useMemo(() => ({ all: tasks.length, todo: tasks.filter((task) => task.status === "todo").length, progress: tasks.filter((task) => task.status === "progress").length, done: tasks.filter((task) => task.status === "done").length }), [tasks]);
  const visible = filter === "all" ? tasks : tasks.filter((task) => task.status === filter);
  return <aside className="task-outline">
    <header><div><p>Task Outline</p></div><button title="收起任务大纲"><CaretDown size={17} /></button></header>
    <nav className="task-tabs" aria-label="任务过滤">{([{ key: "all", label: "All" }, { key: "todo", label: "Todo" }, { key: "progress", label: "In Progress" }, { key: "done", label: "Done" }] as const).map((tab) => <button key={tab.key} className={filter === tab.key ? "selected" : ""} onClick={() => setFilter(tab.key)}>{tab.label}<b>{counts[tab.key]}</b></button>)}</nav>
    <div className="task-file"><Code size={17} weight="bold" />04-focus-mode.md</div>
    <section className="task-list"><div className="task-list-title"><CaretDown size={15} />Task Workflow <b>{tasks.length}</b></div>{visible.map((task) => <button className={"outline-task " + task.status} key={task.line} onClick={() => onCycle(task.line)}>{task.status === "done" ? <CheckSquare size={19} weight="fill" /> : task.status === "progress" ? <Target size={20} weight="bold" /> : <Circle size={19} />}<span>{task.text}</span><small>{task.status === "done" ? "@done" : task.status === "progress" ? "@in-progress" : "@todo"}</small></button>)}</section>
    {["Using Focus Mode", "Keyboard Shortcuts", "Tips & Best Practices"].map((name) => <div className="collapsed-outline" key={name}><CaretRight size={15} />{name}<b>0</b></div>)}
    <footer><kbd>⌘\\</kbd> Toggle Task Outline <kbd>↵</kbd> Toggle Status</footer>
  </aside>;
}

export default function App() {
  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const [text, setText] = useState(focusDocument);
  const [path, setPath] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<string | null>(null);
  const [workspaceFiles, setWorkspaceFiles] = useState<WorkspaceFile[]>([]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [filter, setFilter] = useState<TaskFilter>("all");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [notice, setNotice] = useState("");
  const [dirty, setDirty] = useState(false);
  const [focusMode, setFocusMode] = useState(true);
  const [zenMode, setZenMode] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [tasksVisible, setTasksVisible] = useState(true);

  const refreshWorkspace = useCallback(async (folder: string | null) => { if (!folder) { setWorkspaceFiles([]); return; } setWorkspaceFiles(await invoke<WorkspaceFile[]>("list_workspace", { path: folder })); }, []);
  useEffect(() => {
    document.documentElement.dataset.theme = "command-dark";
    if (!isTauriRuntime()) return;
    void invoke<AppSettings>("get_settings").then(async (loaded) => { setSettings(loaded); setWorkspace(loaded.workspace_path); await refreshWorkspace(loaded.workspace_path); }).catch((error) => setNotice(errorMessage(error)));
  }, [refreshWorkspace]);

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
      await invoke("put_settings", { settings: next });
      setSettings(next); setDirty(false); setNotice("Saved " + target.split(/[\\/]/).pop());
    } catch (error) { setNotice(errorMessage(error)); }
  }, [path, settings, text]);

  const loadPath = useCallback(async (selected: string) => {
    try {
      if (dirty && !window.confirm("当前文档有未保存的修改，仍然打开新文档吗？")) return;
      const document = await invoke<DocumentDto>("read_document", { path: selected });
      setPath(document.path); setText(document.text); setDirty(false);
    } catch (error) { setNotice(errorMessage(error)); }
  }, [dirty]);

  const chooseDocument = async () => { const selected = await open({ multiple: false, directory: false, filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }] }); if (typeof selected === "string") await loadPath(selected); };
  const chooseWorkspace = async () => { const selected = await open({ multiple: false, directory: true }); if (typeof selected !== "string") return; await refreshWorkspace(selected); setWorkspace(selected); const next = { ...settings, workspace_path: selected }; await invoke("put_settings", { settings: next }); setSettings(next); setNotice("已切换工作区"); };
  const setContent = (next: string) => { setText(next); setDirty(true); if (settings.auto_save && path) void persist(path, next); };
  const tasks = useMemo<OutlineTask[]>(() => text.split(/\r?\n/).flatMap((line, index) => { const match = /^\s*(?:(?:[-*+]|\d+[.)])\s+)?\[([ ~x])\]\s*(.*)$/.exec(line); return match ? [{ line: index, text: match[2] || "Untitled task", status: taskStatus(match[1]) }] : []; }), [text]);

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
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && event.key.toLowerCase() === "s") { event.preventDefault(); void persist(); }
      if (modifier && event.key.toLowerCase() === "f") { event.preventDefault(); setPaletteOpen(true); }
      if (modifier && event.key.toLowerCase() === "b") { event.preventDefault(); setSidebarVisible((value) => !value); }
      if (modifier && event.key === "\\") { event.preventDefault(); setTasksVisible((value) => !value); }
      if (event.key === "F11") { event.preventDefault(); setFocusMode((value) => !value); }
      if (event.key === "Escape") setZenMode(false);
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, [persist]);

  const documentTitle = path?.split(/[\\/]/).pop() ?? "04-focus-mode.md";
  const classes = ["focus-shell", focusMode ? "focus-mode" : "", zenMode ? "zen-mode" : "", !sidebarVisible ? "sidebar-hidden" : "", !tasksVisible ? "tasks-hidden" : ""].filter(Boolean).join(" ");
  return <main className={classes}>
    <header className="command-bar">
      <div className="brand"><strong>DevToolbox</strong><span /><p>Focus Mode Command Center</p></div>
      <div className="command-actions">
        <button onClick={() => { setPath(null); setText(focusDocument); setDirty(false); }}><Plus size={18} />New</button><button onClick={() => void chooseDocument()}><FolderOpen size={18} />Open</button><button onClick={() => void persist()}><FloppyDisk size={17} />Save</button><button onClick={() => setPaletteOpen(true)}><MagnifyingGlass size={18} />Find</button>
        <button onClick={() => setFilter((value) => value === "all" ? "todo" : "all")}><CheckCircle size={18} />Tasks: {filter === "all" ? "All" : "Todo"}<CaretDown size={15} /></button><button onClick={() => void persist()}><CloudArrowUp size={18} />Sync</button><i className="sync-dot" />
      </div>
      <div className="view-actions"><button className={focusMode ? "active" : ""} onClick={() => setFocusMode((value) => !value)}><Target size={18} />Focus Mode <kbd>F11</kbd></button><button className={zenMode ? "active" : ""} onClick={() => setZenMode((value) => !value)}><Code size={18} />Zen Mode <kbd>⌘ K Z</kbd></button><button className={tasksVisible ? "active" : ""} onClick={() => setTasksVisible((value) => !value)}><SplitHorizontal size={18} />Split</button><button title="Settings" onClick={() => setNotice("Settings are available from the command palette.")}><Gear size={20} /></button></div>
    </header>
    <section className="focus-workbench">
      {sidebarVisible ? <WorkspaceTree files={workspaceFiles} path={path} onOpen={(filePath) => void loadPath(filePath)} onNew={() => { setPath(null); setText(focusDocument); setDirty(false); }} onChooseWorkspace={() => void chooseWorkspace()} /> : null}
      <section className="editor-workbench">
        <div className="editor-tabs"><button className="editor-tab active"><Code size={17} weight="bold" />{documentTitle}<X size={15} /></button><button className="new-tab" onClick={() => { setPath(null); setText(focusDocument); }}><Plus size={17} /></button></div>
        <header className="editor-meta"><div><span>{workspace ? fileName(workspace) : "docs"}</span><CaretRight size={14} /><Code size={15} weight="bold" /><strong>{documentTitle}</strong></div><div><span>{text.trim().split(/\s+/).filter(Boolean).length.toLocaleString()} words</span><i /><span>{dirty ? "Unsaved" : "Live"} <b /></span><button title="More editor actions"><DotsThree size={20} /></button></div></header>
        <CodeMirror ref={editorRef} className="focus-editor" height="100%" theme={oneDark} extensions={[markdown(), ...devtoolboxMarkdown()]} value={text} onChange={setContent} onKeyDown={(event: ReactKeyboardEvent<HTMLDivElement>) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); const line = editorRef.current?.view?.state.doc.lineAt(editorRef.current.view.state.selection.main.from).number; if (line) void cycleTask(line - 1); } }} basicSetup={{ lineNumbers: true, foldGutter: false, highlightActiveLine: false, highlightActiveLineGutter: false }} indentWithTab aria-label="Focus Mode Markdown editor" />
        <footer className="editor-status"><div><SidebarSimple size={18} />Toggle Sidebar</div><div><span>Ln 1, Col 1</span><span>Spaces: 2</span><span>UTF-8</span><span>LF</span><span>Markdown</span><span><Check size={16} />{tasks.length} tasks</span></div></footer>
      </section>
      {tasksVisible ? <TaskOutline tasks={tasks} filter={filter} setFilter={setFilter} onCycle={(line) => void cycleTask(line)} /> : null}
    </section>
    {notice ? <button className="toast" onClick={() => setNotice("")}>{notice}<X size={16} /></button> : null}
    {paletteOpen ? <div className="palette-backdrop" onMouseDown={() => setPaletteOpen(false)}><section className="command-palette" onMouseDown={(event) => event.stopPropagation()}><header><MagnifyingGlass size={20} /><input autoFocus placeholder="Find a command…" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") setPaletteOpen(false); }} /></header>{["New document", "Open document", "Save document", "Toggle focus mode", "Toggle task outline"].filter((item) => item.toLowerCase().includes(paletteQuery.toLowerCase())).map((item) => <button key={item} onClick={() => setPaletteOpen(false)}>{item}</button>)}</section></div> : null}
  </main>;
}
