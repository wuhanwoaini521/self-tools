"""围栏代码块的轻量语法着色。

不引入第三方依赖，按语言提供「颜色提示」级别的着色：
关键字 / 字符串 / 数字 / 注释 / 函数调用。
支持的语言别名见 ``resolve_lang``；未知语言回退到通用规则
（字符串、数字、常见注释）。

行内格式由 MarkdownHighlighter 在逐块扫描时回调 ``set_format``
应用，本模块只负责分词与选色。
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field

# -- 配色 ------------------------------------------------------------------
COLOR_KEYWORD = "#d73a49"
COLOR_STRING = "#22863a"
COLOR_NUMBER = "#005cc5"
COLOR_COMMENT = "#6a737d"
COLOR_FUNC = "#6f42c1"


@dataclass(frozen=True)
class LangSpec:
    name: str
    keywords: frozenset[str] = field(default_factory=frozenset)
    line_comments: tuple[str, ...] = ()
    case_insensitive: bool = False


def _kw(*words: str) -> frozenset[str]:
    return frozenset(words)


_LANGS: dict[str, LangSpec] = {
    "python": LangSpec("python", _kw(
        "def", "class", "return", "if", "elif", "else", "for", "while",
        "import", "from", "as", "with", "try", "except", "finally", "raise",
        "lambda", "pass", "break", "continue", "global", "nonlocal", "yield",
        "assert", "del", "in", "is", "not", "and", "or", "async", "await",
        "None", "True", "False", "self",
    ), ("#",)),
    "javascript": LangSpec("javascript", _kw(
        "function", "const", "let", "var", "return", "if", "else", "for",
        "while", "do", "switch", "case", "break", "continue", "new", "typeof",
        "instanceof", "of", "class", "extends", "super", "this", "null",
        "undefined", "true", "false", "import", "export", "from", "default",
        "async", "await", "try", "catch", "finally", "throw", "static",
        "get", "set", "yield", "delete", "void",
    ), ("//",)),
    "go": LangSpec("go", _kw(
        "func", "package", "import", "var", "const", "type", "struct",
        "interface", "map", "chan", "go", "defer", "select", "switch", "case",
        "default", "if", "else", "for", "range", "return", "break",
        "continue", "fallthrough", "nil", "true", "false", "string", "int",
        "int64", "float64", "bool", "error",
    ), ("//",)),
    "rust": LangSpec("rust", _kw(
        "fn", "let", "mut", "const", "struct", "enum", "impl", "trait", "pub",
        "use", "mod", "match", "if", "else", "for", "while", "loop", "return",
        "break", "continue", "where", "async", "await", "move", "ref", "dyn",
        "crate", "self", "Self", "super", "true", "false", "Some", "None",
        "Ok", "Err",
    ), ("//",)),
    "java": LangSpec("java", _kw(
        "public", "private", "protected", "static", "final", "class",
        "interface", "extends", "implements", "return", "if", "else", "for",
        "while", "switch", "case", "break", "continue", "new", "this", "super",
        "try", "catch", "finally", "throw", "throws", "import", "package",
        "void", "int", "long", "double", "float", "boolean", "char", "byte",
        "short", "null", "true", "false",
    ), ("//",)),
    "bash": LangSpec("bash", _kw(
        "if", "then", "else", "elif", "fi", "for", "while", "do", "done",
        "case", "esac", "function", "return", "export", "local", "echo",
        "exit", "set", "unset", "shift", "in",
    ), ("#",)),
    "yaml": LangSpec("yaml", _kw(
        "true", "false", "null", "yes", "no", "on", "off",
    ), ("#",)),
    "sql": LangSpec("sql", _kw(
        "select", "from", "where", "insert", "into", "values", "update",
        "set", "delete", "create", "table", "drop", "alter", "join", "left",
        "right", "inner", "outer", "on", "group", "by", "order", "having",
        "limit", "as", "and", "or", "not", "null", "primary", "key",
        "foreign", "references", "distinct", "union", "all", "index",
    ), ("--",), case_insensitive=True),
}

# 别名 → 规范名
_ALIASES: dict[str, str] = {}
for canonical, spec in _LANGS.items():
    _ALIASES[canonical] = canonical
    _ALIASES[spec.name[:2]] = canonical  # 短前缀兜底
_ALIASES.update({
    "py": "python", "py3": "python",
    "js": "javascript", "node": "javascript", "ts": "javascript",
    "typescript": "javascript", "jsx": "javascript", "tsx": "javascript",
    "sh": "bash", "shell": "bash", "zsh": "bash", "console": "bash",
    "yml": "yaml",
    "golang": "go",
    "rs": "rust",
    "kotlin": "java", "kt": "java", "c": "java", "cpp": "java",
    "c++": "java", "cs": "java", "csharp": "java", "swift": "java",
})

# 兜底通用规则：无关键字，仅字符串 / 数字 / 常见注释
_GENERIC = LangSpec("generic", frozenset(), ("#", "//"))


def resolve_lang(info: str) -> LangSpec:
    """把围栏标记后的语言串映射为 LangSpec；未知语言走通用规则。"""
    key = info.strip().lower()
    canonical = _ALIASES.get(key) or _ALIASES.get(key.split("+")[0])
    return _LANGS.get(canonical, _GENERIC) if canonical else _GENERIC


def _token_re(spec: LangSpec) -> re.Pattern[str]:
    comment_alts = "|".join(
        re.escape(prefix) + r"[^\n]*" for prefix in spec.line_comments
    ) or r"(?!x)x"  # 永不匹配的占位
    return re.compile(
        r"(?P<comment>" + comment_alts + r")"
        r"|(?P<string>\"(?:\\.|[^\"\\\n])*\"|'(?:\\.|[^'\\\n])*'|`[^`\n]*`)"
        r"|(?P<number>\b\d+(?:\.\d+)?\b)"
        r"|(?P<ident>[A-Za-z_$][\w$]*)"
    )


_RE_CACHE: dict[str, re.Pattern[str]] = {}


def highlight_code(
    text: str,
    spec: LangSpec,
    *,
    set_format,   # Callable[[int, int, str], None]  (start, length, color)
) -> None:
    """对一行代码文本做着色提示；颜色经回调交由外部应用。"""
    regex = _RE_CACHE.setdefault(spec.name, _token_re(spec))
    kw = spec.keywords
    ci = spec.case_insensitive
    for m in regex.finditer(text):
        kind = m.lastgroup
        start, end = m.span()
        value = m.group()
        if kind == "comment":
            set_format(start, end - start, COLOR_COMMENT)
            continue
        if kind == "string":
            set_format(start, end - start, COLOR_STRING)
            continue
        if kind == "number":
            set_format(start, end - start, COLOR_NUMBER)
            continue
        # 标识符：关键字优先，其次「后随 (」视为函数调用
        probe = value.lower() if ci else value
        if probe in kw:
            set_format(start, end - start, COLOR_KEYWORD)
        elif text[m.end():].lstrip().startswith("("):
            set_format(start, end - start, COLOR_FUNC)
