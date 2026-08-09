use std::collections::{BTreeMap, BTreeSet};

use heck::{ToSnakeCase as _, ToUpperCamelCase as _};
use serde_json::Value;

use crate::{Result, VutilsError, data::parse_json};

#[derive(Debug, Clone, Copy)]
pub enum TargetLanguage {
    Rust,
    Kotlin,
    CSharp,
    TypeScript,
}

#[derive(Debug, Clone, PartialEq)]
struct Schema {
    kind: Kind,
    nullable: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Unknown,
    Bool,
    Integer,
    Float,
    String,
    Array(Box<Schema>),
    Object(BTreeMap<String, Schema>),
    Union(Vec<Schema>),
}

pub fn generate_types(input: &str, language: TargetLanguage, root_name: &str) -> Result<String> {
    let value = parse_json(input)?;
    if !matches!(value, Value::Object(_) | Value::Array(_)) {
        return Err(VutilsError::InvalidInput(
            "type generation requires a JSON object or array".into(),
        ));
    }
    let root = infer(&value);
    let root_name = safe_type_name(root_name);
    let mut definitions = Vec::new();
    let root_type = render_type(&root, &root_name, language, &mut definitions);
    let mut output = match language {
        TargetLanguage::Rust => "use serde::{Deserialize, Serialize};\n\n".to_owned(),
        TargetLanguage::Kotlin => {
            "import kotlinx.serialization.SerialName\nimport kotlinx.serialization.Serializable\n\n"
                .to_owned()
        }
        TargetLanguage::CSharp => {
            "using System.Collections.Generic;\nusing System.Text.Json.Serialization;\n\n"
                .to_owned()
        }
        TargetLanguage::TypeScript => String::new(),
    };
    output.push_str(&definitions.join("\n\n"));
    if !matches!(root.kind, Kind::Object(_)) {
        if !output.trim().is_empty() {
            output.push_str("\n\n");
        }
        match language {
            TargetLanguage::Rust => {
                output.push_str(&format!("pub type {root_name} = {root_type};"))
            }
            TargetLanguage::Kotlin => {
                output.push_str(&format!("typealias {root_name} = {root_type}"))
            }
            TargetLanguage::CSharp => output.push_str(&format!(
                "public sealed record {root_name}({root_type} Value);"
            )),
            TargetLanguage::TypeScript => {
                output.push_str(&format!("export type {root_name} = {root_type};"))
            }
        }
    }
    Ok(output.trim().to_owned())
}

fn infer(value: &Value) -> Schema {
    match value {
        Value::Null => Schema {
            kind: Kind::Unknown,
            nullable: true,
        },
        Value::Bool(_) => Schema {
            kind: Kind::Bool,
            nullable: false,
        },
        Value::Number(number) if number.is_i64() || number.is_u64() => Schema {
            kind: Kind::Integer,
            nullable: false,
        },
        Value::Number(_) => Schema {
            kind: Kind::Float,
            nullable: false,
        },
        Value::String(_) => Schema {
            kind: Kind::String,
            nullable: false,
        },
        Value::Array(values) => {
            let item = values.iter().map(infer).reduce(merge).unwrap_or(Schema {
                kind: Kind::Unknown,
                nullable: false,
            });
            Schema {
                kind: Kind::Array(Box::new(item)),
                nullable: false,
            }
        }
        Value::Object(object) => Schema {
            kind: Kind::Object(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), infer(value)))
                    .collect(),
            ),
            nullable: false,
        },
    }
}

fn merge(left: Schema, right: Schema) -> Schema {
    let nullable = left.nullable || right.nullable;
    let kind = match (left.kind, right.kind) {
        (Kind::Unknown, kind) | (kind, Kind::Unknown) => kind,
        (Kind::Integer, Kind::Float) | (Kind::Float, Kind::Integer) => Kind::Float,
        (left, right) if left == right => left,
        (Kind::Array(left), Kind::Array(right)) => Kind::Array(Box::new(merge(*left, *right))),
        (Kind::Object(left), Kind::Object(right)) => {
            let keys: BTreeSet<_> = left.keys().chain(right.keys()).cloned().collect();
            let fields = keys
                .into_iter()
                .map(|key| {
                    let schema = match (left.get(&key), right.get(&key)) {
                        (Some(left), Some(right)) => merge(left.clone(), right.clone()),
                        (Some(value), None) | (None, Some(value)) => Schema {
                            nullable: true,
                            ..value.clone()
                        },
                        (None, None) => Schema {
                            kind: Kind::Unknown,
                            nullable: true,
                        },
                    };
                    (key, schema)
                })
                .collect();
            Kind::Object(fields)
        }
        (Kind::Union(mut values), kind) | (kind, Kind::Union(mut values)) => {
            let candidate = Schema {
                kind,
                nullable: false,
            };
            if !values.contains(&candidate) {
                values.push(candidate);
            }
            Kind::Union(values)
        }
        (left, right) => Kind::Union(vec![
            Schema {
                kind: left,
                nullable: false,
            },
            Schema {
                kind: right,
                nullable: false,
            },
        ]),
    };
    Schema { kind, nullable }
}

fn render_type(
    schema: &Schema,
    suggested_name: &str,
    language: TargetLanguage,
    definitions: &mut Vec<String>,
) -> String {
    let base = match &schema.kind {
        Kind::Unknown => dynamic_type(language).into(),
        Kind::Bool => match language {
            TargetLanguage::Rust => "bool",
            TargetLanguage::Kotlin => "Boolean",
            TargetLanguage::CSharp => "bool",
            TargetLanguage::TypeScript => "boolean",
        }
        .into(),
        Kind::Integer => match language {
            TargetLanguage::Rust => "i64",
            TargetLanguage::Kotlin => "Long",
            TargetLanguage::CSharp => "long",
            TargetLanguage::TypeScript => "number",
        }
        .into(),
        Kind::Float => match language {
            TargetLanguage::Rust => "f64",
            TargetLanguage::Kotlin => "Double",
            TargetLanguage::CSharp => "double",
            TargetLanguage::TypeScript => "number",
        }
        .into(),
        Kind::String => match language {
            TargetLanguage::Rust => "String",
            TargetLanguage::Kotlin => "String",
            TargetLanguage::CSharp => "string",
            TargetLanguage::TypeScript => "string",
        }
        .into(),
        Kind::Array(item) => {
            let item_name = singularize(suggested_name);
            let item_type = render_type(item, &item_name, language, definitions);
            match language {
                TargetLanguage::Rust => format!("Vec<{item_type}>"),
                TargetLanguage::Kotlin => format!("List<{item_type}>"),
                TargetLanguage::CSharp => format!("IReadOnlyList<{item_type}>"),
                TargetLanguage::TypeScript => format!("{item_type}[]"),
            }
        }
        Kind::Object(fields) => {
            let definition = render_object(suggested_name, fields, language, definitions);
            if !definitions.iter().any(|existing| existing == &definition) {
                definitions.push(definition);
            }
            suggested_name.to_owned()
        }
        Kind::Union(values) if matches!(language, TargetLanguage::TypeScript) => values
            .iter()
            .map(|value| render_type(value, suggested_name, language, definitions))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" | "),
        Kind::Union(_) => dynamic_type(language).into(),
    };
    if schema.nullable {
        match language {
            TargetLanguage::Rust => format!("Option<{base}>"),
            TargetLanguage::Kotlin | TargetLanguage::CSharp => format!("{base}?"),
            TargetLanguage::TypeScript => format!("{base} | null"),
        }
    } else {
        base
    }
}

fn render_object(
    name: &str,
    fields: &BTreeMap<String, Schema>,
    language: TargetLanguage,
    definitions: &mut Vec<String>,
) -> String {
    let mut used_type_names = BTreeSet::new();
    let mut used_properties = BTreeSet::new();
    if matches!(language, TargetLanguage::CSharp) {
        used_properties.insert(name.to_owned());
    }
    let rendered: Vec<_> = fields
        .iter()
        .map(|(json_name, schema)| {
            let quoted_name = json_string_literal(json_name);
            let type_name = unique_type_name(
                format!("{name}{}", safe_type_name(json_name)),
                &mut used_type_names,
            );
            let field_type = render_type(schema, &type_name, language, definitions);
            match language {
                TargetLanguage::Rust => {
                    let property =
                        unique_rust_identifier(rust_identifier(json_name), &mut used_properties);
                    let serialized_name = property.strip_prefix("r#").unwrap_or(&property);
                    let rename = if serialized_name != json_name {
                        format!("    #[serde(rename = {quoted_name})]\n")
                    } else {
                        String::new()
                    };
                    format!("{rename}    pub {property}: {field_type},")
                }
                TargetLanguage::Kotlin => {
                    let property = unique_kotlin_identifier(
                        kotlin_identifier(json_name),
                        &mut used_properties,
                    );
                    let rename = if property != *json_name {
                        format!("    @SerialName({quoted_name})\n")
                    } else {
                        String::new()
                    };
                    let default = if schema.nullable { " = null" } else { "" };
                    format!("{rename}    val {property}: {field_type}{default}")
                }
                TargetLanguage::CSharp => {
                    let property =
                        unique_type_name(safe_type_name(json_name), &mut used_properties);
                    let rename = if property != *json_name {
                        format!("    [property: JsonPropertyName({quoted_name})]\n")
                    } else {
                        String::new()
                    };
                    format!("{rename}    {field_type} {property}")
                }
                TargetLanguage::TypeScript => {
                    let optional = if schema.nullable { "?" } else { "" };
                    format!(
                        "  {}{optional}: {field_type};",
                        typescript_property(json_name)
                    )
                }
            }
        })
        .collect();
    match language {
        TargetLanguage::Rust => format!(
            "#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct {name} {{\n{}\n}}",
            rendered.join("\n")
        ),
        TargetLanguage::Kotlin => format!(
            "@Serializable\ndata class {name}(\n{}\n)",
            rendered.join(",\n")
        ),
        TargetLanguage::CSharp => {
            format!("public sealed record {name}(\n{}\n);", rendered.join(",\n"))
        }
        TargetLanguage::TypeScript => {
            format!("export interface {name} {{\n{}\n}}", rendered.join("\n"))
        }
    }
}

fn dynamic_type(language: TargetLanguage) -> &'static str {
    match language {
        TargetLanguage::Rust => "serde_json::Value",
        TargetLanguage::Kotlin => "kotlinx.serialization.json.JsonElement",
        TargetLanguage::CSharp => "object",
        TargetLanguage::TypeScript => "unknown",
    }
}

fn safe_type_name(value: &str) -> String {
    let candidate = value.to_upper_camel_case();
    if candidate.is_empty() {
        "Root".into()
    } else if candidate.starts_with(|character: char| character.is_ascii_digit()) {
        format!("Type{candidate}")
    } else {
        candidate
    }
}

fn singularize(value: &str) -> String {
    value
        .strip_suffix("ies")
        .map(|stem| format!("{stem}y"))
        .or_else(|| value.strip_suffix('s').map(str::to_owned))
        .unwrap_or_else(|| format!("{value}Item"))
}

fn rust_identifier(value: &str) -> String {
    let value = value.to_snake_case();
    let candidate = if value.is_empty() {
        "field".to_owned()
    } else if value.starts_with(|character: char| character.is_ascii_digit()) {
        format!("field_{value}")
    } else {
        value.to_owned()
    };
    if matches!(candidate.as_str(), "crate" | "self" | "super" | "Self") {
        return format!("field_{candidate}");
    }
    if matches!(
        candidate.as_str(),
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    ) {
        format!("r#{candidate}")
    } else {
        candidate
    }
}

fn unique_type_name(candidate: String, used: &mut BTreeSet<String>) -> String {
    unique_identifier(candidate, used, |base, suffix| format!("{base}{suffix}"))
}

fn unique_rust_identifier(candidate: String, used: &mut BTreeSet<String>) -> String {
    unique_identifier(candidate, used, |base, suffix| format!("{base}_{suffix}"))
}

fn unique_kotlin_identifier(candidate: String, used: &mut BTreeSet<String>) -> String {
    unique_identifier(candidate, used, |base, suffix| {
        base.strip_prefix('`')
            .and_then(|value| value.strip_suffix('`'))
            .map_or_else(
                || format!("{base}{suffix}"),
                |inner| format!("`{inner}{suffix}`"),
            )
    })
}

fn unique_identifier(
    candidate: String,
    used: &mut BTreeSet<String>,
    with_suffix: impl Fn(&str, usize) -> String,
) -> String {
    if used.insert(candidate.clone()) {
        return candidate;
    }
    for suffix in 2.. {
        let value = with_suffix(&candidate, suffix);
        if used.insert(value.clone()) {
            return value;
        }
    }
    candidate
}

fn kotlin_identifier(value: &str) -> String {
    let candidate = value
        .to_snake_case()
        .split('_')
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                part.to_owned()
            } else {
                part.to_upper_camel_case()
            }
        })
        .collect::<String>();
    let candidate = if candidate.is_empty() {
        "field".to_owned()
    } else if candidate.starts_with(|character: char| character.is_ascii_digit()) {
        format!("field{candidate}")
    } else {
        candidate
    };
    if matches!(
        candidate.as_str(),
        "as" | "break"
            | "class"
            | "continue"
            | "do"
            | "else"
            | "false"
            | "for"
            | "fun"
            | "if"
            | "in"
            | "interface"
            | "is"
            | "null"
            | "object"
            | "package"
            | "return"
            | "super"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typealias"
            | "typeof"
            | "val"
            | "var"
            | "when"
            | "while"
    ) {
        format!("`{candidate}`")
    } else {
        candidate
    }
}

fn typescript_property(value: &str) -> String {
    if value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
    {
        value.to_owned()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "\"field\"".into())
    }
}

fn json_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"field\"".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_optional_fields_from_array() {
        let source = r#"[{"id":1,"name":"A"},{"id":2}]"#;
        let generated = generate_types(source, TargetLanguage::TypeScript, "UserList").unwrap();
        assert!(generated.contains("name?: string | null"));

        let kotlin = generate_types(source, TargetLanguage::Kotlin, "UserList").unwrap();
        assert!(kotlin.contains("val name: String? = null"));
    }

    #[test]
    fn generates_each_language() {
        let source = r#"{"user":{"first_name":"Ana"}}"#;
        for language in [
            TargetLanguage::Rust,
            TargetLanguage::Kotlin,
            TargetLanguage::CSharp,
            TargetLanguage::TypeScript,
        ] {
            assert!(!generate_types(source, language, "Root").unwrap().is_empty());
        }
    }

    #[test]
    fn escapes_reserved_and_invalid_property_names() {
        let source = r#"{"type":1,"when":false,"1st-value":true,"":null}"#;
        let rust = generate_types(source, TargetLanguage::Rust, "Root").unwrap();
        assert!(rust.contains("pub r#type: i64"));
        assert!(rust.contains("pub field_1st_value: bool"));
        assert!(rust.contains("pub field: Option<serde_json::Value>"));
        syn::parse_file(&rust).unwrap();

        let kotlin = generate_types(source, TargetLanguage::Kotlin, "Root").unwrap();
        assert!(kotlin.contains("val `when`: Boolean"));
        assert!(kotlin.contains("@SerialName(\"1st-value\")"));
    }

    #[test]
    fn disambiguates_normalized_field_and_type_names() {
        let source = r#"{"a-b":{"x":1},"a_b":{"y":2}}"#;
        let rust = generate_types(source, TargetLanguage::Rust, "Root").unwrap();
        assert!(rust.contains("pub a_b: RootAB"));
        assert!(rust.contains("pub a_b_2: RootAB2"));
        assert!(rust.contains("pub struct RootAB "));
        assert!(rust.contains("pub struct RootAB2 "));
        syn::parse_file(&rust).unwrap();
    }
}
