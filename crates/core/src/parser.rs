use std::sync::OnceLock;

use regex::Regex;

use crate::{TaskState, TaskStateRegistry};

fn list_prefix_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^(?P<indent>\s*)(?P<bullet>(?:[-*+]|\d+[.)])[ \t]+)?")
            .expect("list prefix regex is valid")
    })
}

fn mark_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\[(?P<mark>[^\[\]]*)\]").expect("mark regex is valid"))
}

/// 一行任务 Markdown 的结构化表示。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskLineInfo {
    pub indent: String,
    pub bullet: Option<String>,
    pub mark: String,
    pub text: String,
    /// 以 Unicode scalar value 计数的 `[` 列位置，和 Python `str` 语义一致。
    pub mark_start: usize,
    /// 以 Unicode scalar value 计数的 `]` 后列位置。
    pub mark_end: usize,
}

impl TaskLineInfo {
    #[must_use]
    pub fn rebuild(&self, mark: &str) -> String {
        let prefix = format!("{}{}", self.indent, self.bullet.as_deref().unwrap_or("- "));
        if self.text.is_empty() {
            format!("{prefix}[{mark}]")
        } else {
            format!("{prefix}[{mark}] {}", self.text)
        }
    }
}

/// 匹配任何形似 checkbox 的行。未知 mark 是否为任务由 [`is_task_line`] 决定。
#[must_use]
pub fn match_task(line: &str) -> Option<TaskLineInfo> {
    let prefix = list_prefix_regex().captures(line)?;
    let prefix_end = prefix.get(0)?.end();
    let mark_match = mark_regex().captures(&line[prefix_end..])?;
    let complete_mark = mark_match.get(0)?;
    let mark = mark_match.name("mark")?.as_str().to_owned();
    let mark_start_byte = prefix_end + complete_mark.start();
    let mark_end_byte = prefix_end + complete_mark.end();

    Some(TaskLineInfo {
        indent: prefix.name("indent")?.as_str().to_owned(),
        bullet: prefix
            .name("bullet")
            .map(|bullet| bullet.as_str().to_owned()),
        mark,
        text: line[mark_end_byte..].trim_start().to_owned(),
        mark_start: line[..mark_start_byte].chars().count(),
        mark_end: line[..mark_end_byte].chars().count(),
    })
}

#[must_use]
pub fn is_task_line(line: &str, registry: &TaskStateRegistry) -> bool {
    match_task(line).is_some_and(|info| registry.by_mark(&info.mark).is_some())
}

/// 把任意文本行转换为任务行，保持已有列表符和缩进。
#[must_use]
pub fn make_task_line(line: &str, mark: &str) -> String {
    if let Some(info) = match_task(line) {
        return info.rebuild(mark);
    }

    let prefix = list_prefix_regex()
        .captures(line)
        .expect("anchored regex always matches");
    let prefix_match = prefix.get(0).expect("regex full match exists");
    if let Some(bullet) = prefix.name("bullet") {
        let content = line[prefix_match.end()..].trim();
        let indent = prefix.name("indent").map_or("", |matched| matched.as_str());
        if content.is_empty() {
            return format!("{indent}[{mark}]").replacen('[', &format!("{}[", bullet.as_str()), 1);
        }
        return format!("{indent}{}[{mark}] {content}", bullet.as_str());
    }

    let stripped = line.trim();
    if stripped.is_empty() {
        return format!("- [{mark}]");
    }
    let indent = prefix.name("indent").map_or("", |matched| matched.as_str());
    format!("{indent}- [{mark}] {stripped}")
}

/// 替换形似任务的 mark；未知 mark 也会被替换，保持与 Python 实现一致。
#[must_use]
pub fn set_task_mark(line: &str, mark: &str) -> String {
    match_task(line).map_or_else(|| line.to_owned(), |info| info.rebuild(mark))
}

/// 轮换已注册状态。未知 mark 或普通行保持原样。
#[must_use]
pub fn cycle_task_mark(line: &str, registry: &TaskStateRegistry, step: isize) -> (String, bool) {
    let Some(info) = match_task(line) else {
        return (line.to_owned(), false);
    };
    let Some(state) = registry.by_mark(&info.mark) else {
        return (line.to_owned(), false);
    };
    let Some(next) = registry.next_of(&state.state_id, step) else {
        return (line.to_owned(), false);
    };
    if next.mark == info.mark {
        return (line.to_owned(), false);
    }
    (info.rebuild(&next.mark), true)
}

/// 遍历文本中的已注册任务行。
pub fn iter_task_lines<'a>(
    text: &'a str,
    registry: &'a TaskStateRegistry,
) -> impl Iterator<Item = (usize, TaskLineInfo, &'a TaskState)> + 'a {
    text.lines().enumerate().filter_map(|(line_number, line)| {
        let info = match_task(line)?;
        let state = registry.by_mark(&info.mark)?;
        Some((line_number, info, state))
    })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::default_registry;

    use super::{cycle_task_mark, is_task_line, make_task_line, match_task, set_task_mark};

    #[derive(Deserialize)]
    struct TextCase {
        line: String,
        mark: String,
        expected: String,
    }

    #[derive(Deserialize)]
    struct CycleCase {
        line: String,
        step: isize,
        expected: String,
        changed: bool,
    }

    #[derive(Deserialize)]
    struct MatchCase {
        line: String,
        is_known_task: bool,
        mark: Option<String>,
        mark_start: Option<usize>,
        mark_end: Option<usize>,
    }

    #[derive(Deserialize)]
    struct SharedFixtures {
        make_task_line: Vec<TextCase>,
        set_task_mark: Vec<TextCase>,
        cycle_task_mark: Vec<CycleCase>,
        match_task: Vec<MatchCase>,
    }

    fn shared_fixtures() -> SharedFixtures {
        serde_json::from_str(include_str!("../../../tests/fixtures/task_rules.json"))
            .expect("shared task fixtures are valid JSON")
    }

    #[test]
    fn parses_python_supported_list_variants() {
        for line in ["- [~] JP", "* [x] CA", "+ [ ] A", "1. [ ] B", "2) [ ] C"] {
            assert!(match_task(line).is_some(), "{line}");
        }
    }

    #[test]
    fn preserves_indent_and_unicode_column_positions() {
        let parsed = match_task("    - [x] 嵌套").expect("task line");
        assert_eq!(parsed.indent, "    ");
        assert_eq!(parsed.bullet.as_deref(), Some("- "));
        assert_eq!(parsed.mark, "x");
        assert_eq!(parsed.text, "嵌套");
        assert_eq!(parsed.mark_start, 6);
        assert_eq!(parsed.mark_end, 9);
    }

    #[test]
    fn converts_plain_nested_and_empty_lines() {
        assert_eq!(make_task_line("US", " "), "- [ ] US");
        assert_eq!(make_task_line("  JP", " "), "  - [ ] JP");
        assert_eq!(make_task_line("- US", "~"), "- [~] US");
        assert_eq!(make_task_line("", " "), "- [ ]");
        assert_eq!(make_task_line("- [~] JP", " "), "- [ ] JP");
        assert_eq!(make_task_line("* JP", "x"), "* [x] JP");
    }

    #[test]
    fn cycles_without_touching_unknown_marks() {
        let registry = default_registry();
        assert_eq!(
            cycle_task_mark("- [ ] US", &registry, 1),
            ("- [~] US".to_owned(), true)
        );
        assert_eq!(
            cycle_task_mark("- [~] US", &registry, 1),
            ("- [x] US".to_owned(), true)
        );
        assert_eq!(
            cycle_task_mark("- [x] US", &registry, 1),
            ("- [ ] US".to_owned(), true)
        );
        assert_eq!(
            cycle_task_mark("- [ ] US", &registry, -1),
            ("- [x] US".to_owned(), true)
        );
        assert_eq!(
            cycle_task_mark("- [?] US", &registry, 1),
            ("- [?] US".to_owned(), false)
        );
        assert!(is_task_line("- [x] US", &registry));
        assert!(!is_task_line("- [?] US", &registry));
    }

    #[test]
    fn keeps_mark_replacement_contract() {
        assert_eq!(set_task_mark("- [ ] US", "x"), "- [x] US");
        assert_eq!(set_task_mark("ordinary", "x"), "ordinary");
    }

    #[test]
    fn conforms_to_shared_python_rust_fixtures() {
        let fixtures = shared_fixtures();
        let registry = default_registry();

        for case in fixtures.make_task_line {
            assert_eq!(
                make_task_line(&case.line, &case.mark),
                case.expected,
                "{}",
                case.line
            );
        }
        for case in fixtures.set_task_mark {
            assert_eq!(
                set_task_mark(&case.line, &case.mark),
                case.expected,
                "{}",
                case.line
            );
        }
        for case in fixtures.cycle_task_mark {
            assert_eq!(
                cycle_task_mark(&case.line, &registry, case.step),
                (case.expected, case.changed),
                "{}",
                case.line
            );
        }
        for case in fixtures.match_task {
            let parsed = match_task(&case.line);
            assert_eq!(
                parsed.as_ref().map(|info| &info.mark),
                case.mark.as_ref(),
                "{}",
                case.line
            );
            assert_eq!(
                parsed.as_ref().map(|info| info.mark_start),
                case.mark_start,
                "{}",
                case.line
            );
            assert_eq!(
                parsed.as_ref().map(|info| info.mark_end),
                case.mark_end,
                "{}",
                case.line
            );
            assert_eq!(
                is_task_line(&case.line, &registry),
                case.is_known_task,
                "{}",
                case.line
            );
        }
    }
}
