use super::editor::Editor;

pub(super) const CATEGORY_COUNT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Home,
    Random,
    Formatters,
    Parsers,
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
    Positional,
    ToggleFlag { flag: &'static str, when: bool },
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
        FieldKind::Text { .. } | FieldKind::Number { .. } => {}
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
            FieldArg::Positional if !value.trim().is_empty() => args.push(value),
            FieldArg::ToggleFlag { flag, when } => {
                if matches!(state, FieldState::Toggle(value) if *value == when) {
                    args.push(flag.into());
                }
            }
            FieldArg::Flag(_) | FieldArg::Positional => {}
        }
    }
    Ok(args)
}

pub(super) fn command_preview(tool: &ToolDef, states: &[FieldState]) -> String {
    match build_args(tool, states) {
        Ok(args) => format!(
            "$ vutils {}",
            args.iter()
                .map(|value| shell_quote(value))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Err(error) => format!("$ vutils {}  · {}", tool.base.join(" "), error.message),
    }
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
    FieldDef {
        key: "group_by",
        label: "Group by",
        help: "Create request folders from OpenAPI tags or URL paths",
        kind: FieldKind::Choice {
            options: VRUNO_GROUPS,
            default: 0,
        },
        arg: FieldArg::Flag("--group-by"),
        condition: None,
    },
];
const VRUNO_PREVIEW_FIELDS: &[FieldDef] = &[VRUNO_CHECK_FIELDS[1]];
const VRUNO_SYNC_FIELDS: &[FieldDef] = &[
    VRUNO_CHECK_FIELDS[1],
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

pub(super) const TOOLS: &[ToolDef] = &[
    ToolDef {
        category: Category::Formatters,
        name: "Format JSON",
        description: "Indent and normalize JSON",
        base: &["json", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Ana","active":true}"#),
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
        fields: NO_FIELDS,
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
        sample: Some("{\n  \"name\": \"Ana\"\n}"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Validate",
        description: "Validate JSON syntax",
        base: &["json", "validate"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Ana"}"#),
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
        sample: Some(r#"{"user":{"name":"Ana"}}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Unflatten",
        description: "Expand dotted paths into objects",
        base: &["json", "unflatten"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"user.name":"Ana"}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "Read path",
        description: "Read one value using a JSON path",
        base: &["json", "path"],
        fields: JSON_PATH_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Ana","id":42}"#),
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
        sample: Some(r#"{"name":"Ana","roles":["admin"]}"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "To CSV",
        description: "Convert an array of objects to CSV",
        base: &["json", "to-csv"],
        fields: JSON_CSV_FIELDS,
        uses_input: true,
        sample: Some(r#"[{"id":1,"name":"Ana"}]"#),
    },
    ToolDef {
        category: Category::Parsers,
        name: "To TOML",
        description: "Convert a JSON object to TOML",
        base: &["json", "to-toml"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some(r#"{"name":"Ana","active":true}"#),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Pretty YAML",
        description: "Normalize a YAML document",
        base: &["yaml", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name: Ana\nroles: [admin, developer]"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "YAML to JSON",
        description: "Convert one YAML document to JSON",
        base: &["yaml", "to-json"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name: Ana\nactive: true"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "CSV to JSON",
        description: "Convert CSV rows to JSON objects",
        base: &["csv", "to-json"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name,active\nAna,true"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Pretty TOML",
        description: "Normalize a TOML document",
        base: &["toml", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name=\"Ana\"\nactive=true"),
    },
    ToolDef {
        category: Category::Parsers,
        name: "TOML to JSON",
        description: "Convert TOML to JSON",
        base: &["toml", "to-json"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("name = \"Ana\"\nactive = true"),
    },
    ToolDef {
        category: Category::Formatters,
        name: "Pretty XML",
        description: "Indent an XML document",
        base: &["xml", "pretty"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("<user><name>Ana</name></user>"),
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
        sample: Some("<strong>Ana & Bia</strong>"),
    },
    ToolDef {
        category: Category::Codecs,
        name: "HTML decode",
        description: "Decode HTML entities",
        base: &["html", "decode"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("&lt;strong&gt;Ana&lt;/strong&gt;"),
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
        description: "Encrypt input using the configured password source",
        base: &["enc"],
        fields: NO_FIELDS,
        uses_input: true,
        sample: Some("backend secret"),
    },
    ToolDef {
        category: Category::Security,
        name: "Decrypt",
        description: "Decrypt a vutils envelope using the configured password source",
        base: &["dec"],
        fields: NO_FIELDS,
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
        sample: Some(r#"{"id":1,"name":"Ana","active":true}"#),
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
        name: "Config path",
        description: "Show the active vutils config file",
        base: &["config", "path"],
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
    use clap::Parser as _;
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
            let states = new_form(tool);
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
                    FieldKind::Toggle { .. } | FieldKind::Text { .. } => {}
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
            "Validate",
            "Read path",
            "YAML to JSON",
            "Parse dotenv",
            "Generate models",
            "Cron explain",
        ] {
            assert_eq!(tool(name).category, Category::Parsers, "{name}");
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
