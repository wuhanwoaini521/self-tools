import { Decoration, type DecorationSet, EditorView, keymap, ViewPlugin, ViewUpdate } from "@codemirror/view";
import { insertNewlineContinueMarkup } from "@codemirror/lang-markdown";
import { type Extension, type Range, type Text } from "@codemirror/state";

/**
 * DevToolbox Markdown 视觉装饰：
 * - 任务标记按状态着色（todo 灰 / progress 蓝 / done 绿），done 行文本加删除线变暗；
 * - ATX 标题按级别配色（# 记号弱化）；
 * - 围栏代码块整块背景，围栏记号弱化；
 * - 引用块左侧色条 + 微背景。
 *
 * 规则与 Rust `crates/core/src/parser.rs` 的任务行语义保持一致：
 * 列表前缀可选（`-`/`*`/`+`/有序），标记为 `[...]`，`x` 完成、`~` 进行中。
 */

const taskRegex = /^(\s*)(?:([-*+]|\d+[.)])[ \t]+)?(\[)([^\[\]]*)(\])/;
const headingRegex = /^ {0,3}(#{1,6})(\s|$)/;
const fenceRegex = /^ {0,3}(`{3,}|~{3,})/;
const quoteRegex = /^ {0,3}>/;

const taskMark = Decoration.mark({ class: "dtb-task-mark" });
const taskDoneLine = Decoration.mark({ class: "dtb-task-line-done" });
const headingMark = Decoration.mark({ class: "dtb-heading-mark" });
const codeLine = Decoration.line({ class: "dtb-codeblock" });
const codeFenceLine = Decoration.line({ class: "dtb-codeblock dtb-codeblock-fence" });
const quoteLine = Decoration.line({ class: "dtb-blockquote" });

function headingDecoration(level: number) {
  return Decoration.mark({ class: "dtb-heading dtb-heading-" + level });
}

function buildDecorations(doc: Text): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  let fence: { ch: string; len: number } | null = null;

  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    const text = line.text;

    // 围栏代码块（``` 或 ~~~），先于其他规则处理。
    const fenceMatch = fenceRegex.exec(text);
    if (fenceMatch) {
      const marker = fenceMatch[1];
      if (!fence) {
        fence = { ch: marker[0], len: marker.length };
        ranges.push(codeFenceLine.range(line.from));
      } else if (marker[0] === fence.ch && marker.length >= fence.len) {
        fence = null;
        ranges.push(codeFenceLine.range(line.from));
      } else {
        ranges.push(codeLine.range(line.from));
      }
      continue;
    }
    if (fence) {
      ranges.push(codeLine.range(line.from));
      continue;
    }

    // 引用块。
    if (quoteRegex.test(text)) {
      ranges.push(quoteLine.range(line.from));
      continue;
    }

    // ATX 标题：整行按级别着色，`#` 记号额外弱化（嵌套 mark）。
    const headingMatch = headingRegex.exec(text);
    if (headingMatch) {
      const level = headingMatch[1].length;
      ranges.push(headingDecoration(level).range(line.from, line.to));
      ranges.push(headingMark.range(line.from, line.from + headingMatch[1].length));
      continue;
    }

    // 任务行：标记着色，done 行文本删除线变暗。
    const taskMatch = taskRegex.exec(text);
    if (taskMatch) {
      const mark = taskMatch[4];
      const status = mark.toLowerCase() === "x" ? "done" : mark === "~" ? "progress" : "todo";
      const markStart = line.from + taskMatch[0].length - taskMatch[3].length - taskMatch[4].length - taskMatch[5].length;
      const markEnd = line.from + taskMatch[0].length;
      ranges.push(taskMark.range(markStart, markEnd));
      ranges.push(Decoration.mark({ class: "dtb-task-" + status }).range(markStart, markEnd));
      if (status === "done" && line.to > markEnd) {
        ranges.push(taskDoneLine.range(markEnd, line.to));
      }
    }
  }

  return Decoration.set(ranges, true);
}

class DevtoolboxDecorations {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = buildDecorations(view.state.doc);
  }

  update(update: ViewUpdate) {
    if (update.docChanged || update.viewportChanged) {
      this.decorations = buildDecorations(update.state.doc);
    }
  }
}

const decorationsPlugin = ViewPlugin.fromClass(DevtoolboxDecorations, {
  decorations: (plugin) => plugin.decorations,
});

/** 编辑器附加扩展：Markdown 视觉装饰 + 列表自动延续（Enter 补全列表前缀）。 */
export function devtoolboxMarkdown(): Extension[] {
  return [
    decorationsPlugin,
    keymap.of([{ key: "Enter", run: insertNewlineContinueMarkup }]),
  ];
}
