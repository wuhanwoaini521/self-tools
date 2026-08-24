"""Markdown 任务行解析与生成。

底层存储保持普通 Markdown 任务列表语法::

    - [ ] US      → Pending
    - [~] JP      → Ing
    - [x] CA      → Done

规则：
* 支持有/无列表符号（``-`` ``*`` ``+`` ``1.``）的行；
* 支持任意缩进与嵌套列表；
* 方括号内是注册表未知的标记时，视为普通文本，不做任何处理，
  因此不会破坏用户文档中的其他内容。
"""
from __future__ import annotations

import re
from dataclasses import dataclass

from .task_state import TaskStateRegistry

# 行首：缩进 + 可选列表符号（"- " / "* " / "+ " / "1. " / "2) "）
_LIST_PREFIX_RE = re.compile(r"^(?P<indent>\s*)(?P<bullet>(?:[-*+]|\d+[.)])[ \t]+)?")
# 紧随其后的状态标记
_MARK_RE = re.compile(r"\[(?P<mark>[^\[\]]*)\]")


@dataclass(frozen=True)
class TaskLineInfo:
    """一行任务文本的解析结果。"""

    indent: str        # 前导空白
    bullet: str | None  # 列表符号（含尾随空白），可能为 None
    mark: str          # 方括号内的原始标记
    text: str          # 标记之后的正文（已去除前导空白）
    mark_start: int    # "[" 在整行中的起始列
    mark_end: int      # "]" 在整行中的结束列（不含）

    def rebuild(self, mark: str) -> str:
        """用新的标记重建整行文本。"""
        prefix = f"{self.indent}{self.bullet or '- '}"
        if self.text:
            return f"{prefix}[{mark}] {self.text}"
        return f"{prefix}[{mark}]"


def match_task(line: str) -> TaskLineInfo | None:
    """解析一行文本；不是「看起来像任务」的行返回 None。"""
    m = _LIST_PREFIX_RE.match(line)
    mm = _MARK_RE.match(line, m.end())
    if mm is None:
        return None
    return TaskLineInfo(
        indent=m.group("indent"),
        bullet=m.group("bullet"),
        mark=mm.group("mark"),
        text=line[mm.end():].lstrip(),
        mark_start=mm.start(),
        mark_end=mm.end(),
    )


def is_task_line(line: str, registry: TaskStateRegistry) -> bool:
    """该行是否是一个**已知状态**的任务项。"""
    info = match_task(line)
    return info is not None and registry.by_mark(info.mark) is not None


def make_task_line(line: str, mark: str) -> str:
    """把任意一行转换成任务行。

    * 已是任务行：仅更新标记（幂等）；
    * 已有列表符号的普通列表项：保留符号，插入标记；
    * 普通文本行：添加 "- " 列表符号 + 标记；
    * 空白行：生成一个空任务项。
    """
    info = match_task(line)
    if info is not None:
        prefix = f"{info.indent}{info.bullet or '- '}"
        if info.text:
            return f"{prefix}[{mark}] {info.text}"
        return f"{prefix}[{mark}]"
    m = _LIST_PREFIX_RE.match(line)
    if m is not None and m.group("bullet"):
        indent, bullet = m.group("indent"), m.group("bullet")
        content = line[m.end():].strip()
        if content:
            return f"{indent}{bullet}[{mark}] {content}"
        return f"{indent}{bullet}[{mark}]"
    stripped = line.strip()
    if not stripped:
        return "- [ ]"
    indent = line[: len(line) - len(line.lstrip())]
    return f"{indent}- [{mark}] {stripped}"


def set_task_mark(line: str, mark: str) -> str:
    """把已有任务行的状态标记替换为指定标记；非任务行原样返回。"""
    info = match_task(line)
    if info is None:
        return line
    return info.rebuild(mark)


def cycle_task_mark(
    line: str,
    registry: TaskStateRegistry,
    step: int = 1,
) -> tuple[str, bool]:
    """切换任务行的状态。

    返回 ``(新文本, 是否发生变化)``。未知标记或非任务行不修改，
    这样不会破坏普通 Markdown 内容。
    """
    info = match_task(line)
    if info is None:
        return line, False
    state = registry.by_mark(info.mark)
    if state is None:
        return line, False
    new_mark = registry.next_of(state.state_id, step).mark
    if new_mark == info.mark:
        return line, False
    return info.rebuild(new_mark), True


def iter_task_lines(text: str, registry: TaskStateRegistry):
    """遍历文本中所有任务行，yield ``(行号(0-based), TaskLineInfo, TaskState)``。"""
    for i, line in enumerate(text.splitlines()):
        info = match_task(line)
        if info is not None:
            state = registry.by_mark(info.mark)
            if state is not None:
                yield i, info, state
