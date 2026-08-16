use super::editor::Editor;

pub(super) const CATEGORY_COUNT: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Home,
    Random,
    Formatters,
    Parsers,
    Validators,
    Codecs,
    Security,
    Vruno,
    Configuration,
}

impl Category {
    pub(super) const ALL: [Self; CATEGORY_COUNT] = [
        Self::Home,
        Self::Random,
        Self::Formatters,
        Self::Parsers,
        Self::Validators,
        Self::Codecs,
        Self::Security,
        Self::Vruno,
        Self::Configuration,
    ];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Random => "Random",
            Self::Formatters => "Formatters",
            Self::Parsers => "Parsers",
            Self::Validators => "Validators",
            Self::Codecs => "Codecs",
            Self::Security => "Security",
            Self::Vruno => "Vruno",
            Self::Configuration => "Configuration",
        }
    }

    pub(super) const fn tab_label(self, compact: bool, narrow: bool) -> &'static str {
        if !compact {
            return self.label();
        }
        match self {
            Self::Home => "Home",
            Self::Random if narrow => "Rand",
            Self::Random => "Random",
            Self::Formatters if narrow => "Fmt",
            Self::Formatters => "Format",
            Self::Parsers => "Parse",
            Self::Validators if narrow => "Val",
            Self::Validators => "Validate",
            Self::Codecs => "Codec",
            Self::Security => "Sec",
            Self::Vruno => "Vruno",
            Self::Configuration if narrow => "Cfg",
            Self::Configuration => "Config",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ToolDef {
    pub(super) category: Category,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) base: &'static [&'static str],
    pub(super) fields: &'static [FieldDef],
    pub(super) uses_input: bool,
    pub(super) sample: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub(super) struct FieldDef {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) help: &'static str,
    pub(super) kind: FieldKind,
    arg: FieldArg,
    condition: Option<Condition>,
}

impl FieldDef {
    pub(super) const fn is_secret(&self) -> bool {
        matches!(self.kind, FieldKind::Secret { .. })
    }
}

#[derive(Clone, Copy)]
pub(super) enum FieldKind {
    Choice {
        options: &'static [&'static str],
        default: usize,
    },
    Toggle {
        default: bool,
    },
    Text {
        default: &'static str,
        required: bool,
    },
    Secret {
        required: bool,
    },
    Number {
        default: u64,
        min: u64,
        max: u64,
        step: u64,
    },
}

#[derive(Clone, Copy)]
enum FieldArg {
    Flag(&'static str),
    FlagUnless {
        flag: &'static str,
        omitted: &'static [&'static str],
    },
    Positional,
    Positionals,
    ToggleFlag {
        flag: &'static str,
        when: bool,
    },
    None,
}

#[derive(Clone, Copy)]
struct Condition {
    field: &'static str,
    values: &'static [&'static str],
}

#[derive(Clone)]
pub(super) enum FieldState {
    Choice(usize),
    Toggle(bool),
    Text(Editor),
    Number(Editor),
}

impl FieldState {
    pub(super) fn from_def(definition: &FieldDef) -> Self {
        match definition.kind {
            FieldKind::Choice { default, .. } => Self::Choice(default),
            FieldKind::Toggle { default } => Self::Toggle(default),
            FieldKind::Text { default, .. } => Self::Text(Editor::from(default)),
            FieldKind::Secret { .. } => Self::Text(Editor::default()),
            FieldKind::Number { default, .. } => Self::Number(Editor::from(&default.to_string())),
        }
    }

    pub(super) fn value(&self, definition: &FieldDef) -> String {
        match (self, definition.kind) {
            (Self::Choice(index), FieldKind::Choice { options, .. }) => options[*index].into(),
            (Self::Toggle(value), _) => if *value { "yes" } else { "no" }.into(),
            (Self::Text(editor) | Self::Number(editor), _) => editor.value(),
            _ => String::new(),
        }
    }

    pub(super) fn display(&self, definition: &FieldDef) -> String {
        match (self, definition.kind) {
            (Self::Choice(index), FieldKind::Choice { options, .. }) => {
                format!("‹ {} ›", options[*index])
            }
            (Self::Toggle(value), _) => {
                format!(
                    "[{}] {}",
                    if *value { 'x' } else { ' ' },
                    if *value { "Yes" } else { "No" }
                )
            }
            (Self::Text(editor), FieldKind::Secret { .. }) if editor.is_empty() => {
                "(not set)".into()
            }
            (Self::Text(editor), FieldKind::Secret { .. }) => {
                "•".repeat(editor.value().chars().count())
            }
            (Self::Text(editor), _) if editor.is_empty() => "(empty)".into(),
            (Self::Text(editor) | Self::Number(editor), _) => editor.value(),
            _ => String::new(),
        }
    }

    pub(super) const fn is_editable(&self) -> bool {
        matches!(self, Self::Text(_) | Self::Number(_))
    }

    pub(super) const fn is_numeric(&self) -> bool {
        matches!(self, Self::Number(_))
    }

    pub(super) fn editor_mut(&mut self) -> Option<&mut Editor> {
        match self {
            Self::Text(editor) | Self::Number(editor) => Some(editor),
            Self::Choice(_) | Self::Toggle(_) => None,
        }
    }

    pub(super) fn editor(&self) -> Option<&Editor> {
        match self {
            Self::Text(editor) | Self::Number(editor) => Some(editor),
            Self::Choice(_) | Self::Toggle(_) => None,
        }
    }

    pub(super) fn replace_value(&mut self, definition: &FieldDef, value: &str) -> bool {
        match (self, definition.kind) {
            (Self::Choice(index), FieldKind::Choice { options, .. }) => {
                let Some(position) = options.iter().position(|option| *option == value) else {
                    return false;
                };
                *index = position;
                true
            }
            (Self::Text(editor) | Self::Number(editor), _) => {
                editor.replace(value);
                true
            }
            (Self::Toggle(current), FieldKind::Toggle { .. }) => match value {
                "yes" | "true" => {
                    *current = true;
                    true
                }
                "no" | "false" => {
                    *current = false;
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Debug)]
pub(super) struct BuildError {
    pub(super) field_index: usize,
    pub(super) message: String,
}

pub(super) fn new_form(tool: &ToolDef) -> Vec<FieldState> {
    tool.fields.iter().map(FieldState::from_def).collect()
}

pub(super) fn tools_in(category: Category) -> Vec<usize> {
    TOOLS
        .iter()
        .enumerate()
        .filter_map(|(index, tool)| (tool.category == category).then_some(index))
        .collect()
}

pub(super) fn tool_id(tool: &ToolDef) -> String {
    tool.base.join(".")
}

pub(super) fn find_tool(id: &str) -> Option<usize> {
    TOOLS.iter().position(|tool| tool_id(tool) == id)
}

pub(super) fn visible_fields(tool: &ToolDef, states: &[FieldState]) -> Vec<usize> {
    tool.fields
        .iter()
        .enumerate()
        .filter_map(|(index, field)| condition_matches(field, tool, states).then_some(index))
        .collect()
}

pub(super) fn adjust_field(
    definition: &FieldDef,
    state: &mut FieldState,
    direction: i8,
    large_step: bool,
) {
    match (state, definition.kind) {
        (FieldState::Choice(index), FieldKind::Choice { options, .. }) => {
            if direction < 0 {
                *index = index.checked_sub(1).unwrap_or(options.len() - 1);
            } else {
                *index = (*index + 1) % options.len();
            }
        }
        (FieldState::Toggle(value), FieldKind::Toggle { .. }) => *value = !*value,
        (FieldState::Number(editor), FieldKind::Number { min, max, step, .. }) => {
            let current = editor.value().parse::<u64>().unwrap_or(min).clamp(min, max);
            let amount = step.saturating_mul(if large_step { 10 } else { 1 });
            let next = if direction < 0 {
                current.saturating_sub(amount).max(min)
            } else {
                current.saturating_add(amount).min(max)
            };
            editor.replace(&next.to_string());
        }
        _ => {}
    }
}

pub(super) fn toggle_field(definition: &FieldDef, state: &mut FieldState) {
    match definition.kind {
        FieldKind::Toggle { .. } | FieldKind::Choice { .. } => {
            adjust_field(definition, state, 1, false);
        }
        FieldKind::Text { .. } | FieldKind::Secret { .. } | FieldKind::Number { .. } => {}
    }
}

pub(super) fn build_args(
    tool: &ToolDef,
    states: &[FieldState],
) -> std::result::Result<Vec<String>, BuildError> {
    let mut args: Vec<String> = tool.base.iter().map(|value| (*value).into()).collect();
    for (index, (definition, state)) in tool.fields.iter().zip(states).enumerate() {
        if !condition_matches(definition, tool, states) {
            continue;
        }
        let value = state.value(definition);
        match definition.kind {
            FieldKind::Text { required: true, .. } if value.trim().is_empty() => {
                return Err(BuildError {
                    field_index: index,
                    message: format!("{} is required", definition.label),
                });
            }
            FieldKind::Secret { required: true } if value.is_empty() => {
                return Err(BuildError {
                    field_index: index,
                    message: format!("{} is required", definition.label),
                });
            }
            FieldKind::Number { min, max, .. } => match value.parse::<u64>() {
                Ok(number) if (min..=max).contains(&number) => {}
                _ => {
                    return Err(BuildError {
                        field_index: index,
                        message: format!("{} must be between {min} and {max}", definition.label),
                    });
                }
            },
            _ => {}
        }

        match definition.arg {
            FieldArg::Flag(flag) if !value.trim().is_empty() => {
                args.push(flag.into());
                args.push(value);
            }
            FieldArg::FlagUnless { flag, omitted } if !omitted.contains(&value.as_str()) => {
                args.push(flag.into());
                args.push(value);
            }
            FieldArg::Positional if !value.trim().is_empty() => args.push(value),
            FieldArg::Positionals if !value.trim().is_empty() => {
                let values = shell_words::split(&value).map_err(|error| BuildError {
                    field_index: index,
                    message: format!("{} has invalid quoting: {error}", definition.label),
                })?;
                if values.is_empty() {
                    return Err(BuildError {
                        field_index: index,
                        message: format!("{} is required", definition.label),
                    });
                }
                args.extend(values);
            }
            FieldArg::ToggleFlag { flag, when } => {
                if matches!(state, FieldState::Toggle(value) if *value == when) {
                    args.push(flag.into());
                }
            }
            FieldArg::Flag(_)
            | FieldArg::FlagUnless { .. }
            | FieldArg::Positional
            | FieldArg::Positionals
            | FieldArg::None => {}
        }
    }
    Ok(args)
}

pub(super) fn command_preview(tool: &ToolDef, states: &[FieldState]) -> String {
    match build_args(tool, states) {
        Ok(args) => format!(
            "$ vutils {}",
            redact_args(&args)
                .iter()
                .map(|value| shell_quote(value))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Err(error) => format!("$ vutils {}  · {}", tool.base.join(" "), error.message),
    }
}

fn redact_args(args: &[String]) -> Vec<String> {
    const SECRET_FLAGS: &[&str] = &["--passwd", "--secret"];
    let mut redact_next = false;
    args.iter()
        .map(|argument| {
            if redact_next {
                redact_next = false;
                return "<redacted>".into();
            }
            redact_next = SECRET_FLAGS.contains(&argument.as_str());
            argument.clone()
        })
        .collect()
}

fn condition_matches(field: &FieldDef, tool: &ToolDef, states: &[FieldState]) -> bool {
    let Some(condition) = field.condition else {
        return true;
    };
    tool.fields
        .iter()
        .position(|candidate| candidate.key == condition.field)
        .is_some_and(|index| {
            let value = states[index].value(&tool.fields[index]);
            condition.values.contains(&value.as_str())
        })
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

const NO_FIELDS: &[FieldDef] = &[];
const YES: &[&str] = &["yes"];
const NO: &[&str] = &["no"];
const UUID_NAME_VERSIONS: &[&str] = &["v3", "v5"];
const UUID_NODE_VERSIONS: &[&str] = &["v1", "v2", "v6"];
const UUID_V2: &[&str] = &["v2"];
const UUID_V8: &[&str] = &["v8"];

const fn text_def(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    default: &'static str,
    required: bool,
    arg: FieldArg,
) -> FieldDef {
    FieldDef {
        key,
        label,
        help,
        kind: FieldKind::Text { default, required },
        arg,
        condition: None,
    }
}

const fn choice_def(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    options: &'static [&'static str],
    default: usize,
    arg: FieldArg,
) -> FieldDef {
    FieldDef {
        key,
        label,
        help,
        kind: FieldKind::Choice { options, default },
        arg,
        condition: None,
    }
}

const fn number_def(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    default: u64,
    min: u64,
    max: u64,
    arg: FieldArg,
) -> FieldDef {
    FieldDef {
        key,
        label,
        help,
        kind: FieldKind::Number {
            default,
            min,
            max,
            step: 1,
        },
        arg,
        condition: None,
    }
}

const fn toggle_def(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    default: bool,
    flag: &'static str,
) -> FieldDef {
    FieldDef {
        key,
        label,
        help,
        kind: FieldKind::Toggle { default },
        arg: FieldArg::ToggleFlag { flag, when: true },
        condition: None,
    }
}

const JSON_PATH_FIELDS: &[FieldDef] = &[FieldDef {
    key: "path",
    label: "Path",
    help: "JSON path expression, for example $.user.id",
    kind: FieldKind::Text {
        default: "$.name",
        required: true,
    },
    arg: FieldArg::Positional,
    condition: None,
}];

const JSON_CSV_FIELDS: &[FieldDef] = &[FieldDef {
    key: "stringify",
    label: "Nested as JSON",
    help: "Serialize nested arrays and objects instead of rejecting them",
    kind: FieldKind::Toggle { default: false },
    arg: FieldArg::ToggleFlag {
        flag: "--stringify-nested",
        when: true,
    },
    condition: None,
}];

const JSON_DIFF_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "left",
        label: "Left JSON",
        help: "Original JSON document",
        kind: FieldKind::Text {
            default: r#"{"active":false}"#,
            required: true,
        },
        arg: FieldArg::Flag("--left"),
        condition: None,
    },
    FieldDef {
        key: "right",
        label: "Right JSON",
        help: "JSON document to compare against",
        kind: FieldKind::Text {
            default: r#"{"active":true}"#,
            required: true,
        },
        arg: FieldArg::Flag("--right"),
        condition: None,
    },
    FieldDef {
        key: "patch",
        label: "JSON patch",
        help: "Return machine-readable JSON Patch operations",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--patch",
            when: true,
        },
        condition: None,
    },
];

const CASE_OPTIONS: &[&str] = &["camel", "pascal", "snake", "kebab", "constant", "title"];
const TEXT_CASE_FIELDS: &[FieldDef] = &[FieldDef {
    key: "style",
    label: "Target style",
    help: "Use ←/→ to choose the naming convention",
    kind: FieldKind::Choice {
        options: CASE_OPTIONS,
        default: 0,
    },
    arg: FieldArg::Positional,
    condition: None,
}];

const TEXT_SORT_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "unique",
        label: "Unique",
        help: "Remove duplicate lines after sorting",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--unique",
            when: true,
        },
        condition: None,
    },
    FieldDef {
        key: "descending",
        label: "Descending",
        help: "Reverse the sort order",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--descending",
            when: true,
        },
        condition: None,
    },
];

const REGEX_TEST_FIELDS: &[FieldDef] = &[FieldDef {
    key: "pattern",
    label: "Pattern",
    help: "Rust-compatible regular expression",
    kind: FieldKind::Text {
        default: "[A-Za-z]+",
        required: true,
    },
    arg: FieldArg::Positional,
    condition: None,
}];

const REGEX_REPLACE_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "pattern",
        label: "Pattern",
        help: "Rust-compatible regular expression",
        kind: FieldKind::Text {
            default: "[0-9]+",
            required: true,
        },
        arg: FieldArg::Positional,
        condition: None,
    },
    FieldDef {
        key: "replacement",
        label: "Replacement",
        help: "Replacement text; capture groups are supported",
        kind: FieldKind::Text {
            default: "#",
            required: false,
        },
        arg: FieldArg::Positional,
        condition: None,
    },
    FieldDef {
        key: "first",
        label: "First only",
        help: "Replace only the first match",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--first-only",
            when: true,
        },
        condition: None,
    },
];

const BASE64_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "url_safe",
        label: "URL safe",
        help: "Use the URL-safe Base64 alphabet",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--url-safe",
            when: true,
        },
        condition: None,
    },
    FieldDef {
        key: "padding",
        label: "Padding",
        help: "Include trailing = padding",
        kind: FieldKind::Toggle { default: true },
        arg: FieldArg::ToggleFlag {
            flag: "--no-padding",
            when: false,
        },
        condition: None,
    },
];

const HEX_FIELDS: &[FieldDef] = &[FieldDef {
    key: "uppercase",
    label: "Uppercase",
    help: "Use A-F instead of a-f",
    kind: FieldKind::Toggle { default: false },
    arg: FieldArg::ToggleFlag {
        flag: "--uppercase",
        when: true,
    },
    condition: None,
}];

const URL_FIELDS: &[FieldDef] = &[FieldDef {
    key: "form",
    label: "Form encoding",
    help: "Encode spaces as + using application/x-www-form-urlencoded rules",
    kind: FieldKind::Toggle { default: false },
    arg: FieldArg::ToggleFlag {
        flag: "--form",
        when: true,
    },
    condition: None,
}];

const UUID_VERSIONS: &[&str] = &["v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8"];
const UUID_FORMATS: &[&str] = &["hyphenated", "simple", "urn", "braced"];
const UUID_NAMESPACES: &[&str] = &["dns", "url", "oid", "x500"];
const UUID_DOMAINS: &[&str] = &["person", "group", "organization"];
const UUID_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "version",
        label: "Version",
        help: "v7 is recommended. Plot twist: v3/v5 are deterministic, not random 😄",
        kind: FieldKind::Choice {
            options: UUID_VERSIONS,
            default: 6,
        },
        arg: FieldArg::Flag("--version"),
        condition: None,
    },
    FieldDef {
        key: "count",
        label: "Quantity",
        help: "Number of UUIDs to generate",
        kind: FieldKind::Number {
            default: 1,
            min: 1,
            max: 100_000,
            step: 1,
        },
        arg: FieldArg::Flag("--count"),
        condition: None,
    },
    FieldDef {
        key: "format",
        label: "Format",
        help: "Text representation of the generated UUID",
        kind: FieldKind::Choice {
            options: UUID_FORMATS,
            default: 0,
        },
        arg: FieldArg::Flag("--format"),
        condition: None,
    },
    FieldDef {
        key: "namespace",
        label: "Namespace",
        help: "Namespace used by deterministic UUID v3/v5",
        kind: FieldKind::Choice {
            options: UUID_NAMESPACES,
            default: 0,
        },
        arg: FieldArg::Flag("--namespace"),
        condition: Some(Condition {
            field: "version",
            values: UUID_NAME_VERSIONS,
        }),
    },
    FieldDef {
        key: "name",
        label: "Name",
        help: "Stable name hashed into deterministic UUID v3/v5",
        kind: FieldKind::Text {
            default: "api.example.com",
            required: true,
        },
        arg: FieldArg::Flag("--name"),
        condition: Some(Condition {
            field: "version",
            values: UUID_NAME_VERSIONS,
        }),
    },
    FieldDef {
        key: "domain",
        label: "DCE domain",
        help: "DCE security domain required by UUID v2",
        kind: FieldKind::Choice {
            options: UUID_DOMAINS,
            default: 0,
        },
        arg: FieldArg::Flag("--domain"),
        condition: Some(Condition {
            field: "version",
            values: UUID_V2,
        }),
    },
    FieldDef {
        key: "local_id",
        label: "Local ID",
        help: "User/group/organization identifier embedded by UUID v2",
        kind: FieldKind::Number {
            default: 1000,
            min: 0,
            max: u32::MAX as u64,
            step: 1,
        },
        arg: FieldArg::Flag("--local-id"),
        condition: Some(Condition {
            field: "version",
            values: UUID_V2,
        }),
    },
    FieldDef {
        key: "custom",
        label: "Custom bytes",
        help: "Exactly 32 hexadecimal digits required by UUID v8",
        kind: FieldKind::Text {
            default: "00112233445566778899aabbccddeeff",
            required: true,
        },
        arg: FieldArg::Flag("--custom-bytes"),
        condition: Some(Condition {
            field: "version",
            values: UUID_V8,
        }),
    },
    FieldDef {
        key: "node",
        label: "Node ID",
        help: "Optional 12-digit hexadecimal node; random when empty",
        kind: FieldKind::Text {
            default: "",
            required: false,
        },
        arg: FieldArg::Flag("--node-id"),
        condition: Some(Condition {
            field: "version",
            values: UUID_NODE_VERSIONS,
        }),
    },
];

const PASSWORD_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "length",
        label: "Length",
        help: "Password length from 4 to 4096 characters",
        kind: FieldKind::Number {
            default: 20,
            min: 4,
            max: 4096,
            step: 1,
        },
        arg: FieldArg::Flag("--length"),
        condition: None,
    },
    FieldDef {
        key: "count",
        label: "Quantity",
        help: "Number of passwords to generate",
        kind: FieldKind::Number {
            default: 1,
            min: 1,
            max: 100_000,
            step: 1,
        },
        arg: FieldArg::Flag("--count"),
        condition: None,
    },
    FieldDef {
        key: "symbols",
        label: "Special chars",
        help: "Include at least one symbol such as !, @ or #",
        kind: FieldKind::Toggle { default: true },
        arg: FieldArg::ToggleFlag {
            flag: "--no-symbols",
            when: false,
        },
        condition: None,
    },
    FieldDef {
        key: "ambiguous",
        label: "Exclude 0O1lI",
        help: "Avoid characters that are easy to confuse visually",
        kind: FieldKind::Toggle { default: true },
        arg: FieldArg::ToggleFlag {
            flag: "--exclude-ambiguous",
            when: true,
        },
        condition: None,
    },
];

const TOKEN_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "length",
        label: "Length",
        help: "Token length from 1 to 4096 characters",
        kind: FieldKind::Number {
            default: 32,
            min: 1,
            max: 4096,
            step: 1,
        },
        arg: FieldArg::Flag("--length"),
        condition: None,
    },
    FieldDef {
        key: "count",
        label: "Quantity",
        help: "Number of tokens to generate",
        kind: FieldKind::Number {
            default: 1,
            min: 1,
            max: 100_000,
            step: 1,
        },
        arg: FieldArg::Flag("--count"),
        condition: None,
    },
    FieldDef {
        key: "alphabet",
        label: "Alphabet",
        help: "Optional custom character alphabet",
        kind: FieldKind::Text {
            default: "",
            required: false,
        },
        arg: FieldArg::Flag("--alphabet"),
        condition: None,
    },
];

const COUNT_FIELDS: &[FieldDef] = &[FieldDef {
    key: "count",
    label: "Quantity",
    help: "Number of values to generate",
    kind: FieldKind::Number {
        default: 1,
        min: 1,
        max: 100_000,
        step: 1,
    },
    arg: FieldArg::Flag("--count"),
    condition: None,
}];

const NANOID_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "length",
        label: "Length",
        help: "NanoID length from 1 to 1024 characters",
        kind: FieldKind::Number {
            default: 21,
            min: 1,
            max: 1024,
            step: 1,
        },
        arg: FieldArg::Flag("--length"),
        condition: None,
    },
    COUNT_FIELDS[0],
];

const BR_KINDS: &[&str] = &["random", "cpf", "cnpj", "email", "phone"];
const BR_PIX_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "kind",
        label: "PIX kind",
        help: "Type of synthetic PIX key",
        kind: FieldKind::Choice {
            options: BR_KINDS,
            default: 0,
        },
        arg: FieldArg::Flag("--kind"),
        condition: None,
    },
    COUNT_FIELDS[0],
];

const HASH_OPTIONS: &[&str] = &["sha256", "sha512"];
const HASH_FIELDS: &[FieldDef] = &[FieldDef {
    key: "algorithm",
    label: "Algorithm",
    help: "Choose SHA-256 or SHA-512",
    kind: FieldKind::Choice {
        options: HASH_OPTIONS,
        default: 0,
    },
    arg: FieldArg::Positional,
    condition: None,
}];

const TOTP_SECRET_FIELDS: &[FieldDef] = &[FieldDef {
    key: "bytes",
    label: "Secret bytes",
    help: "Entropy size for the generated Base32 secret",
    kind: FieldKind::Number {
        default: 20,
        min: 16,
        max: 128,
        step: 1,
    },
    arg: FieldArg::Flag("--bytes"),
    condition: None,
}];

const SQL_DIALECTS: &[&str] = &["generic", "postgres", "mysql", "sqlite", "mssql"];
const KEYWORD_CASES: &[&str] = &["upper", "lower", "preserve"];
const SQL_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "dialect",
        label: "Dialect",
        help: "SQL syntax used for parsing and formatting",
        kind: FieldKind::Choice {
            options: SQL_DIALECTS,
            default: 1,
        },
        arg: FieldArg::Flag("--dialect"),
        condition: None,
    },
    FieldDef {
        key: "keywords",
        label: "Keywords",
        help: "Keyword casing in formatted SQL",
        kind: FieldKind::Choice {
            options: KEYWORD_CASES,
            default: 0,
        },
        arg: FieldArg::Flag("--keyword-case"),
        condition: None,
    },
    FieldDef {
        key: "indent",
        label: "Indent",
        help: "Spaces per indentation level",
        kind: FieldKind::Number {
            default: 2,
            min: 0,
            max: 16,
            step: 1,
        },
        arg: FieldArg::Flag("--indent"),
        condition: None,
    },
];

const CODE_LANGUAGES: &[&str] = &["rust", "kotlin", "csharp", "typescript"];
const CODE_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "language",
        label: "Language",
        help: "Target language for generated models",
        kind: FieldKind::Choice {
            options: CODE_LANGUAGES,
            default: 3,
        },
        arg: FieldArg::Flag("--lang"),
        condition: None,
    },
    FieldDef {
        key: "name",
        label: "Root type",
        help: "Name of the generated root model",
        kind: FieldKind::Text {
            default: "Root",
            required: true,
        },
        arg: FieldArg::Flag("--name"),
        condition: None,
    },
];

const CRON_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "expression",
        label: "Expression",
        help: "Seven-field cron expression including seconds and year",
        kind: FieldKind::Text {
            default: "0 0 9 * * MON-FRI *",
            required: true,
        },
        arg: FieldArg::Positional,
        condition: None,
    },
    FieldDef {
        key: "count",
        label: "Occurrences",
        help: "How many future occurrences to show",
        kind: FieldKind::Number {
            default: 5,
            min: 1,
            max: 1000,
            step: 1,
        },
        arg: FieldArg::Flag("--count"),
        condition: None,
    },
    FieldDef {
        key: "utc",
        label: "UTC",
        help: "Return timestamps in UTC",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--utc",
            when: true,
        },
        condition: None,
    },
];

const TIME_UNITS: &[&str] = &["seconds", "milliseconds"];
const TIME_NOW_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "unix",
        label: "Unix output",
        help: "Return a numeric Unix timestamp",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--unix",
            when: true,
        },
        condition: None,
    },
    FieldDef {
        key: "unit",
        label: "Unit",
        help: "Unix timestamp precision",
        kind: FieldKind::Choice {
            options: TIME_UNITS,
            default: 0,
        },
        arg: FieldArg::Flag("--unit"),
        condition: Some(Condition {
            field: "unix",
            values: YES,
        }),
    },
    FieldDef {
        key: "utc",
        label: "UTC",
        help: "Format RFC 3339 output in UTC",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--utc",
            when: true,
        },
        condition: Some(Condition {
            field: "unix",
            values: NO,
        }),
    },
];

const NUMBER_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "value",
        label: "Value",
        help: "Integer to convert",
        kind: FieldKind::Text {
            default: "ff",
            required: true,
        },
        arg: FieldArg::Positional,
        condition: None,
    },
    FieldDef {
        key: "from",
        label: "From base",
        help: "Source base from 2 to 36",
        kind: FieldKind::Number {
            default: 16,
            min: 2,
            max: 36,
            step: 1,
        },
        arg: FieldArg::Flag("--from"),
        condition: None,
    },
    FieldDef {
        key: "to",
        label: "To base",
        help: "Destination base from 2 to 36",
        kind: FieldKind::Number {
            default: 10,
            min: 2,
            max: 36,
            step: 1,
        },
        arg: FieldArg::Flag("--to"),
        condition: None,
    },
];

const BYTES_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "value",
        label: "Bytes",
        help: "Raw byte count",
        kind: FieldKind::Number {
            default: 1_048_576,
            min: 0,
            max: u64::MAX,
            step: 1024,
        },
        arg: FieldArg::Positional,
        condition: None,
    },
    FieldDef {
        key: "iec",
        label: "IEC units",
        help: "Use KiB/MiB instead of kB/MB",
        kind: FieldKind::Toggle { default: true },
        arg: FieldArg::ToggleFlag {
            flag: "--iec",
            when: true,
        },
        condition: None,
    },
    FieldDef {
        key: "precision",
        label: "Precision",
        help: "Decimal places in the formatted result",
        kind: FieldKind::Number {
            default: 2,
            min: 0,
            max: 20,
            step: 1,
        },
        arg: FieldArg::Flag("--precision"),
        condition: None,
    },
];

const VRUNO_OUTPUT_FORMATS: &[&str] = &["text", "json"];
const VRUNO_GROUPS: &[&str] = &["tags", "path"];
const VRUNO_RUN_FIELDS: &[FieldDef] = &[
    text_def(
        "collection",
        "Collection",
        "Bruno collection directory; prefilled from configuration",
        "",
        false,
        FieldArg::Flag("--collection"),
    ),
    text_def(
        "openapi",
        "OpenAPI file",
        "Local OpenAPI file; prefilled from configuration",
        "",
        false,
        FieldArg::Flag("--openapi"),
    ),
    choice_def(
        "group_by",
        "Group by",
        "Create request folders from OpenAPI tags or URL paths",
        VRUNO_GROUPS,
        0,
        FieldArg::Flag("--group-by"),
    ),
];
const VRUNO_CONFIGURE_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "collection",
        label: "Collection",
        help: "Directory containing the Bruno collection to update",
        kind: FieldKind::Text {
            default: ".",
            required: true,
        },
        arg: FieldArg::Flag("--collection"),
        condition: None,
    },
    FieldDef {
        key: "openapi",
        label: "OpenAPI file",
        help: "Local OpenAPI 3.x file ending in .json, .yaml, or .yml",
        kind: FieldKind::Text {
            default: "openapi.yaml",
            required: true,
        },
        arg: FieldArg::Flag("--openapi"),
        condition: None,
    },
];
const VRUNO_CHECK_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "format",
        label: "Report format",
        help: "Human-readable text or machine-readable JSON",
        kind: FieldKind::Choice {
            options: VRUNO_OUTPUT_FORMATS,
            default: 0,
        },
        arg: FieldArg::Flag("--format"),
        condition: None,
    },
    VRUNO_RUN_FIELDS[0],
    VRUNO_RUN_FIELDS[1],
    VRUNO_RUN_FIELDS[2],
];
const VRUNO_PREVIEW_FIELDS: &[FieldDef] = VRUNO_RUN_FIELDS;
const VRUNO_SYNC_FIELDS: &[FieldDef] = &[
    VRUNO_RUN_FIELDS[0],
    VRUNO_RUN_FIELDS[1],
    VRUNO_RUN_FIELDS[2],
    FieldDef {
        key: "confirm",
        label: "Write changes",
        help: "Required confirmation: create and update collection files",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--yes",
            when: true,
        },
        condition: None,
    },
];

const CRYPTO_ALGORITHMS: &[&str] = &["xchacha20-poly1305", "aes-256-gcm"];
const DECRYPT_ALGORITHMS: &[&str] = &["auto", "xchacha20-poly1305", "aes-256-gcm"];
const AUTO_VALUE: &[&str] = &["auto"];
const PASSWORD_SOURCES: &[&str] = &["configured", "direct", "environment", "file"];
const PASSWORD_DIRECT: &[&str] = &["direct"];
const PASSWORD_ENVIRONMENT: &[&str] = &["environment"];
const PASSWORD_FILE: &[&str] = &["file"];
const PASSWORD_SOURCE_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "password_source",
        label: "Password source",
        help: "Use configured crypto settings, a masked value, an environment variable, or a file",
        kind: FieldKind::Choice {
            options: PASSWORD_SOURCES,
            default: 0,
        },
        arg: FieldArg::None,
        condition: None,
    },
    FieldDef {
        key: "password",
        label: "Password",
        help: "Masked in the form and command preview; never persisted by the TUI",
        kind: FieldKind::Secret { required: true },
        arg: FieldArg::Flag("--passwd"),
        condition: Some(Condition {
            field: "password_source",
            values: PASSWORD_DIRECT,
        }),
    },
    FieldDef {
        key: "password_env",
        label: "Environment",
        help: "Variable name only; its secret passphrase or text automates enc and dec",
        kind: FieldKind::Text {
            default: "VUTILS_PASSWORD",
            required: true,
        },
        arg: FieldArg::Flag("--passwd-env"),
        condition: Some(Condition {
            field: "password_source",
            values: PASSWORD_ENVIRONMENT,
        }),
    },
    FieldDef {
        key: "password_file",
        label: "Password file",
        help: "Local file containing the password",
        kind: FieldKind::Text {
            default: "password.txt",
            required: true,
        },
        arg: FieldArg::Flag("--passwd-file"),
        condition: Some(Condition {
            field: "password_source",
            values: PASSWORD_FILE,
        }),
    },
];
const ENCRYPT_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "algorithm",
        label: "Algorithm",
        help: "Authenticated-encryption algorithm; prefilled from crypto.algorithm",
        kind: FieldKind::Choice {
            options: CRYPTO_ALGORITHMS,
            default: 0,
        },
        arg: FieldArg::Flag("--alg"),
        condition: None,
    },
    PASSWORD_SOURCE_FIELDS[0],
    PASSWORD_SOURCE_FIELDS[1],
    PASSWORD_SOURCE_FIELDS[2],
    PASSWORD_SOURCE_FIELDS[3],
];
const DECRYPT_FIELDS: &[FieldDef] = &[
    FieldDef {
        key: "algorithm",
        label: "Algorithm",
        help: "Auto reads the algorithm from the encrypted envelope",
        kind: FieldKind::Choice {
            options: DECRYPT_ALGORITHMS,
            default: 0,
        },
        arg: FieldArg::FlagUnless {
            flag: "--alg",
            omitted: AUTO_VALUE,
        },
        condition: None,
    },
    PASSWORD_SOURCE_FIELDS[0],
    PASSWORD_SOURCE_FIELDS[1],
    PASSWORD_SOURCE_FIELDS[2],
    PASSWORD_SOURCE_FIELDS[3],
];
const CONFIG_KEYS: &[&str] = &[
    "sql.dialect",
    "uuid.version",
    "uuid.format",
    "crypto.algorithm",
    "crypto.password-env",
    "crypto.password-file",
    "tui.home",
    "vruno.collection",
    "vruno.openapi",
];

const fn config_choice_field(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    options: &'static [&'static str],
    default: usize,
) -> FieldDef {
    FieldDef {
        key,
        label,
        help,
        kind: FieldKind::Choice { options, default },
        arg: FieldArg::Positional,
        condition: None,
    }
}

const fn config_text_field(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    default: &'static str,
) -> FieldDef {
    FieldDef {
        key,
        label,
        help,
        kind: FieldKind::Text {
            default,
            required: true,
        },
        arg: FieldArg::Positional,
        condition: None,
    }
}

const CONFIG_SQL_FIELDS: &[FieldDef] = &[config_choice_field(
    "value",
    "Dialect",
    "Default dialect used by SQL commands",
    SQL_DIALECTS,
    0,
)];
const CONFIG_UUID_VERSION_FIELDS: &[FieldDef] = &[config_choice_field(
    "value",
    "Version",
    "Default UUID version; v7 is recommended for new backend identifiers",
    UUID_VERSIONS,
    6,
)];
const CONFIG_UUID_FORMAT_FIELDS: &[FieldDef] = &[config_choice_field(
    "value",
    "Format",
    "Default textual UUID representation",
    UUID_FORMATS,
    0,
)];
const CONFIG_CRYPTO_FIELDS: &[FieldDef] = &[config_choice_field(
    "value",
    "Algorithm",
    "Default authenticated-encryption algorithm",
    CRYPTO_ALGORITHMS,
    0,
)];
const CONFIG_PASSWORD_ENV_FIELDS: &[FieldDef] = &[config_text_field(
    "value",
    "Environment",
    "Only the variable name is saved; its secret phrase/text automates enc/dec. Clears Password file",
    "VUTILS_PASSWORD",
)];
const CONFIG_PASSWORD_FILE_FIELDS: &[FieldDef] = &[config_text_field(
    "value",
    "Password file",
    "Password file path; setting it clears Password environment",
    "password.txt",
)];
const CONFIG_HOME_FIELDS: &[FieldDef] = &[config_text_field(
    "value",
    "Operation IDs",
    "Comma-separated Home operation IDs, in display order",
    "json.pretty,uuid,gen.password,enc,dec,sql.format",
)];
const CONFIG_VRUNO_COLLECTION_FIELDS: &[FieldDef] = &[config_text_field(
    "value",
    "Collection",
    "Default Bruno collection directory used by Vruno",
    ".",
)];
const CONFIG_VRUNO_OPENAPI_FIELDS: &[FieldDef] = &[config_text_field(
    "value",
    "OpenAPI file",
    "Default local OpenAPI JSON or YAML file used by Vruno",
    "openapi.yaml",
)];
const CONFIG_RESET_FIELDS: &[FieldDef] = &[config_choice_field(
    "key",
    "Setting",
    "Restore a built-in default or clear an optional setting",
    CONFIG_KEYS,
    0,
)];

const GEN_EMAIL_FIELDS: &[FieldDef] = &[
    text_def(
        "domain",
        "Domain",
        "Domain used by generated addresses",
        "example.com",
        true,
        FieldArg::Flag("--domain"),
    ),
    COUNT_FIELDS[0],
];
const GEN_LOREM_FIELDS: &[FieldDef] = &[number_def(
    "words",
    "Words",
    "Number of lorem-ipsum words",
    24,
    1,
    100_000,
    FieldArg::Flag("--words"),
)];
const GENERATE_MODE: &[&str] = &["generate"];
const VALIDATE_MODE: &[&str] = &["validate"];
const BR_DOCUMENT_MODES: &[&str] = &["generate", "validate"];
const BR_DOCUMENT_FIELDS: &[FieldDef] = &[
    choice_def(
        "mode",
        "Mode",
        "Generate fixtures or validate one existing document",
        BR_DOCUMENT_MODES,
        0,
        FieldArg::None,
    ),
    FieldDef {
        key: "value",
        label: "Document",
        help: "CPF or CNPJ value to validate",
        kind: FieldKind::Text {
            default: "",
            required: true,
        },
        arg: FieldArg::Flag("--validate"),
        condition: Some(Condition {
            field: "mode",
            values: VALIDATE_MODE,
        }),
    },
    FieldDef {
        condition: Some(Condition {
            field: "mode",
            values: GENERATE_MODE,
        }),
        ..COUNT_FIELDS[0]
    },
    FieldDef {
        key: "formatted",
        label: "Formatted",
        help: "Include punctuation in generated documents",
        kind: FieldKind::Toggle { default: false },
        arg: FieldArg::ToggleFlag {
            flag: "--formatted",
            when: true,
        },
        condition: Some(Condition {
            field: "mode",
            values: GENERATE_MODE,
        }),
    },
];
const BR_FIXTURE_FIELDS: &[FieldDef] = &[
    COUNT_FIELDS[0],
    toggle_def(
        "formatted",
        "Formatted",
        "Include punctuation in generated values",
        false,
        "--formatted",
    ),
];
const BINARY_ENCODE_FIELDS: &[FieldDef] = &[toggle_def(
    "spaced",
    "Separate bytes",
    "Separate each encoded byte with a space",
    false,
    "--spaced",
)];
const GZIP_FIELDS: &[FieldDef] = &[number_def(
    "level",
    "Level",
    "Compression level from 0 to 9",
    6,
    0,
    9,
    FieldArg::Flag("--level"),
)];
const CURL_SHELLS: &[&str] = &["posix", "powershell"];
const CURL_FIELDS: &[FieldDef] = &[choice_def(
    "shell",
    "Shell",
    "Target quoting rules",
    CURL_SHELLS,
    0,
    FieldArg::Flag("--shell"),
)];
const JSON_SCHEMA_FIELDS: &[FieldDef] = &[text_def(
    "schema",
    "Schema file",
    "Local JSON Schema file",
    "schema.json",
    true,
    FieldArg::Flag("--schema"),
)];
const YAML_SPLIT_FIELDS: &[FieldDef] = &[text_def(
    "output_dir",
    "Output directory",
    "Optional directory for split documents; empty prints them",
    "",
    false,
    FieldArg::Flag("--output-dir"),
)];
const FILE_LIST_FIELDS: &[FieldDef] = &[text_def(
    "files",
    "Files",
    "Space-separated file paths; quote paths containing spaces",
    "first.yaml second.yaml",
    true,
    FieldArg::Positionals,
)];
const TEXT_DIFF_FIELDS: &[FieldDef] = &[
    text_def(
        "left",
        "Left text",
        "Original text",
        "before",
        true,
        FieldArg::Flag("--left"),
    ),
    text_def(
        "right",
        "Right text",
        "Text to compare against",
        "after",
        true,
        FieldArg::Flag("--right"),
    ),
];
const DOTENV_DIFF_FIELDS: &[FieldDef] = &[
    text_def(
        "left",
        "Left dotenv",
        "Original dotenv content",
        "PORT=8080",
        true,
        FieldArg::Flag("--left"),
    ),
    text_def(
        "right",
        "Right dotenv",
        "Dotenv content to compare against",
        "PORT=3000",
        true,
        FieldArg::Flag("--right"),
    ),
    toggle_def(
        "show_values",
        "Show values",
        "Include potentially sensitive dotenv values",
        false,
        "--show-values",
    ),
];
const NORMALIZE_EOL_FIELDS: &[FieldDef] = &[toggle_def(
    "crlf",
    "Windows CRLF",
    "Use CRLF instead of Unix LF line endings",
    false,
    "--crlf",
)];
const ESCAPE_LANGUAGES: &[&str] = &[
    "json",
    "rust",
    "kotlin",
    "java",
    "csharp",
    "javascript",
    "typescript",
    "python",
    "sql",
    "posix-shell",
];
const STRING_FIELDS: &[FieldDef] = &[choice_def(
    "language",
    "Language",
    "String-literal escaping rules",
    ESCAPE_LANGUAGES,
    0,
    FieldArg::Flag("--language"),
)];
const BYTES_PARSE_FIELDS: &[FieldDef] = &[text_def(
    "value",
    "Size",
    "Human-readable byte size such as 1.5 MiB",
    "1.5 MiB",
    true,
    FieldArg::Positional,
)];

const SECRET_SOURCES: &[&str] = &["direct", "environment", "file"];
const SECRET_DIRECT: &[&str] = &["direct"];
const SECRET_ENVIRONMENT: &[&str] = &["environment"];
const SECRET_FILE: &[&str] = &["file"];
const SECRET_SOURCE_FIELDS: &[FieldDef] = &[
    choice_def(
        "secret_source",
        "Secret source",
        "Use a masked value, environment variable, or file",
        SECRET_SOURCES,
        0,
        FieldArg::None,
    ),
    FieldDef {
        key: "secret",
        label: "Secret",
        help: "Masked in the form and command preview",
        kind: FieldKind::Secret { required: true },
        arg: FieldArg::Flag("--secret"),
        condition: Some(Condition {
            field: "secret_source",
            values: SECRET_DIRECT,
        }),
    },
    FieldDef {
        key: "secret_env",
        label: "Environment",
        help: "Environment variable containing the secret",
        kind: FieldKind::Text {
            default: "VUTILS_SECRET",
            required: true,
        },
        arg: FieldArg::Flag("--secret-env"),
        condition: Some(Condition {
            field: "secret_source",
            values: SECRET_ENVIRONMENT,
        }),
    },
    FieldDef {
        key: "secret_file",
        label: "Secret file",
        help: "Local file containing the secret",
        kind: FieldKind::Text {
            default: "secret.txt",
            required: true,
        },
        arg: FieldArg::Flag("--secret-file"),
        condition: Some(Condition {
            field: "secret_source",
            values: SECRET_FILE,
        }),
    },
];
const HMAC_FIELDS: &[FieldDef] = &[
    choice_def(
        "algorithm",
        "Algorithm",
        "HMAC hash algorithm",
        HASH_OPTIONS,
        0,
        FieldArg::Flag("--algorithm"),
    ),
    SECRET_SOURCE_FIELDS[0],
    SECRET_SOURCE_FIELDS[1],
    SECRET_SOURCE_FIELDS[2],
    SECRET_SOURCE_FIELDS[3],
];
const PASSWORD_VERIFY_FIELDS: &[FieldDef] = &[
    text_def(
        "encoded",
        "Encoded hash",
        "Existing Argon2 or bcrypt hash",
        "$argon2id$...",
        true,
        FieldArg::Positional,
    ),
    SECRET_SOURCE_FIELDS[0],
    SECRET_SOURCE_FIELDS[1],
    SECRET_SOURCE_FIELDS[2],
    SECRET_SOURCE_FIELDS[3],
];
const BCRYPT_HASH_FIELDS: &[FieldDef] = &[
    number_def(
        "cost",
        "Cost",
        "bcrypt work factor from 4 to 31",
        12,
        4,
        31,
        FieldArg::Flag("--cost"),
    ),
    SECRET_SOURCE_FIELDS[0],
    SECRET_SOURCE_FIELDS[1],
    SECRET_SOURCE_FIELDS[2],
    SECRET_SOURCE_FIELDS[3],
];
const TOTP_ALGORITHMS: &[&str] = &["sha1", "sha256", "sha512"];
const TOTP_CODE_FIELDS: &[FieldDef] = &[
    SECRET_SOURCE_FIELDS[0],
    SECRET_SOURCE_FIELDS[1],
    SECRET_SOURCE_FIELDS[2],
    SECRET_SOURCE_FIELDS[3],
    choice_def(
        "algorithm",
        "Algorithm",
        "TOTP hash algorithm",
        TOTP_ALGORITHMS,
        0,
        FieldArg::Flag("--algorithm"),
    ),
    number_def(
        "digits",
        "Digits",
        "Number of output digits",
        6,
        6,
        10,
        FieldArg::Flag("--digits"),
    ),
    number_def(
        "period",
        "Period",
        "TOTP period in seconds",
        30,
        1,
        u32::MAX as u64,
        FieldArg::Flag("--period"),
    ),
    text_def(
        "timestamp",
        "Timestamp",
        "Optional Unix timestamp; empty uses now",
        "",
        false,
        FieldArg::Flag("--timestamp"),
    ),
];
const TOTP_VERIFY_FIELDS: &[FieldDef] = &[
    text_def(
        "code",
        "Code",
        "TOTP code to verify",
        "123456",
        true,
        FieldArg::Positional,
    ),
    TOTP_CODE_FIELDS[0],
    TOTP_CODE_FIELDS[1],
    TOTP_CODE_FIELDS[2],
    TOTP_CODE_FIELDS[3],
    TOTP_CODE_FIELDS[4],
    TOTP_CODE_FIELDS[5],
    TOTP_CODE_FIELDS[6],
    TOTP_CODE_FIELDS[7],
    number_def(
        "window",
        "Window",
        "Accepted periods before and after the timestamp",
        1,
        0,
        100,
        FieldArg::Flag("--window"),
    ),
];
const CHECKSUM_FILE_FIELDS: &[FieldDef] = &[
    text_def(
        "path",
        "Path",
        "File to checksum",
        "artifact.bin",
        true,
        FieldArg::Positional,
    ),
    choice_def(
        "algorithm",
        "Algorithm",
        "Checksum hash algorithm",
        HASH_OPTIONS,
        0,
        FieldArg::Flag("--algorithm"),
    ),
];
const CHECKSUM_DIRECTORY_FIELDS: &[FieldDef] = &[
    CHECKSUM_FILE_FIELDS[0],
    CHECKSUM_FILE_FIELDS[1],
    toggle_def(
        "follow_links",
        "Follow symlinks",
        "Follow symbolic links while walking the directory",
        false,
        "--follow-links",
    ),
];
const TIME_TO_ISO_FIELDS: &[FieldDef] = &[
    text_def(
        "value",
        "Timestamp",
        "Unix timestamp, including negative pre-epoch values",
        "1700000000",
        true,
        FieldArg::Positional,
    ),
    choice_def(
        "unit",
        "Unit",
        "Timestamp unit",
        TIME_UNITS,
        0,
        FieldArg::Flag("--unit"),
    ),
    toggle_def("utc", "UTC", "Format the result in UTC", false, "--utc"),
];
const TIME_TO_UNIX_FIELDS: &[FieldDef] = &[
    text_def(
        "value",
        "RFC 3339",
        "Time with an explicit offset",
        "2026-08-16T12:00:00-03:00",
        true,
        FieldArg::Positional,
    ),
    TIME_TO_ISO_FIELDS[1],
];
const DURATION_FIELDS: &[FieldDef] = &[text_def(
    "value",
    "Duration",
    "Human-readable duration such as 2h 30m",
    "2h 30m",
    true,
    FieldArg::Positional,
)];
const VALUE_FIELDS: &[FieldDef] = &[text_def(
    "value",
    "Value",
    "Value to process",
    "",
    true,
    FieldArg::Positional,
)];
const PATH_RELATIVE_FIELDS: &[FieldDef] = &[
    text_def(
        "from",
        "From",
        "Base path",
        "/srv/api",
        true,
        FieldArg::Positional,
    ),
    text_def(
        "to",
        "To",
        "Target path",
        "/srv/api/spec/openapi.yaml",
        true,
        FieldArg::Positional,
    ),
];
const SEMVER_COMPARE_FIELDS: &[FieldDef] = &[
    text_def(
        "left",
        "Left version",
        "First semantic version",
        "1.2.3",
        true,
        FieldArg::Positional,
    ),
    text_def(
        "right",
        "Right version",
        "Second semantic version",
        "2.0.0",
        true,
        FieldArg::Positional,
    ),
];
const SEMVER_SORT_FIELDS: &[FieldDef] = &[text_def(
    "versions",
    "Versions",
    "Space-separated semantic versions",
    "2.0.0 1.10.0 1.2.3",
    true,
    FieldArg::Positionals,
)];
const SEMVER_BUMP_KINDS: &[&str] = &["major", "minor", "patch"];
const SEMVER_BUMP_FIELDS: &[FieldDef] = &[
    text_def(
        "value",
        "Version",
        "Semantic version to bump",
        "1.2.3",
        true,
        FieldArg::Positional,
    ),
    choice_def(
        "kind",
        "Part",
        "Version component to increment",
        SEMVER_BUMP_KINDS,
        2,
        FieldArg::Positional,
    ),
];
const QR_FORMATS: &[&str] = &["terminal", "svg", "png"];
const QR_FIELDS: &[FieldDef] = &[
    choice_def(
        "format",
        "Format",
        "Terminal text, SVG, or PNG output",
        QR_FORMATS,
        0,
        FieldArg::Flag("--format"),
    ),
    number_def(
        "size",
        "Image size",
        "SVG/PNG size in pixels",
        256,
        16,
        4096,
        FieldArg::Flag("--size"),
    ),
];
const COMPLETION_SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell", "elvish"];
const COMPLETION_FIELDS: &[FieldDef] = &[choice_def(
    "shell",
    "Shell",
    "Shell completion format",
    COMPLETION_SHELLS,
    1,
    FieldArg::Positional,
)];
const CONFIG_GET_FIELDS: &[FieldDef] = &[config_choice_field(
    "key",
    "Setting",
    "Read one effective setting",
    CONFIG_KEYS,
    0,
)];

pub(super) const TOOLS: &[ToolDef] = &[
    ToolDef {
        category: Category::Formatters,
        name: "Format JSON",
        description: "Indent and normalize JSON",
        base: &["json", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Volnei","active":true}"#),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Format SQL",
        description: "Format SQL for a selected dialect",
        base: &["sql", "format"],
        fields: SQL_FIELDS,
        uses_input: true,
        sample: Some("select id,name from users where active=true"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Format cURL",
        description: "Normalize and safely quote static cURL",
        base: &["curl", "format"],
        fields: CURL_FIELDS,
        uses_input: true,
        sample: Some("curl -H 'Accept: application/json' https://example.com"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Minify",
        description: "Remove insignificant whitespace",
        base: &["json", "minify"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("{\n  \"name\": \"Volnei\"\n}"),
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate JSON",
        description: "Validate JSON syntax",
        base: &["json", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Volnei"}"#),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Sort keys",
        description: "Sort object keys recursively",
        base: &["json", "sort-keys"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"z":1,"a":2}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Flatten",
        description: "Convert nested objects to dotted paths",
        base: &["json", "flatten"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"user":{"name":"Volnei"}}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Unflatten",
        description: "Expand dotted paths into objects",
        base: &["json", "unflatten"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"user.name":"Volnei"}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Read path",
        description: "Read one value using a JSON path",
        base: &["json", "path"],
        fields: JSON_PATH_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Volnei","id":42}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Compare",
        description: "Compare two JSON documents",
        base: &["json", "diff"],
        fields: JSON_DIFF_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "To YAML",
        description: "Convert JSON to YAML",
        base: &["json", "to-yaml"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Volnei","roles":["admin"]}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "To CSV",
        description: "Convert an array of objects to CSV",
        base: &["json", "to-csv"],
        fields: JSON_CSV_FIELDS,
        uses_input: true,
        sample: Some(r#"[{"id":1,"name":"Volnei"}]"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "To TOML",
        description: "Convert a JSON object to TOML",
        base: &["json", "to-toml"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Volnei","active":true}"#),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Pretty YAML",
        description: "Normalize a YAML document",
        base: &["yaml", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name: Volnei\nroles: [admin, developer]"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "YAML to JSON",
        description: "Convert one YAML document to JSON",
        base: &["yaml", "to-json"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name: Volnei\nactive: true"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "CSV to JSON",
        description: "Convert CSV rows to JSON objects",
        base: &["csv", "to-json"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name,active\nVolnei,true"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Pretty TOML",
        description: "Normalize a TOML document",
        base: &["toml", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name=\"Volnei\"\nactive=true"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "TOML to JSON",
        description: "Convert TOML to JSON",
        base: &["toml", "to-json"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name = \"Volnei\"\nactive = true"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Pretty XML",
        description: "Indent an XML document",
        base: &["xml", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("<user><name>Volnei</name></user>"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Parse dotenv",
        description: "Parse dotenv entries as JSON",
        base: &["dotenv", "parse"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("APP_ENV=development\nPORT=8080"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Sort dotenv",
        description: "Sort dotenv entries by key",
        base: &["dotenv", "sort"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("PORT=8080\nAPP_ENV=development"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Change case",
        description: "Convert naming convention",
        base: &["text", "case"],
        fields: TEXT_CASE_FIELDS,
        uses_input: true,
        sample: Some("hello backend world"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Slug",
        description: "Create a URL-safe slug",
        base: &["text", "slug"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("Olá, backend com vutils!"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Trim",
        description: "Trim surrounding whitespace",
        base: &["text", "trim"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("  backend value  "),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Sort lines",
        description: "Sort lines with optional deduplication",
        base: &["text", "sort-lines"],
        fields: TEXT_SORT_FIELDS,
        uses_input: true,
        sample: Some("pear\napple\norange"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Unique lines",
        description: "Keep the first occurrence of each line",
        base: &["text", "unique-lines"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("alpha\nbeta\nalpha"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Only digits",
        description: "Remove every non-digit character",
        base: &["text", "only-digits"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("+55 (11) 91234-5678"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Regex test",
        description: "Inspect regex matches",
        base: &["regex", "test"],
        fields: REGEX_TEST_FIELDS,
        uses_input: true,
        sample: Some("abc 123 def"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Regex replace",
        description: "Replace regex matches",
        base: &["regex", "replace"],
        fields: REGEX_REPLACE_FIELDS,
        uses_input: true,
        sample: Some("order 123, item 456"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "Base64 encode",
        description: "Encode bytes as Base64",
        base: &["base64", "encode"],
        fields: BASE64_FIELDS,
        uses_input: true,
        sample: Some("Hello, vutils!"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "Base64 decode",
        description: "Decode Base64 into bytes",
        base: &["base64", "decode"],
        fields: BASE64_FIELDS,
        uses_input: true,
        sample: Some("SGVsbG8sIHZ1dGlscyE="),
    },
    ToolDef {
        category: Category::Codecs,
        name: "Hex encode",
        description: "Encode bytes as hexadecimal",
        base: &["hex", "encode"],
        fields: HEX_FIELDS,
        uses_input: true,
        sample: Some("Hello"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "Hex decode",
        description: "Decode hexadecimal into bytes",
        base: &["hex", "decode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("48656c6c6f"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "URL encode",
        description: "Percent-encode a URL component",
        base: &["url", "encode"],
        fields: URL_FIELDS,
        uses_input: true,
        sample: Some("hello world/olá"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "URL decode",
        description: "Decode a URL component",
        base: &["url", "decode"],
        fields: URL_FIELDS,
        uses_input: true,
        sample: Some("hello%20world%2Fol%C3%A1"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "URL inspect",
        description: "Inspect URL components locally",
        base: &["url", "inspect"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("https://example.com:8443/api?q=vutils#result"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "HTML encode",
        description: "Escape HTML entities",
        base: &["html", "encode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("<strong>Volnei & vutils</strong>"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "HTML decode",
        description: "Decode HTML entities",
        base: &["html", "decode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("&lt;strong&gt;Volnei&lt;/strong&gt;"),
    },
    ToolDef {
        category: Category::Random,
        name: "UUID",
        description: "Generate UUID v1 through v8",
        base: &["uuid"],
        fields: UUID_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Password",
        description: "Generate category-balanced passwords",
        base: &["gen", "password"],
        fields: PASSWORD_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Token",
        description: "Generate random local tokens",
        base: &["gen", "token"],
        fields: TOKEN_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "ULID",
        description: "Generate sortable ULIDs",
        base: &["id", "ulid"],
        fields: COUNT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "NanoID",
        description: "Generate compact random IDs",
        base: &["id", "nanoid"],
        fields: NANOID_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "ObjectId",
        description: "Generate MongoDB-style ObjectIds",
        base: &["id", "objectid"],
        fields: COUNT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Brazil profile",
        description: "Generate a synthetic Brazilian profile",
        base: &["br"],
        fields: NO_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "PIX key",
        description: "Generate synthetic PIX keys",
        base: &["br", "pix"],
        fields: BR_PIX_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "Encrypt",
        description: "Encrypt input with a guided password source",
        base: &["enc"],
        fields: ENCRYPT_FIELDS,
        uses_input: true,
        sample: Some("backend secret"),
    },
    ToolDef {
        category: Category::Security,
        name: "Decrypt",
        description: "Decrypt an envelope with a guided password source",
        base: &["dec"],
        fields: DECRYPT_FIELDS,
        uses_input: true,
        sample: Some(""),
    },
    ToolDef {
        category: Category::Security,
        name: "Hash",
        description: "Calculate a SHA digest",
        base: &["hash"],
        fields: HASH_FIELDS,
        uses_input: true,
        sample: Some("Hello, vutils!"),
    },
    ToolDef {
        category: Category::Security,
        name: "Decode JWT",
        description: "Inspect claims without signature verification",
        base: &["jwt", "decode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("eyJhbGciOiJub25lIn0.eyJzdWIiOiIxMjMifQ."),
    },
    ToolDef {
        category: Category::Security,
        name: "TOTP secret",
        description: "Generate a local Base32 TOTP secret",
        base: &["totp", "generate-secret"],
        fields: TOTP_SECRET_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "Inspect PEM",
        description: "Inspect blocks in a PEM container",
        base: &["pem", "inspect"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("-----BEGIN PUBLIC KEY-----\nAA==\n-----END PUBLIC KEY-----"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Generate models",
        description: "Infer typed models from JSON",
        base: &["code", "types"],
        fields: CODE_FIELDS,
        uses_input: true,
        sample: Some(r#"{"id":1,"name":"Volnei","active":true}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Cron next",
        description: "List upcoming cron occurrences",
        base: &["cron", "next"],
        fields: CRON_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Cron explain",
        description: "Explain a cron schedule",
        base: &["cron", "explain"],
        fields: CRON_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Current time",
        description: "Return local, UTC, or Unix time",
        base: &["time", "now"],
        fields: TIME_NOW_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Codecs,
        name: "Convert number",
        description: "Convert an integer between bases",
        base: &["number", "convert"],
        fields: NUMBER_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Formatters,
        name: "Format bytes",
        description: "Format a byte count for humans",
        base: &["bytes", "format"],
        fields: BYTES_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Email",
        description: "Generate synthetic email addresses",
        base: &["gen", "email"],
        fields: GEN_EMAIL_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Name",
        description: "Generate synthetic person names",
        base: &["gen", "name"],
        fields: COUNT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Lorem ipsum",
        description: "Generate placeholder prose",
        base: &["gen", "lorem"],
        fields: GEN_LOREM_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "CPF",
        description: "Generate or validate Brazilian CPF values",
        base: &["br", "cpf"],
        fields: BR_DOCUMENT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "CNPJ",
        description: "Generate or validate Brazilian CNPJ values",
        base: &["br", "cnpj"],
        fields: BR_DOCUMENT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "CEP",
        description: "Generate synthetic Brazilian CEP values",
        base: &["br", "cep"],
        fields: BR_FIXTURE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Random,
        name: "Phone",
        description: "Generate synthetic Brazilian mobile phones",
        base: &["br", "phone"],
        fields: BR_FIXTURE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Codecs,
        name: "Binary encode",
        description: "Encode bytes as a bit string",
        base: &["binary", "encode"],
        fields: BINARY_ENCODE_FIELDS,
        uses_input: true,
        sample: Some("Volnei"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "Binary decode",
        description: "Decode a bit string back to bytes",
        base: &["binary", "decode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("01010110 01101111 01101100 01101110 01100101 01101001"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "GZip compress",
        description: "Compress input with GZip",
        base: &["gzip", "compress"],
        fields: GZIP_FIELDS,
        uses_input: true,
        sample: Some("Volnei backend payload"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "GZip decompress",
        description: "Decompress GZip bytes",
        base: &["gzip", "decompress"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: None,
    },
    ToolDef {
        category: Category::Codecs,
        name: "JSON escape",
        description: "Escape text as JSON string content",
        base: &["json", "escape"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("Hello, \"Volnei\"\n"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "JSON unescape",
        description: "Decode JSON string escapes",
        base: &["json", "unescape"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("Hello, \\\"Volnei\\\"\\n"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "String escape",
        description: "Escape a string literal for a language",
        base: &["string", "escape"],
        fields: STRING_FIELDS,
        uses_input: true,
        sample: Some("Hello, \"Volnei\""),
    },
    ToolDef {
        category: Category::Codecs,
        name: "String unescape",
        description: "Unescape a language string literal",
        base: &["string", "unescape"],
        fields: STRING_FIELDS,
        uses_input: true,
        sample: Some("Hello, \\\"Volnei\\\""),
    },
    ToolDef {
        category: Category::Codecs,
        name: "Parse bytes",
        description: "Parse a human-readable size into bytes",
        base: &["bytes", "parse"],
        fields: BYTES_PARSE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Codecs,
        name: "Chmod encode",
        description: "Convert symbolic Unix permissions to octal",
        base: &["chmod", "encode"],
        fields: VALUE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Codecs,
        name: "Chmod decode",
        description: "Convert octal Unix permissions to symbolic form",
        base: &["chmod", "decode"],
        fields: VALUE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Codecs,
        name: "QR code",
        description: "Render a QR code locally",
        base: &["qr", "generate"],
        fields: QR_FIELDS,
        uses_input: true,
        sample: Some("https://example.com/users/volnei"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Normalize EOL",
        description: "Normalize text line endings",
        base: &["text", "normalize-eol"],
        fields: NORMALIZE_EOL_FIELDS,
        uses_input: true,
        sample: Some("first line\nsecond line"),
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate JSON Schema",
        description: "Validate JSON against a local schema",
        base: &["json", "schema-validate"],
        fields: JSON_SCHEMA_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Volnei"}"#),
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate YAML",
        description: "Validate YAML syntax",
        base: &["yaml", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name: Volnei"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Split YAML",
        description: "Split a multi-document YAML stream",
        base: &["yaml", "split"],
        fields: YAML_SPLIT_FIELDS,
        uses_input: true,
        sample: Some("name: Volnei\n---\nname: vutils"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Join YAML",
        description: "Join YAML files into a document stream",
        base: &["yaml", "join"],
        fields: FILE_LIST_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate CSV",
        description: "Validate CSV structure",
        base: &["csv", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name,active\nVolnei,true"),
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate TOML",
        description: "Validate TOML syntax",
        base: &["toml", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name = \"Volnei\""),
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate XML",
        description: "Validate XML well-formedness",
        base: &["xml", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("<name>Volnei</name>"),
    },
    ToolDef {
        category: Category::Validators,
        name: "Validate dotenv",
        description: "Validate dotenv syntax",
        base: &["dotenv", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("NAME=Volnei"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Compare dotenv",
        description: "Compare dotenv keys with values redacted by default",
        base: &["dotenv", "diff"],
        fields: DOTENV_DIFF_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Compare text",
        description: "Compare two text values",
        base: &["text", "diff"],
        fields: TEXT_DIFF_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Unicode inspect",
        description: "Inspect Unicode code points",
        base: &["text", "unicode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("Olá, Volnei 👋"),
    },
    ToolDef {
        category: Category::Security,
        name: "HMAC",
        description: "Calculate a keyed message authentication code",
        base: &["hmac"],
        fields: HMAC_FIELDS,
        uses_input: true,
        sample: Some("backend payload"),
    },
    ToolDef {
        category: Category::Security,
        name: "Argon2 hash",
        description: "Hash a secret with Argon2id",
        base: &["password-hash", "argon2-hash"],
        fields: SECRET_SOURCE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "Argon2 verify",
        description: "Verify a secret against an Argon2 hash",
        base: &["password-hash", "argon2-verify"],
        fields: PASSWORD_VERIFY_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "bcrypt hash",
        description: "Hash a secret with bcrypt",
        base: &["password-hash", "bcrypt-hash"],
        fields: BCRYPT_HASH_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "bcrypt verify",
        description: "Verify a secret against a bcrypt hash",
        base: &["password-hash", "bcrypt-verify"],
        fields: PASSWORD_VERIFY_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "TOTP code",
        description: "Generate an offline TOTP code",
        base: &["totp", "code"],
        fields: TOTP_CODE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "TOTP verify",
        description: "Verify an offline TOTP code",
        base: &["totp", "verify"],
        fields: TOTP_VERIFY_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "Checksum file",
        description: "Calculate a local file checksum",
        base: &["checksum", "file"],
        fields: CHECKSUM_FILE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "Checksum directory",
        description: "Calculate deterministic directory checksums",
        base: &["checksum", "directory"],
        fields: CHECKSUM_DIRECTORY_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Security,
        name: "Inspect certificate",
        description: "Inspect a local PEM-encoded X.509 certificate",
        base: &["cert", "inspect"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Timestamp to ISO",
        description: "Convert a Unix timestamp to RFC 3339",
        base: &["time", "to-iso"],
        fields: TIME_TO_ISO_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "ISO to Unix",
        description: "Convert RFC 3339 time to a Unix timestamp",
        base: &["time", "to-unix"],
        fields: TIME_TO_UNIX_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Parse duration",
        description: "Parse a human-readable duration into milliseconds",
        base: &["time", "duration"],
        fields: DURATION_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Normalize path",
        description: "Normalize path components without filesystem access",
        base: &["path", "normalize"],
        fields: VALUE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Relative path",
        description: "Calculate one path relative to another",
        base: &["path", "relative"],
        fields: PATH_RELATIVE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Compare semver",
        description: "Compare semantic versions",
        base: &["semver", "compare"],
        fields: SEMVER_COMPARE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Sort semver",
        description: "Sort semantic versions",
        base: &["semver", "sort"],
        fields: SEMVER_SORT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Bump semver",
        description: "Increment a semantic-version component",
        base: &["semver", "bump"],
        fields: SEMVER_BUMP_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "Inspect CIDR",
        description: "Inspect an IP address or CIDR range",
        base: &["ip", "cidr"],
        fields: VALUE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Parsers,
        name: "MIME lookup",
        description: "Look up a MIME type by file extension",
        base: &["mime"],
        fields: VALUE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Vruno,
        name: "Configure",
        description: "Register collection and OpenAPI paths",
        base: &["vruno", "configure"],
        fields: VRUNO_CONFIGURE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Vruno,
        name: "Show setup",
        description: "Show the effective Vruno paths",
        base: &["vruno", "show"],
        fields: NO_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Vruno,
        name: "Check drift",
        description: "Compare OpenAPI with the collection without writing",
        base: &["vruno", "check"],
        fields: VRUNO_CHECK_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Vruno,
        name: "Preview sync",
        description: "Preview files Vruno would create or update",
        base: &["vruno", "preview"],
        fields: VRUNO_PREVIEW_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Vruno,
        name: "Sync collection",
        description: "Create and update collection files from OpenAPI",
        base: &["vruno", "sync"],
        fields: VRUNO_SYNC_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Configuration",
        description: "Show effective vutils configuration",
        base: &["config", "list"],
        fields: NO_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Read setting",
        description: "Read one effective configuration value",
        base: &["config", "get"],
        fields: CONFIG_GET_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "SQL dialect",
        description: "Set the default SQL dialect",
        base: &["config", "set", "sql.dialect"],
        fields: CONFIG_SQL_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "UUID version",
        description: "Set the default UUID version",
        base: &["config", "set", "uuid.version"],
        fields: CONFIG_UUID_VERSION_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "UUID format",
        description: "Set the default UUID text format",
        base: &["config", "set", "uuid.format"],
        fields: CONFIG_UUID_FORMAT_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Encryption algorithm",
        description: "Set the default encryption algorithm",
        base: &["config", "set", "crypto.algorithm"],
        fields: CONFIG_CRYPTO_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Password environment",
        description: "Set the environment-variable name used to automate enc and dec",
        base: &["config", "set", "crypto.password-env"],
        fields: CONFIG_PASSWORD_ENV_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Password file",
        description: "Set the password file source",
        base: &["config", "set", "crypto.password-file"],
        fields: CONFIG_PASSWORD_FILE_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Home shortcuts",
        description: "Set the operations shown on Home",
        base: &["config", "set", "tui.home"],
        fields: CONFIG_HOME_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Vruno collection",
        description: "Set the default Bruno collection directory",
        base: &["config", "set", "vruno.collection"],
        fields: CONFIG_VRUNO_COLLECTION_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Vruno OpenAPI",
        description: "Set the default local OpenAPI file",
        base: &["config", "set", "vruno.openapi"],
        fields: CONFIG_VRUNO_OPENAPI_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Reset setting",
        description: "Restore one default or clear an optional setting",
        base: &["config", "unset"],
        fields: CONFIG_RESET_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Config path",
        description: "Show the active vutils config file",
        base: &["config", "path"],
        fields: NO_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Shell completion",
        description: "Generate shell completion definitions",
        base: &["completion"],
        fields: COMPLETION_FIELDS,
        uses_input: false,
        sample: None,
    },
    ToolDef {
        category: Category::Configuration,
        name: "Manual page",
        description: "Generate the vutils manual page",
        base: &["man"],
        fields: NO_FIELDS,
        uses_input: false,
        sample: None,
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::cli::Cli;
    use clap::{CommandFactory as _, Parser as _};
    use vutils::config::DEFAULT_TUI_HOME;

    fn tool(name: &str) -> &'static ToolDef {
        TOOLS.iter().find(|tool| tool.name == name).unwrap()
    }

    fn set_choice(tool: &ToolDef, states: &mut [FieldState], key: &str, value: &str) {
        let index = tool
            .fields
            .iter()
            .position(|field| field.key == key)
            .unwrap();
        let FieldKind::Choice { options, .. } = tool.fields[index].kind else {
            panic!("field is not a choice");
        };
        states[index] = FieldState::Choice(options.iter().position(|item| *item == value).unwrap());
    }

    #[test]
    fn every_tool_has_a_valid_default_command() {
        for tool in TOOLS {
            let mut states = new_form(tool);
            for (definition, state) in tool.fields.iter().zip(&mut states) {
                let needs_test_value = matches!(definition.kind, FieldKind::Secret { .. })
                    || matches!(
                        definition.kind,
                        FieldKind::Text {
                            default: "",
                            required: true
                        }
                    );
                if needs_test_value && let Some(editor) = state.editor_mut() {
                    editor.replace("value");
                }
            }
            let args = build_args(tool, &states)
                .unwrap_or_else(|error| panic!("{}: {}", tool.name, error.message));
            let command = std::iter::once("vutils".to_owned()).chain(args);
            assert!(
                Cli::try_parse_from(command).is_ok(),
                "{} emits arguments rejected by the CLI parser",
                tool.name
            );
        }
    }

    #[test]
    fn catalog_schema_is_internally_consistent() {
        let mut tool_ids = HashSet::new();
        for tool in TOOLS {
            assert!(!tool.base.is_empty(), "{} has no base command", tool.name);
            assert!(
                tool_ids.insert(tool_id(tool)),
                "{} repeats a command id",
                tool.name
            );
            let mut keys = HashSet::new();
            for field in tool.fields {
                assert!(
                    keys.insert(field.key),
                    "{} repeats {}",
                    tool.name,
                    field.key
                );
                match field.kind {
                    FieldKind::Choice { options, default } => {
                        assert!(!options.is_empty(), "{} has an empty choice", field.label);
                        assert!(
                            default < options.len(),
                            "{} has an invalid default",
                            field.label
                        );
                    }
                    FieldKind::Number {
                        default,
                        min,
                        max,
                        step,
                    } => {
                        assert!(min <= default && default <= max);
                        assert!(step > 0);
                    }
                    FieldKind::Toggle { .. }
                    | FieldKind::Text { .. }
                    | FieldKind::Secret { .. } => {}
                }
                if let Some(condition) = field.condition {
                    assert!(
                        tool.fields
                            .iter()
                            .any(|candidate| candidate.key == condition.field),
                        "{} references missing condition field {}",
                        field.label,
                        condition.field
                    );
                    assert!(!condition.values.is_empty());
                }
            }
        }
    }

    #[test]
    fn uuid_fields_follow_the_selected_version() {
        let tool = tool("UUID");
        let mut states = new_form(tool);
        set_choice(tool, &mut states, "version", "v3");
        let args = build_args(tool, &states).unwrap();
        assert!(args.windows(2).any(|pair| pair == ["--namespace", "dns"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--name", "api.example.com"])
        );
        assert!(!args.iter().any(|argument| argument == "--custom-bytes"));

        set_choice(tool, &mut states, "version", "v8");
        let args = build_args(tool, &states).unwrap();
        assert!(args.iter().any(|argument| argument == "--custom-bytes"));
        assert!(!args.iter().any(|argument| argument == "--name"));
    }

    #[test]
    fn uuid_form_only_exposes_parameters_relevant_to_each_version() {
        let tool = tool("UUID");
        let mut states = new_form(tool);
        let cases = [
            ("v1", vec!["version", "count", "format", "node"]),
            (
                "v2",
                vec!["version", "count", "format", "domain", "local_id", "node"],
            ),
            (
                "v3",
                vec!["version", "count", "format", "namespace", "name"],
            ),
            ("v4", vec!["version", "count", "format"]),
            (
                "v5",
                vec!["version", "count", "format", "namespace", "name"],
            ),
            ("v6", vec!["version", "count", "format", "node"]),
            ("v7", vec!["version", "count", "format"]),
            ("v8", vec!["version", "count", "format", "custom"]),
        ];

        for (version, expected) in cases {
            set_choice(tool, &mut states, "version", version);
            let visible = visible_fields(tool, &states)
                .into_iter()
                .map(|index| tool.fields[index].key)
                .collect::<Vec<_>>();
            assert_eq!(visible, expected, "unexpected fields for {version}");
        }
    }

    #[test]
    fn password_form_maps_friendly_toggles_to_cli_flags() {
        let tool = tool("Password");
        let mut states = new_form(tool);
        let symbols = tool
            .fields
            .iter()
            .position(|field| field.key == "symbols")
            .unwrap();
        states[symbols] = FieldState::Toggle(false);
        let args = build_args(tool, &states).unwrap();
        assert!(args.iter().any(|argument| argument == "--no-symbols"));
        assert!(
            args.iter()
                .any(|argument| argument == "--exclude-ambiguous")
        );
    }

    #[test]
    fn encryption_password_is_required_but_never_exposed_in_the_ui_preview() {
        let tool = tool("Encrypt");
        let mut states = new_form(tool);
        set_choice(tool, &mut states, "password_source", "direct");
        let password = tool
            .fields
            .iter()
            .position(|field| field.key == "password")
            .unwrap();
        assert!(build_args(tool, &states).is_err());
        states[password]
            .editor_mut()
            .unwrap()
            .replace("correct horse battery staple");

        let args = build_args(tool, &states).unwrap();
        let preview = command_preview(tool, &states);

        assert!(
            args.iter()
                .any(|argument| argument == "correct horse battery staple")
        );
        let display = states[password].display(&tool.fields[password]);
        assert_eq!(
            display.chars().count(),
            "correct horse battery staple".chars().count()
        );
        assert!(display.chars().all(|character| character == '•'));
        assert!(preview.contains("<redacted>"));
        assert!(!preview.contains("correct horse"));
    }

    #[test]
    fn every_fixed_category_contains_operations() {
        for category in Category::ALL
            .into_iter()
            .filter(|item| *item != Category::Home)
        {
            assert!(
                !tools_in(category).is_empty(),
                "{} is empty",
                category.label()
            );
        }
    }

    #[test]
    fn every_cli_leaf_command_is_available_in_the_tui() {
        fn collect_leaves(
            command: &clap::Command,
            prefix: &mut Vec<String>,
            leaves: &mut Vec<Vec<String>>,
        ) {
            for subcommand in command
                .get_subcommands()
                .filter(|subcommand| subcommand.get_name() != "help")
            {
                prefix.push(subcommand.get_name().to_owned());
                if subcommand.get_subcommands().next().is_none() {
                    leaves.push(prefix.clone());
                } else {
                    collect_leaves(subcommand, prefix, leaves);
                }
                prefix.pop();
            }
        }

        let mut leaves = Vec::new();
        collect_leaves(&Cli::command(), &mut Vec::new(), &mut leaves);
        let missing = leaves
            .into_iter()
            .filter(|path| path != &["tui"])
            .filter(|path| {
                let direct = TOOLS.iter().any(|tool| {
                    tool.base.len() >= path.len()
                        && tool
                            .base
                            .iter()
                            .zip(path)
                            .all(|(actual, expected)| *actual == expected)
                });
                let dynamic_hash = path.first().is_some_and(|part| part == "hash")
                    && path.get(1).is_some_and(|algorithm| {
                        TOOLS.iter().any(|tool| {
                            tool.base == ["hash"]
                                && tool.fields.iter().any(|field| {
                                    matches!(
                                        field.kind,
                                        FieldKind::Choice { options, .. }
                                            if options.contains(&algorithm.as_str())
                                    )
                                })
                        })
                    });
                !direct && !dynamic_hash
            })
            .map(|path| path.join(" "))
            .collect::<Vec<_>>();

        assert!(missing.is_empty(), "missing TUI commands: {missing:?}");
    }

    #[test]
    fn taxonomy_matches_backend_workflows() {
        for name in [
            "Format JSON",
            "Format SQL",
            "Format cURL",
            "Pretty YAML",
            "Regex replace",
            "Format bytes",
        ] {
            assert_eq!(tool(name).category, Category::Formatters, "{name}");
        }
        for name in [
            "Read path",
            "YAML to JSON",
            "Parse dotenv",
            "Generate models",
            "Cron explain",
        ] {
            assert_eq!(tool(name).category, Category::Parsers, "{name}");
        }
        for name in [
            "Validate JSON",
            "Validate JSON Schema",
            "Validate YAML",
            "Validate CSV",
            "Validate TOML",
            "Validate XML",
            "Validate dotenv",
        ] {
            assert_eq!(tool(name).category, Category::Validators, "{name}");
        }
        assert_eq!(tool("Convert number").category, Category::Codecs);
        assert_eq!(tool("UUID").category, Category::Random);
        assert_eq!(tool("Encrypt").category, Category::Security);
        assert_eq!(tool("Configure").category, Category::Vruno);
        assert_eq!(tool("Configuration").category, Category::Configuration);

        let first_formatters = tools_in(Category::Formatters)
            .into_iter()
            .take(3)
            .map(|index| TOOLS[index].name)
            .collect::<Vec<_>>();
        assert_eq!(
            first_formatters,
            ["Format JSON", "Format SQL", "Format cURL"]
        );

        let validators = tools_in(Category::Validators)
            .into_iter()
            .map(|index| tool_id(&TOOLS[index]))
            .collect::<Vec<_>>();
        assert_eq!(
            validators,
            [
                "json.validate",
                "json.schema-validate",
                "yaml.validate",
                "csv.validate",
                "toml.validate",
                "xml.validate",
                "dotenv.validate",
            ]
        );
    }

    #[test]
    fn configuration_tab_edits_every_supported_setting() {
        let editable = tools_in(Category::Configuration)
            .into_iter()
            .filter_map(|index| match TOOLS[index].base {
                ["config", "set", key] => Some(*key),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let supported = CONFIG_KEYS.iter().copied().collect::<HashSet<_>>();

        assert_eq!(editable, supported);
        assert_eq!(CONFIG_RESET_FIELDS.len(), 1);
    }

    #[test]
    fn default_home_shortcuts_resolve_to_catalog_tools() {
        for shortcut in DEFAULT_TUI_HOME {
            assert!(
                find_tool(shortcut).is_some(),
                "missing Home tool {shortcut}"
            );
        }
    }
}
