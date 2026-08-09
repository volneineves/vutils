use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;
use sqlformat::{Dialect as FormatDialect, FormatOptions, Indent, QueryParams};
use sqlparser::{
    ast::Statement,
    dialect::{
        Dialect as ParserDialect, GenericDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect,
        SQLiteDialect,
    },
    parser::Parser,
};

use crate::{Result, VutilsError, data::parse_json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Generic,
    PostgreSql,
    MySql,
    SQLite,
    SqlServer,
}

#[derive(Debug, Serialize)]
pub struct SqlGeneration {
    pub sql: String,
    pub params: Vec<Value>,
}

pub fn format_sql(
    input: &str,
    dialect: SqlDialect,
    uppercase: Option<bool>,
    indent: u8,
    inline: bool,
) -> Result<String> {
    validate_sql(input, dialect)?;
    if !(1..=8).contains(&indent) {
        return Err(VutilsError::InvalidInput(
            "SQL indent must be between 1 and 8".into(),
        ));
    }
    let options = FormatOptions {
        indent: Indent::Spaces(indent),
        uppercase,
        inline,
        dialect: format_dialect(dialect),
        ..FormatOptions::default()
    };
    Ok(sqlformat::format(input, &QueryParams::None, &options))
}

pub fn minify_sql(input: &str, dialect: SqlDialect, strip_comments: bool) -> Result<String> {
    let input = if strip_comments {
        remove_sql_comments(input)?
    } else {
        input.to_owned()
    };
    format_sql(&input, dialect, None, 2, true)
}

pub fn validate_sql(input: &str, dialect: SqlDialect) -> Result<Vec<Statement>> {
    Parser::parse_sql(parser_dialect(dialect).as_ref(), input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid SQL: {error}")))
}

pub fn inspect_sql(input: &str, dialect: SqlDialect) -> Result<String> {
    #[derive(Serialize)]
    struct StatementInfo {
        kind: &'static str,
        normalized: String,
        tables: Vec<String>,
        aliases: std::collections::BTreeMap<String, String>,
        placeholders: Vec<String>,
    }
    let statements = validate_sql(input, dialect)?;
    let values = statements
        .iter()
        .map(|statement| {
            let normalized = statement.to_string();
            let (tables, aliases) = extract_sql_metadata(&normalized)?;
            let placeholders = extract_placeholders(&normalized);
            Ok(StatementInfo {
                kind: statement_kind(statement),
                normalized,
                tables,
                aliases,
                placeholders,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    serde_json::to_string_pretty(&values).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn generate_insert(
    table: &str,
    input: &str,
    dialect: SqlDialect,
    literal: bool,
) -> Result<SqlGeneration> {
    let value = parse_json(input)?;
    let rows: Vec<_> = match value {
        Value::Object(object) => vec![object],
        Value::Array(values) => values
            .into_iter()
            .map(|value| {
                value.as_object().cloned().ok_or_else(|| {
                    VutilsError::InvalidInput("every insert row must be a JSON object".into())
                })
            })
            .collect::<Result<_>>()?,
        _ => {
            return Err(VutilsError::InvalidInput(
                "SQL insert data must be a JSON object or array of objects".into(),
            ));
        }
    };
    if rows.is_empty() {
        return Err(VutilsError::InvalidInput(
            "SQL insert data cannot be empty".into(),
        ));
    }
    let columns: BTreeSet<_> = rows.iter().flat_map(|row| row.keys().cloned()).collect();
    if columns.is_empty() {
        return Err(VutilsError::InvalidInput(
            "SQL insert rows cannot be empty".into(),
        ));
    }
    let columns: Vec<_> = columns.into_iter().collect();
    let mut params = Vec::new();
    let mut tuples = Vec::new();
    for row in rows {
        let values = columns
            .iter()
            .map(|column| row.get(column).cloned().unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        let rendered = if literal {
            values
                .iter()
                .map(|value| quote_literal(value, dialect))
                .collect::<Result<Vec<_>>>()?
        } else {
            values
                .iter()
                .map(|value| {
                    params.push(value.clone());
                    placeholder(params.len(), dialect)
                })
                .collect()
        };
        tuples.push(format!("({})", rendered.join(", ")));
    }
    let sql = format!(
        "INSERT INTO {} ({}) VALUES {};",
        quote_identifier(table, dialect)?,
        columns
            .iter()
            .map(|column| quote_identifier(column, dialect))
            .collect::<Result<Vec<_>>>()?
            .join(", "),
        tuples.join(", ")
    );
    Ok(SqlGeneration { sql, params })
}

pub fn generate_update(
    table: &str,
    data: &str,
    where_data: &str,
    dialect: SqlDialect,
    literal: bool,
) -> Result<SqlGeneration> {
    let data = parse_json(data)?
        .as_object()
        .cloned()
        .ok_or_else(|| VutilsError::InvalidInput("update data must be a JSON object".into()))?;
    let where_data = parse_json(where_data)?
        .as_object()
        .cloned()
        .ok_or_else(|| {
            VutilsError::InvalidInput("update where data must be a JSON object".into())
        })?;
    if data.is_empty() || where_data.is_empty() {
        return Err(VutilsError::InvalidInput(
            "update data and where objects cannot be empty".into(),
        ));
    }
    let mut params = Vec::new();
    let mut render_value = |value: &Value| -> Result<String> {
        if literal {
            quote_literal(value, dialect)
        } else {
            params.push(value.clone());
            Ok(placeholder(params.len(), dialect))
        }
    };
    let assignments = data
        .iter()
        .map(|(column, value)| {
            Ok(format!(
                "{} = {}",
                quote_identifier(column, dialect)?,
                render_value(value)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let predicates = where_data
        .iter()
        .map(|(column, value)| {
            if value.is_null() {
                Ok(format!("{} IS NULL", quote_identifier(column, dialect)?))
            } else {
                Ok(format!(
                    "{} = {}",
                    quote_identifier(column, dialect)?,
                    render_value(value)?
                ))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SqlGeneration {
        sql: format!(
            "UPDATE {} SET {} WHERE {};",
            quote_identifier(table, dialect)?,
            assignments.join(", "),
            predicates.join(" AND ")
        ),
        params,
    })
}

pub fn convert_placeholders(input: &str, dialect: SqlDialect, target: &str) -> Result<String> {
    validate_sql(input, dialect)?;
    if !matches!(target, "question" | "dollar" | "named") {
        return Err(VutilsError::InvalidInput(
            "placeholder target must be question, dollar, or named".into(),
        ));
    }
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut count = 0;
    let mut quote = None;
    while index < bytes.len() {
        let character = input[index..].chars().next().ok_or_else(|| {
            VutilsError::InvalidInput("invalid UTF-8 boundary while scanning SQL".into())
        })?;
        let character_length = character.len_utf8();
        if let Some(active) = quote {
            output.push(character);
            if character == active {
                if index + 1 < bytes.len() && bytes[index + 1] as char == active {
                    output.push(active);
                    index += 1;
                } else {
                    quote = None;
                }
            }
            index += character_length;
            continue;
        }
        if bytes[index..].starts_with(b"--") {
            let end = input[index..]
                .find('\n')
                .map_or(input.len(), |offset| index + offset);
            output.push_str(&input[index..end]);
            index = end;
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            let end = input[index + 2..]
                .find("*/")
                .map_or(input.len(), |offset| index + offset + 4);
            output.push_str(&input[index..end]);
            index = end;
            continue;
        }
        if let Some(end) = dollar_quoted_end(input, index) {
            output.push_str(&input[index..end]);
            index = end;
            continue;
        }
        if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
            output.push(character);
            index += character_length;
            continue;
        }
        let placeholder_length = if character == '?' {
            1
        } else if character == '$' {
            let digits = bytes[index + 1..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            usize::from(digits > 0) * (digits + 1)
        } else if matches!(character, ':' | '@')
            && index + 1 < bytes.len()
            && (bytes[index + 1] as char).is_ascii_alphabetic()
        {
            1 + bytes[index + 1..]
                .iter()
                .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
                .count()
        } else {
            0
        };
        if placeholder_length > 0 {
            count += 1;
            output.push_str(
                match target {
                    "question" => "?".into(),
                    "dollar" => format!("${count}"),
                    "named" => format!(":p{count}"),
                    _ => {
                        return Err(VutilsError::InvalidInput(
                            "placeholder target must be question, dollar, or named".into(),
                        ));
                    }
                }
                .as_str(),
            );
            index += placeholder_length;
        } else {
            output.push(character);
            index += character_length;
        }
    }
    Ok(output)
}

pub fn quote_identifier(input: &str, dialect: SqlDialect) -> Result<String> {
    if input.is_empty() || input.contains('\0') || input.split('.').any(str::is_empty) {
        return Err(VutilsError::InvalidInput(
            "SQL identifier segments cannot be empty or contain NUL".into(),
        ));
    }
    let (open, close) = match dialect {
        SqlDialect::MySql => ('`', '`'),
        SqlDialect::SqlServer => ('[', ']'),
        _ => ('"', '"'),
    };
    Ok(input
        .split('.')
        .map(|part| {
            format!(
                "{open}{}{close}",
                part.replace(close, &format!("{close}{close}"))
            )
        })
        .collect::<Vec<_>>()
        .join("."))
}

pub fn quote_literal(value: &Value, dialect: SqlDialect) -> Result<String> {
    match value {
        Value::Null => Ok("NULL".into()),
        Value::Bool(value) => Ok(match dialect {
            SqlDialect::SqlServer => {
                if *value {
                    "1"
                } else {
                    "0"
                }
            }
            _ => {
                if *value {
                    "TRUE"
                } else {
                    "FALSE"
                }
            }
        }
        .into()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => {
            if value.contains('\0') {
                return Err(VutilsError::InvalidInput(
                    "SQL string literal cannot contain NUL".into(),
                ));
            }
            Ok(format!("'{}'", value.replace('\'', "''")))
        }
        Value::Array(_) | Value::Object(_) => Ok(format!(
            "'{}'",
            serde_json::to_string(value)
                .map_err(|error| VutilsError::Message(error.to_string()))?
                .replace('\'', "''")
        )),
    }
}

fn parser_dialect(dialect: SqlDialect) -> Box<dyn ParserDialect> {
    match dialect {
        SqlDialect::Generic => Box::new(GenericDialect {}),
        SqlDialect::PostgreSql => Box::new(PostgreSqlDialect {}),
        SqlDialect::MySql => Box::new(MySqlDialect {}),
        SqlDialect::SQLite => Box::new(SQLiteDialect {}),
        SqlDialect::SqlServer => Box::new(MsSqlDialect {}),
    }
}

fn format_dialect(dialect: SqlDialect) -> FormatDialect {
    match dialect {
        SqlDialect::PostgreSql => FormatDialect::PostgreSql,
        SqlDialect::SqlServer => FormatDialect::SQLServer,
        _ => FormatDialect::Generic,
    }
}

fn statement_kind(statement: &Statement) -> &'static str {
    match statement {
        Statement::Query(_) => "select/query",
        Statement::Insert(_) => "insert",
        Statement::Update { .. } => "update",
        Statement::Delete(_) => "delete",
        Statement::CreateTable(_) => "create-table",
        Statement::AlterTable { .. } => "alter-table",
        Statement::Drop { .. } => "drop",
        _ => "other",
    }
}

fn extract_sql_metadata(
    sql: &str,
) -> Result<(Vec<String>, std::collections::BTreeMap<String, String>)> {
    let pattern = regex::Regex::new(
        r#"(?i)\b(?:from|join|into|update|table)\s+((?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[A-Za-z_][A-Za-z0-9_$]*)(?:\.(?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[A-Za-z_][A-Za-z0-9_$]*))?)(?:\s+(?:AS\s+)?([A-Za-z_][A-Za-z0-9_$]*))?"#,
    )
    .map_err(|error| VutilsError::Message(format!("internal SQL metadata pattern: {error}")))?;
    let reserved = [
        "where",
        "join",
        "left",
        "right",
        "inner",
        "outer",
        "cross",
        "on",
        "set",
        "values",
        "returning",
        "group",
        "order",
        "limit",
        "offset",
        "union",
    ];
    let mut tables = Vec::new();
    let mut aliases = std::collections::BTreeMap::new();
    for captures in pattern.captures_iter(sql) {
        let table = captures[1].to_owned();
        if !tables.contains(&table) {
            tables.push(table.clone());
        }
        if let Some(alias) = captures.get(2).map(|value| value.as_str())
            && !reserved.contains(&alias.to_ascii_lowercase().as_str())
        {
            aliases.insert(alias.to_owned(), table);
        }
    }
    Ok((tables, aliases))
}

fn extract_placeholders(sql: &str) -> Vec<String> {
    let mut values = Vec::new();
    let bytes = sql.as_bytes();
    let mut index = 0;
    let mut quote = None;
    while index < bytes.len() {
        let Some(character) = sql[index..].chars().next() else {
            break;
        };
        let character_length = character.len_utf8();
        if let Some(active) = quote {
            if character == active {
                if index + 1 < bytes.len() && bytes[index + 1] as char == active {
                    index += 1;
                } else {
                    quote = None;
                }
            }
            index += character_length;
            continue;
        }
        if bytes[index..].starts_with(b"--") {
            index = sql[index..]
                .find('\n')
                .map_or(sql.len(), |offset| index + offset);
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            index = sql[index + 2..]
                .find("*/")
                .map_or(sql.len(), |offset| index + offset + 4);
            continue;
        }
        if let Some(end) = dollar_quoted_end(sql, index) {
            index = end;
            continue;
        }
        if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
            index += character_length;
            continue;
        }
        let length = if character == '?' {
            1
        } else if character == '$' {
            let digits = bytes[index + 1..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            usize::from(digits > 0) * (digits + 1)
        } else if matches!(character, ':' | '@')
            && index + 1 < bytes.len()
            && (bytes[index + 1] as char).is_ascii_alphabetic()
        {
            1 + bytes[index + 1..]
                .iter()
                .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
                .count()
        } else {
            0
        };
        if length > 0 {
            values.push(sql[index..index + length].to_owned());
            index += length;
        } else {
            index += character_length;
        }
    }
    values
}

fn placeholder(index: usize, dialect: SqlDialect) -> String {
    match dialect {
        SqlDialect::PostgreSql => format!("${index}"),
        SqlDialect::SqlServer => format!("@p{index}"),
        _ => "?".into(),
    }
}

fn remove_sql_comments(input: &str) -> Result<String> {
    let mut output = String::new();
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut quote = None;
    while index < bytes.len() {
        let character = input[index..].chars().next().ok_or_else(|| {
            VutilsError::InvalidInput("invalid UTF-8 boundary while scanning SQL".into())
        })?;
        let character_length = character.len_utf8();
        if let Some(active) = quote {
            output.push(character);
            if character == active {
                if index + 1 < bytes.len() && bytes[index + 1] as char == active {
                    output.push(active);
                    index += 1;
                } else {
                    quote = None;
                }
            }
            index += character_length;
        } else if let Some(end) = dollar_quoted_end(input, index) {
            output.push_str(&input[index..end]);
            index = end;
        } else if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
            output.push(character);
            index += character_length;
        } else if bytes[index..].starts_with(b"--") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            let rest = &input[index + 2..];
            let end = rest.find("*/").ok_or_else(|| {
                VutilsError::InvalidInput("unterminated SQL block comment".into())
            })?;
            index += end + 4;
            if output
                .chars()
                .last()
                .is_some_and(|value| !value.is_whitespace())
                && input[index..]
                    .chars()
                    .next()
                    .is_some_and(|value| !value.is_whitespace())
            {
                output.push(' ');
            }
        } else {
            output.push(character);
            index += character_length;
        }
    }
    Ok(output)
}

fn dollar_quoted_end(input: &str, start: usize) -> Option<usize> {
    if input.as_bytes().get(start) != Some(&b'$') {
        return None;
    }
    let after_open = &input[start + 1..];
    let tag_end = after_open.find('$')?;
    let tag = &after_open[..tag_end];
    if !tag.is_empty()
        && (!tag
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            || !tag
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'))
    {
        return None;
    }
    let delimiter = &input[start..start + tag_end + 2];
    let content_start = start + delimiter.len();
    input[content_start..]
        .find(delimiter)
        .map(|offset| content_start + offset + delimiter.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_is_parameterized() {
        let generated = generate_insert(
            "users",
            r#"{"name":"O'Reilly","active":true}"#,
            SqlDialect::PostgreSql,
            false,
        )
        .unwrap();
        assert!(generated.sql.contains("$1"));
        assert!(!generated.sql.contains("O'Reilly"));
        assert_eq!(generated.params.len(), 2);
    }

    #[test]
    fn update_requires_where() {
        assert!(
            generate_update("users", r#"{"name":"x"}"#, "{}", SqlDialect::SQLite, false).is_err()
        );
    }

    #[test]
    fn placeholder_conversion_ignores_literals() {
        let converted = convert_placeholders(
            "SELECT '?' AS value WHERE id = ?",
            SqlDialect::Generic,
            "dollar",
        )
        .unwrap();
        assert_eq!(converted, "SELECT '?' AS value WHERE id = $1");
    }

    #[test]
    fn placeholder_conversion_preserves_dollar_quotes_comments_and_unicode() {
        let sql = "SELECT $$? $2$$ AS body, 'olá' AS label, $1 AS id -- ? $3\n";
        let converted = convert_placeholders(sql, SqlDialect::PostgreSql, "named").unwrap();
        assert_eq!(
            converted,
            "SELECT $$? $2$$ AS body, 'olá' AS label, :p1 AS id -- ? $3\n"
        );
    }

    #[test]
    fn placeholder_conversion_supports_sql_server_names() {
        let converted = convert_placeholders(
            "SELECT * FROM users WHERE id = @p1",
            SqlDialect::SqlServer,
            "question",
        )
        .unwrap();
        assert_eq!(converted, "SELECT * FROM users WHERE id = ?");
    }

    #[test]
    fn inspection_reports_tables_aliases_and_placeholders() {
        let inspected = inspect_sql(
            "SELECT u.id FROM users AS u JOIN teams t ON t.id = u.team_id WHERE u.id = $1",
            SqlDialect::PostgreSql,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&inspected).unwrap();
        assert_eq!(value[0]["tables"], serde_json::json!(["users", "teams"]));
        assert_eq!(value[0]["aliases"]["u"], "users");
        assert_eq!(value[0]["placeholders"], serde_json::json!(["$1"]));
    }
}
