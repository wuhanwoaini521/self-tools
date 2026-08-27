import DOMPurify from "dompurify";
import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import { marked } from "marked";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { oneDark } from "@codemirror/theme-one-dark";
import { redo, undo } from "@codemirror/commands";
import { CheckCircle, CheckSquare, Circle, Code, Copy, DotsThree, FileText, FolderOpen, Gear, LinkSimple, MagnifyingGlass, Plus, ShareNetwork, Star, Trash } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { AppSettings, DocumentDto, MarkdownView, ThemeMode, WorkspaceFile } from "./types";

type Page = "markdown" | "api" | "json" | "string" | "converter" | "settings";
type Command = { id: string; label: string; shortcut?: string; run: () => void | Promise<void> };
type RightPanel = "tasks" | "preview";
type TaskStatus = "pending" | "progress" | "done";
type TimelineTask = { line: number; text: string; status: TaskStatus };

const defaultSettings: AppSettings = {
  schema_version: 1,
  recent_files: [],
  workspace_path: null,
  theme_mode: "system",
  editor_font_size: 13,
  auto_save: false,
  markdown_default_view: "split",
};

function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return "操作失败，请查看开发者日志。";
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function taskSymbols(markdown: string): string {
  return markdown
    .replace(/^(\s*(?:(?:[-*+]|\d+[.)])\s+)?\[ \])/gm, "$1<span class=\"task-symbol pending\">○</span>")
    .replace(/^(\s*(?:(?:[-*+]|\d+[.)])\s+)?\[~\])/gm, "$1<span class=\"task-symbol progress\">◐</span>")
    .replace(/^(\s*(?:(?:[-*+]|\d+[.)])\s+)?\[x\])/gim, "$1<span class=\"task-symbol done\">●</span>");
}

function lineRange(text: string, start: number, end: number) {
  const from = text.lastIndexOf("\n", Math.max(0, start - 1)) + 1;
  const newline = text.indexOf("\n", end);
  const to = newline === -1 ? text.length : newline;
  return { from, to };
}

export default function App() {
  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const previewRef = useRef<HTMLElement>(null);
  const [page, setPage] = useState<Page>("markdown");
  const [text, setText] = useState("");
  const [path, setPath] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<string | null>(null);
  const [workspaceFiles, setWorkspaceFiles] = useState<WorkspaceFile[]>([]);
  const [rightPanel, setRightPanel] = useState<RightPanel>("tasks");
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [notice, setNotice] = useState("");
  const [dirty, setDirty] = useState(false);

  const theme: ThemeMode = settings.theme_mode;
  const dark = theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  const renderedMarkdown = useMemo(() => {
    const html = marked.parse(taskSymbols(text), { async: false, gfm: true, breaks: false });
    return DOMPurify.sanitize(html, { USE_PROFILES: { html: true } });
  }, [text]);

  const saveSettings = useCallback(async (next: AppSettings) => {
    await invoke("put_settings", { settings: next });
    setSettings(next);
  }, []);

  const refreshWorkspace = useCallback(async (nextWorkspace: string | null) => {
    if (!nextWorkspace) {
      setWorkspaceFiles([]);
      return;
    }
    const files = await invoke<WorkspaceFile[]>("list_workspace", { path: nextWorkspace });
    setWorkspaceFiles(files);
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void invoke<AppSettings>("get_settings")
      .then(async (loaded) => {
        setSettings(loaded);
        setWorkspace(loaded.workspace_path);
        setRightPanel(loaded.markdown_default_view === "preview" ? "preview" : "tasks");
        await refreshWorkspace(loaded.workspace_path);
      })
      .catch((error) => setNotice(errorMessage(error)));
  }, [refreshWorkspace]);

  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  }, [dark]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen(true);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    const editor = editorRef.current?.view;
    const preview = previewRef.current;
    if (!editor || !preview || rightPanel !== "preview") return undefined;
    const syncPreview = () => {
      const editorHeight = editor.scrollDOM.scrollHeight - editor.scrollDOM.clientHeight;
      const previewHeight = preview.scrollHeight - preview.clientHeight;
      if (editorHeight > 0 && previewHeight > 0) preview.scrollTop = (editor.scrollDOM.scrollTop / editorHeight) * previewHeight;
    };
    editor.scrollDOM.addEventListener("scroll", syncPreview, { passive: true });
    return () => editor.scrollDOM.removeEventListener("scroll", syncPreview);
  }, [rightPanel]);

  const setContent = (next: string) => {
    setText(next);
    setDirty(true);
    if (settings.auto_save && path) void persist(path, next);
  };

  const persist = async (targetPath = path, targetText = text) => {
    if (!targetPath) return saveAs();
    try {
      await invoke("write_document", { path: targetPath, text: targetText });
      const recent = [targetPath, ...settings.recent_files.filter((file) => file !== targetPath)].slice(0, 10);
      await saveSettings({ ...settings, recent_files: recent });
      setDirty(false);
      setNotice(`已保存：${targetPath.split(/[\\/]/).pop()}`);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const saveAs = async () => {
    const selected = await save({ defaultPath: path ?? "untitled.md", filters: [{ name: "Markdown", extensions: ["md", "markdown"] }] });
    if (!selected) return;
    setPath(selected);
    await persist(selected, text);
  };

  const loadPath = async (selected: string) => {
    try {
      if (dirty && !window.confirm("当前文档有未保存的修改，仍然打开新文档吗？")) return;
      const document = await invoke<DocumentDto>("read_document", { path: selected });
      setPath(document.path);
      setText(document.text);
      setDirty(false);
      setNotice(`已打开：${selected.split(/[\\/]/).pop()}`);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const chooseDocument = async () => {
    const selected = await open({ multiple: false, directory: false, filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }] });
    if (typeof selected === "string") await loadPath(selected);
  };

  const chooseWorkspace = async () => {
    const selected = await open({ multiple: false, directory: true });
    if (typeof selected !== "string") return;
    try {
      await refreshWorkspace(selected);
      setWorkspace(selected);
      await saveSettings({ ...settings, workspace_path: selected });
      setNotice(`工作区：${selected.split(/[\\/]/).pop()}`);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const transformSelectedLines = async (command: "convert_task_lines" | "cycle_task_lines", step?: number) => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const sourceText = editor.state.doc.toString();
    const selection = editor.state.selection.main;
    const range = lineRange(sourceText, selection.from, selection.to);
    const source = sourceText.slice(range.from, range.to).split("\n");
    const lines = await invoke<string[]>(command, step === undefined ? { lines: source } : { lines: source, step });
    const replacement = lines.join("\n");
    editor.dispatch({
      changes: { from: range.from, to: range.to, insert: replacement },
      selection: { anchor: range.from, head: range.from + replacement.length },
      userEvent: "input.task-transform",
    });
  };

  const cycleClickedTask = async () => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const sourceText = editor.state.doc.toString();
    const selection = editor.state.selection.main;
    const range = lineRange(sourceText, selection.from, selection.from);
    const line = sourceText.slice(range.from, range.to);
    const marker = /\[([^\[\]]*)\]/.exec(line);
    const column = selection.from - range.from;
    if (!marker || ![" ", "~", "x"].includes(marker[1]) || marker.index === undefined) return;
    if (column < marker.index - 1 || column > marker.index + marker[0].length + 1) return;
    const [replacement] = await invoke<string[]>("cycle_task_lines", { lines: [line], step: 1 });
    editor.dispatch({
      changes: { from: range.from, to: range.to, insert: replacement },
      selection: { anchor: range.from + column },
      userEvent: "input.task-cycle",
    });
  };

  const continueTaskList = () => {
    const editor = editorRef.current?.view;
    if (!editor) return false;
    const sourceText = editor.state.doc.toString();
    const selection = editor.state.selection.main;
    const range = lineRange(sourceText, selection.from, selection.from);
    const line = sourceText.slice(range.from, range.to);
    const task = /^(\s*)((?:[-*+]|\d+[.)])\s+)\[([^\[\]]*)\]\s*(.*)$/.exec(line);
    const list = /^(\s*)((?:[-*+]|\d+[.)])\s+)(.*)$/.exec(line);
    let continuation: string | null = null;
    if (task && [" ", "~", "x"].includes(task[3])) continuation = task[4] ? `${task[1]}${task[2]}[${task[3]}] ` : "";
    else if (list) continuation = list[3].trim() ? `${list[1]}${list[2]}` : "";
    if (continuation === null) return false;
    const position = selection.from;
    editor.dispatch({
      changes: { from: position, to: selection.to, insert: `\n${continuation}` },
      selection: { anchor: position + continuation.length + 1 },
      userEvent: "input.task-continue",
    });
    return true;
  };

  const wrapSelection = (prefix: string, suffix: string, placeholder: string) => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const selection = editor.state.selection.main;
    const sourceText = editor.state.doc.toString();
    const selected = sourceText.slice(selection.from, selection.to) || placeholder;
    editor.dispatch({
      changes: { from: selection.from, to: selection.to, insert: `${prefix}${selected}${suffix}` },
      selection: { anchor: selection.from + prefix.length, head: selection.from + prefix.length + selected.length },
      userEvent: "input.markdown-format",
    });
    editor.focus();
  };

  const prefixSelectedLines = (prefix: string) => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const sourceText = editor.state.doc.toString();
    const selection = editor.state.selection.main;
    const range = lineRange(sourceText, selection.from, selection.to);
    const replacement = sourceText.slice(range.from, range.to).split("\n").map((line) => `${prefix}${line}`).join("\n");
    editor.dispatch({
      changes: { from: range.from, to: range.to, insert: replacement },
      selection: { anchor: range.from, head: range.from + replacement.length },
      userEvent: "input.markdown-format",
    });
    editor.focus();
  };

  const timelineTasks = useMemo<TimelineTask[]>(() => text.split(/\r?\n/).flatMap((line, index) => {
    const match = /^\s*(?:(?:[-*+]|\d+[.)])\s+)?\[([ ~x])\]\s*(.*)$/.exec(line);
    if (!match) return [];
    const status: TaskStatus = match[1] === "x" ? "done" : match[1] === "~" ? "progress" : "pending";
    return [{ line: index, text: match[2] || "未命名任务", status }];
  }), [text]);

  const cycleTimelineTask = async (lineNumber: number) => {
    const editor = editorRef.current?.view;
    if (!editor) return;
    const sourceText = editor.state.doc.toString();
    const lines = sourceText.split("\n");
    const line = lines[lineNumber];
    if (line === undefined) return;
    const [replacement] = await invoke<string[]>("cycle_task_lines", { lines: [line], step: 1 });
    const from = lines.slice(0, lineNumber).reduce((offset, current) => offset + current.length + 1, 0);
    editor.dispatch({
      changes: { from, to: from + line.length, insert: replacement },
      selection: { anchor: from },
      userEvent: "input.task-cycle",
    });
    editor.focus();
  };

  const commands: Command[] = [
    { id: "new", label: "新建文档", shortcut: "Ctrl+N", run: () => { if (!dirty || window.confirm("丢弃未保存修改？")) { setPath(null); setText(""); setDirty(false); } } },
    { id: "open", label: "打开文档", shortcut: "Ctrl+O", run: chooseDocument },
    { id: "workspace", label: "打开工作区", shortcut: "Ctrl+Shift+O", run: chooseWorkspace },
    { id: "save", label: "保存文档", shortcut: "Ctrl+S", run: () => persist() },
    { id: "save-as", label: "另存为", shortcut: "Ctrl+Shift+S", run: saveAs },
    { id: "convert", label: "转换选中行为任务", shortcut: "Ctrl+L", run: () => transformSelectedLines("convert_task_lines") },
    { id: "cycle-next", label: "下一个任务状态", shortcut: "Ctrl+Enter", run: () => transformSelectedLines("cycle_task_lines", 1) },
    { id: "cycle-prev", label: "上一个任务状态", shortcut: "Ctrl+Shift+Enter", run: () => transformSelectedLines("cycle_task_lines", -1) },
    { id: "editor", label: "任务时间线", run: () => setRightPanel("tasks") },
    { id: "preview", label: "预览文档", run: () => setRightPanel("preview") },
    { id: "theme", label: "切换深浅主题", shortcut: "Ctrl+Shift+T", run: () => void saveSettings({ ...settings, theme_mode: dark ? "light" : "dark" }) },
  ];

  const filteredCommands = commands.filter((command) => command.label.toLowerCase().includes(paletteQuery.toLowerCase()));

  const onEditorKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const key = event.key.toLowerCase();
    if ((event.ctrlKey || event.metaKey) && key === "s") { event.preventDefault(); void persist(); }
    if ((event.ctrlKey || event.metaKey) && key === "l") { event.preventDefault(); void transformSelectedLines("convert_task_lines"); }
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); void transformSelectedLines("cycle_task_lines", event.shiftKey ? -1 : 1); }
    if (!event.ctrlKey && !event.metaKey && event.key === "Enter" && continueTaskList()) event.preventDefault();
  };

  const updatePreference = async <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    await saveSettings({ ...settings, [key]: value });
  };

  return (
    <main className="project-shell">
      <aside className="project-sidebar">
        <button className="wordmark" onClick={() => setPage("markdown")}>DevToolbox</button>
        <nav className="primary-nav" aria-label="主导航">
          <button className={page === "markdown" ? "active" : ""} onClick={() => setPage("markdown")}><FolderOpen size={20} weight="duotone" />工作区</button>
          <button onClick={() => setPaletteOpen(true)}><MagnifyingGlass size={20} />搜索</button>
          <button onClick={() => setRightPanel("tasks")}><CheckSquare size={20} />任务</button>
          <button onClick={() => setPage("converter")}><Code size={20} />转换</button>
          <button onClick={() => setPage("json")}><FileText size={20} />工具</button>
          <button onClick={() => setPage("settings")}><Gear size={20} />设置</button>
        </nav>
        <div className="project-list">
          <div className="section-label">工作区 <button title="打开工作区" onClick={() => void chooseWorkspace()}><Plus size={17} /></button></div>
          {workspace ? <button className="workspace-root" onClick={() => void chooseWorkspace()}><FolderOpen size={18} />{workspace.split(/[\\/]/).pop()}</button> : <button className="workspace-root" onClick={() => void chooseWorkspace()}><FolderOpen size={18} />打开工作区</button>}
          <div className="file-list">
            {workspaceFiles.slice(0, 8).map((file) => <button className={file.path === path ? "selected" : ""} key={file.path} onClick={() => void loadPath(file.path)}><FileText size={17} />{file.relative_path.replaceAll("\\", "/")}</button>)}
          </div>
        </div>
        <button className="trash-link"><Trash size={19} />废纸篓</button>
      </aside>
      <section className="project-content">
        {page === "settings" ? <section className="settings-panel"><div><p className="eyebrow">偏好设置</p><h1>设置</h1></div><label>主题<select value={settings.theme_mode} onChange={(event) => void updatePreference("theme_mode", event.target.value as ThemeMode)}><option value="light">浅色</option><option value="dark">深色</option><option value="system">跟随系统</option></select></label><label>编辑器字号<input type="number" min="9" max="24" value={settings.editor_font_size} onChange={(event) => void updatePreference("editor_font_size", Number(event.target.value))} /></label><label className="check-setting"><input type="checkbox" checked={settings.auto_save} onChange={(event) => void updatePreference("auto_save", event.target.checked)} /> 自动保存</label><label>默认右侧面板<select value={settings.markdown_default_view} onChange={(event) => void updatePreference("markdown_default_view", event.target.value as MarkdownView)}><option value="editor">任务</option><option value="split">任务</option><option value="preview">预览</option></select></label></section> : null}
        {page !== "markdown" && page !== "settings" ? <section className="empty-tool"><Code size={32} weight="duotone" /><h1>即将提供</h1><p>此模块保留入口，当前版本专注于 Markdown 任务工作区。</p></section> : null}
        {page === "markdown" ? <section className="workspace-screen">
          <header className="document-header"><div className="breadcrumbs"><span>DevToolbox</span><span>/</span><span>工作区</span><span>/</span><strong>{path?.split(/[\\/]/).pop() ?? "Untitled.md"}</strong></div><div className="document-actions"><span>{dirty ? "未保存的修改" : "已保存"}</span><button title="收藏"><Star size={20} /></button><button title="复制"><Copy size={20} /></button><button title="分享"><ShareNetwork size={20} /></button><button title="更多"><DotsThree size={22} /></button></div></header>
          <div className="document-layout">
            <section className="writing-pane">
              <div className="document-title-row"><div><p className="eyebrow">Markdown 文档</p><h1>{path?.split(/[\\/]/).pop()?.replace(/\.(md|markdown|txt)$/i, "") ?? "Untitled"}</h1></div><button className="open-button" onClick={() => void chooseDocument()}>打开文档</button></div>
              <div className="format-toolbar" aria-label="Markdown 格式工具"><button title="标题" onClick={() => prefixSelectedLines("# ")}>H1</button><button title="粗体" onClick={() => wrapSelection("**", "**", "粗体文本")}>B</button><button title="斜体" onClick={() => wrapSelection("*", "*", "斜体文本")}>I</button><button title="行内代码" onClick={() => wrapSelection("`", "`", "代码")}><Code size={18} /></button><button title="插入链接" onClick={() => wrapSelection("[", "](https://)", "链接文本")}><LinkSimple size={18} /></button><span /><button title="撤销" onClick={() => editorRef.current?.view && undo(editorRef.current.view)}>撤销</button><button title="重做" onClick={() => editorRef.current?.view && redo(editorRef.current.view)}>重做</button></div>
              <CodeMirror ref={editorRef} className="editor" style={{ fontSize: String(settings.editor_font_size) + "px" }} height="100%" theme={dark ? oneDark : "light"} extensions={[markdown()]} value={text} onChange={(next) => setContent(next)} onKeyDown={onEditorKeyDown} onClick={() => void cycleClickedTask()} basicSetup={{ lineNumbers: true, foldGutter: false, highlightActiveLine: true, highlightActiveLineGutter: true }} indentWithTab aria-label="Markdown 编辑器" />
              <footer className="editor-status"><span>Markdown</span><span>{text.split(/\s+/).filter(Boolean).length} words</span><span>{text.length} chars</span><span>UTF-8</span></footer>
            </section>
            <aside className="timeline-panel">
              <header className="timeline-header"><div><p className="eyebrow">执行视图</p><h2>{rightPanel === "tasks" ? "任务时间线" : "文档预览"}</h2></div><button title="切换预览" onClick={() => setRightPanel(rightPanel === "tasks" ? "preview" : "tasks")}><CheckSquare size={21} /></button></header>
              {rightPanel === "preview" ? <article ref={previewRef} className="preview" dangerouslySetInnerHTML={{ __html: renderedMarkdown }} /> : <div className="task-groups">
                {(["pending", "progress", "done"] as TaskStatus[]).map((status) => { const tasks = timelineTasks.filter((task) => task.status === status); const label = status === "pending" ? "待办" : status === "progress" ? "进行中" : "已完成"; return <section className={"task-group " + status} key={status}><div className="task-group-title"><span>{label}</span><b>{tasks.length}</b></div>{tasks.length ? tasks.map((task) => <button className="timeline-task" key={task.line} onClick={() => void cycleTimelineTask(task.line)}>{status === "done" ? <CheckCircle size={20} weight="fill" /> : status === "progress" ? <CheckSquare size={20} weight="duotone" /> : <Circle size={20} />}<span>{task.text}</span></button>) : <p className="empty-tasks">暂无任务</p>}</section>; })}
              </div>}
              {rightPanel === "tasks" ? <button className="create-task" onClick={() => void transformSelectedLines("convert_task_lines")}><Plus size={19} weight="bold" />创建任务</button> : null}
            </aside>
          </div>
        </section> : null}
      </section>
      {notice ? <div className="notice">{notice}</div> : null}
      {paletteOpen ? <div className="modal-backdrop" onMouseDown={() => setPaletteOpen(false)}><section className="command-palette" onMouseDown={(event) => event.stopPropagation()}><input autoFocus placeholder="搜索命令…" value={paletteQuery} onChange={(event) => setPaletteQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") setPaletteOpen(false); }} />{filteredCommands.map((command) => <button key={command.id} onClick={() => { void command.run(); setPaletteOpen(false); }}>{command.label}<kbd>{command.shortcut}</kbd></button>)}</section></div> : null}
    </main>
  );
}
