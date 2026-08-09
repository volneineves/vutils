use std::collections::{BTreeMap, BTreeSet};

use quick_xml::{Reader, Writer, events::Event};
use serde::Deserialize as _;
use serde_json::{Map, Value, json};

use crate::{Result, VutilsError};

pub fn json_pretty(input: &str) -> Result<String> {
    serde_json::to_string_pretty(&parse_json(input)?)
        .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_minify(input: &str) -> Result<String> {
    serde_json::to_string(&parse_json(input)?)
        .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn parse_json(input: &str) -> Result<Value> {
    serde_json::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid JSON: {error}")))
}

pub fn json_escape(input: &str) -> Result<String> {
    serde_json::to_string(input).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_unescape(input: &str) -> Result<String> {
    serde_json::from_str::<String>(input).map_err(|error| {
        VutilsError::InvalidInput(format!("input must be a JSON string literal: {error}"))
    })
}

pub fn json_sort_keys(input: &str) -> Result<String> {
    let sorted = sort_value(parse_json(input)?);
    serde_json::to_string_pretty(&sorted).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_query(input: &str, expression: &str) -> Result<String> {
    let value = parse_json(input)?;
    let pointer = expression_to_pointer(expression)?;
    let selected = value.pointer(&pointer).ok_or_else(|| {
        VutilsError::InvalidInput(format!("path `{expression}` did not match a value"))
    })?;
    serde_json::to_string_pretty(selected).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_flatten(input: &str) -> Result<String> {
    let value = parse_json(input)?;
    let mut flattened = Map::new();
    flatten_value(&value, "", &mut flattened);
    serde_json::to_string_pretty(&Value::Object(flattened))
        .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_unflatten(input: &str) -> Result<String> {
    let flat = parse_json(input)?;
    let object = flat
        .as_object()
        .ok_or_else(|| VutilsError::InvalidInput("unflatten input must be a JSON object".into()))?;
    let mut root = Value::Null;
    for (pointer, value) in object {
        if pointer.is_empty() {
            root = value.clone();
            continue;
        }
        insert_pointer(&mut root, pointer, value.clone())?;
    }
    serde_json::to_string_pretty(&root).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_diff(left: &str, right: &str, patch: bool) -> Result<String> {
    let left = parse_json(left)?;
    let right = parse_json(right)?;
    if patch {
        let mut operations = Vec::new();
        build_patch(&left, &right, "", &mut operations);
        serde_json::to_string_pretty(&operations)
            .map_err(|error| VutilsError::Message(error.to_string()))
    } else {
        let left = serde_json::to_string_pretty(&left)
            .map_err(|error| VutilsError::Message(error.to_string()))?;
        let right = serde_json::to_string_pretty(&right)
            .map_err(|error| VutilsError::Message(error.to_string()))?;
        Ok(similar::TextDiff::from_lines(&left, &right)
            .unified_diff()
            .header("left", "right")
            .to_string())
    }
}

pub fn json_to_yaml(input: &str) -> Result<String> {
    serde_yaml_ng::to_string(&parse_json(input)?)
        .map_err(|error| VutilsError::InvalidInput(format!("cannot convert JSON to YAML: {error}")))
}

pub fn yaml_to_json(input: &str) -> Result<String> {
    ensure_single_yaml_document(input)?;
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid YAML: {error}")))?;
    let json_value = serde_json::to_value(value).map_err(|error| {
        VutilsError::InvalidInput(format!("YAML is not JSON-compatible: {error}"))
    })?;
    serde_json::to_string_pretty(&json_value)
        .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn yaml_pretty(input: &str) -> Result<String> {
    ensure_single_yaml_document(input)?;
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid YAML: {error}")))?;
    serde_yaml_ng::to_string(&value)
        .map_err(|error| VutilsError::Message(format!("failed to format YAML: {error}")))
}

pub fn yaml_split(input: &str) -> Result<Vec<String>> {
    let mut documents = Vec::new();
    for document in serde_yaml_ng::Deserializer::from_str(input) {
        let value = serde_yaml_ng::Value::deserialize(document)
            .map_err(|error| VutilsError::InvalidInput(format!("invalid YAML: {error}")))?;
        documents.push(
            serde_yaml_ng::to_string(&value)
                .map_err(|error| VutilsError::Message(error.to_string()))?,
        );
    }
    if documents.is_empty() {
        return Err(VutilsError::InvalidInput(
            "YAML contains no documents".into(),
        ));
    }
    Ok(documents)
}

pub fn yaml_join(documents: &[String]) -> Result<String> {
    if documents.is_empty() {
        return Err(VutilsError::InvalidInput(
            "at least one YAML document is required".into(),
        ));
    }
    for document in documents {
        let _: serde_yaml_ng::Value = serde_yaml_ng::from_str(document)
            .map_err(|error| VutilsError::InvalidInput(format!("invalid YAML: {error}")))?;
    }
    Ok(documents
        .iter()
        .map(|document| format!("---\n{}", document.trim_start_matches("---\n").trim_end()))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn json_to_toml(input: &str) -> Result<String> {
    let value = parse_json(input)?;
    toml::to_string_pretty(&value)
        .map_err(|error| VutilsError::InvalidInput(format!("cannot convert JSON to TOML: {error}")))
}

pub fn toml_to_json(input: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid TOML: {error}")))?;
    serde_json::to_string_pretty(&value).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn toml_pretty(input: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid TOML: {error}")))?;
    toml::to_string_pretty(&value).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn csv_to_json(input: &str) -> Result<String> {
    let mut reader = csv::Reader::from_reader(input.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| VutilsError::InvalidInput(format!("invalid CSV header: {error}")))?
        .clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record
            .map_err(|error| VutilsError::InvalidInput(format!("invalid CSV row: {error}")))?;
        let object = headers
            .iter()
            .zip(record.iter())
            .map(|(key, value)| (key.to_owned(), Value::String(value.to_owned())))
            .collect();
        rows.push(Value::Object(object));
    }
    serde_json::to_string_pretty(&rows).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn json_to_csv(input: &str, stringify_nested: bool) -> Result<String> {
    let value = parse_json(input)?;
    let rows = value.as_array().ok_or_else(|| {
        VutilsError::InvalidInput("JSON to CSV expects an array of objects".into())
    })?;
    let mut headers = BTreeSet::new();
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| VutilsError::InvalidInput("every JSON row must be an object".into()))?;
        headers.extend(object.keys().cloned());
    }
    let headers: Vec<_> = headers.into_iter().collect();
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(&headers).map_err(csv_error)?;
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| VutilsError::InvalidInput("every JSON row must be an object".into()))?;
        let values = headers
            .iter()
            .map(|header| csv_cell(object.get(header), stringify_nested))
            .collect::<Result<Vec<_>>>()?;
        writer.write_record(values).map_err(csv_error)?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|error| VutilsError::Message(error.to_string()))?;
    String::from_utf8(bytes).map_err(|error| {
        VutilsError::Message(format!("CSV writer produced invalid UTF-8: {error}"))
    })
}

pub fn xml_pretty(input: &str) -> Result<String> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    loop {
        let event = reader
            .read_event()
            .map_err(|error| VutilsError::InvalidInput(format!("invalid XML: {error}")))?;
        let end = matches!(event, Event::Eof);
        writer
            .write_event(event)
            .map_err(|error| VutilsError::Message(format!("failed to format XML: {error}")))?;
        if end {
            break;
        }
    }
    String::from_utf8(writer.into_inner())
        .map_err(|error| VutilsError::Message(format!("XML output is not UTF-8: {error}")))
}

pub fn dotenv_parse(input: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for (index, original) in input.lines().enumerate() {
        let line = original.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, raw_value) = line
            .split_once('=')
            .ok_or_else(|| VutilsError::InvalidInput(format!("invalid .env line {}", index + 1)))?;
        let key = key.trim();
        if key.is_empty()
            || !key.chars().enumerate().all(|(position, character)| {
                character == '_'
                    || character.is_ascii_alphanumeric()
                        && (position > 0 || !character.is_ascii_digit())
            })
        {
            return Err(VutilsError::InvalidInput(format!(
                "invalid .env key `{key}` on line {}",
                index + 1
            )));
        }
        let value = parse_dotenv_value(raw_value.trim(), index + 1)?;
        if values.insert(key.to_owned(), value).is_some() {
            return Err(VutilsError::InvalidInput(format!(
                "duplicate .env key `{key}`"
            )));
        }
    }
    Ok(values)
}

pub fn dotenv_to_json(input: &str) -> Result<String> {
    serde_json::to_string_pretty(&dotenv_parse(input)?)
        .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn dotenv_sort(input: &str) -> Result<String> {
    Ok(dotenv_parse(input)?
        .into_iter()
        .map(|(key, value)| format!("{key}={}", quote_dotenv(&value)))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn dotenv_diff(left: &str, right: &str, show_values: bool) -> Result<String> {
    let left = dotenv_parse(left)?;
    let right = dotenv_parse(right)?;
    let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
    let mut changes = Vec::new();
    for key in keys {
        match (left.get(key), right.get(key)) {
            (Some(_), None) => changes.push(format!("- {key}")),
            (None, Some(_)) => changes.push(format!("+ {key}")),
            (Some(before), Some(after)) if before != after => {
                if show_values {
                    changes.push(format!("~ {key}: {before:?} -> {after:?}"));
                } else {
                    changes.push(format!("~ {key}: <redacted> -> <redacted>"));
                }
            }
            _ => {}
        }
    }
    Ok(changes.join("\n"))
}

pub fn validate_json_schema(instance: &str, schema: &str) -> Result<()> {
    let instance = parse_json(instance)?;
    let schema = parse_json(schema)?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid JSON Schema: {error}")))?;
    let errors: Vec<_> = validator
        .iter_errors(&instance)
        .map(|error| error.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(VutilsError::InvalidInput(format!(
            "JSON Schema validation failed:\n{}",
            errors.join("\n")
        )))
    }
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted: BTreeMap<_, _> = object
                .into_iter()
                .map(|(key, value)| (key, sort_value(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        scalar => scalar,
    }
}

fn expression_to_pointer(expression: &str) -> Result<String> {
    if expression.starts_with('/') {
        return Ok(expression.to_owned());
    }
    if expression == "$" {
        return Ok(String::new());
    }
    let remainder = expression.strip_prefix("$.").ok_or_else(|| {
        VutilsError::InvalidInput(
            "path must be JSON Pointer or simple JSONPath starting with $.".into(),
        )
    })?;
    let mut segments = Vec::new();
    for segment in remainder.split('.') {
        let mut rest = segment;
        while let Some(open) = rest.find('[') {
            let key = &rest[..open];
            if !key.is_empty() {
                segments.push(key.to_owned());
            }
            let close = rest[open + 1..].find(']').ok_or_else(|| {
                VutilsError::InvalidInput(format!("unclosed array index in `{expression}`"))
            })? + open
                + 1;
            segments.push(rest[open + 1..close].to_owned());
            rest = &rest[close + 1..];
        }
        if !rest.is_empty() {
            segments.push(rest.to_owned());
        }
    }
    Ok(format!(
        "/{}",
        segments
            .iter()
            .map(|segment| segment.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    ))
}

fn flatten_value(value: &Value, pointer: &str, output: &mut Map<String, Value>) {
    match value {
        Value::Object(object) if !object.is_empty() => {
            for (key, child) in object {
                let key = key.replace('~', "~0").replace('/', "~1");
                flatten_value(child, &format!("{pointer}/{key}"), output);
            }
        }
        Value::Array(array) if !array.is_empty() => {
            for (index, child) in array.iter().enumerate() {
                flatten_value(child, &format!("{pointer}/{index}"), output);
            }
        }
        _ => {
            output.insert(pointer.to_owned(), value.clone());
        }
    }
}

fn insert_pointer(root: &mut Value, pointer: &str, value: Value) -> Result<()> {
    if !pointer.starts_with('/') {
        return Err(VutilsError::InvalidInput(format!(
            "flattened key `{pointer}` is not a JSON Pointer"
        )));
    }
    let segments: Vec<String> = pointer[1..]
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect();
    insert_segments(root, &segments, value)
}

fn insert_segments(current: &mut Value, segments: &[String], value: Value) -> Result<()> {
    if segments.is_empty() {
        if !current.is_null() {
            return Err(VutilsError::InvalidInput(
                "flattened paths contain an overlapping value".into(),
            ));
        }
        *current = value;
        return Ok(());
    }
    let segment = &segments[0];
    let next_is_index = segments
        .get(1)
        .is_some_and(|next| next.parse::<usize>().is_ok());
    if let Ok(index) = segment.parse::<usize>() {
        if index > 100_000 {
            return Err(VutilsError::InvalidInput(
                "flattened array index cannot exceed 100000".into(),
            ));
        }
        if current.is_null() {
            *current = Value::Array(Vec::new());
        }
        let array = current.as_array_mut().ok_or_else(|| {
            VutilsError::InvalidInput("flattened paths mix object and array shapes".into())
        })?;
        while array.len() <= index {
            array.push(Value::Null);
        }
        insert_segments(&mut array[index], &segments[1..], value)
    } else {
        if current.is_null() {
            *current = Value::Object(Map::new());
        }
        let object = current.as_object_mut().ok_or_else(|| {
            VutilsError::InvalidInput("flattened paths mix object and array shapes".into())
        })?;
        let child = object.entry(segment.clone()).or_insert_with(|| {
            if next_is_index {
                Value::Array(Vec::new())
            } else {
                Value::Null
            }
        });
        insert_segments(child, &segments[1..], value)
    }
}

fn build_patch(left: &Value, right: &Value, path: &str, operations: &mut Vec<Value>) {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().filter(|key| !right.contains_key(*key)) {
                operations.push(json!({"op": "remove", "path": join_pointer(path, key)}));
            }
            for (key, value) in right {
                let child_path = join_pointer(path, key);
                if let Some(before) = left.get(key) {
                    build_patch(before, value, &child_path, operations);
                } else {
                    operations.push(json!({"op": "add", "path": child_path, "value": value}));
                }
            }
        }
        _ if left != right => {
            operations.push(json!({"op": "replace", "path": path, "value": right}));
        }
        _ => {}
    }
}

fn join_pointer(base: &str, key: &str) -> String {
    format!("{base}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn ensure_single_yaml_document(input: &str) -> Result<()> {
    let count = serde_yaml_ng::Deserializer::from_str(input).count();
    match count {
        1 => Ok(()),
        0 => Err(VutilsError::InvalidInput(
            "YAML contains no document".into(),
        )),
        _ => Err(VutilsError::InvalidInput(
            "operation requires exactly one YAML document; use `yaml split` first".into(),
        )),
    }
}

fn csv_cell(value: Option<&Value>, stringify_nested: bool) -> Result<String> {
    match value.unwrap_or(&Value::Null) {
        Value::Null => Ok(String::new()),
        Value::String(value) => Ok(value.clone()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        nested if stringify_nested => {
            serde_json::to_string(nested).map_err(|error| VutilsError::Message(error.to_string()))
        }
        _ => Err(VutilsError::InvalidInput(
            "nested JSON requires --stringify-nested for CSV conversion".into(),
        )),
    }
}

fn csv_error(error: csv::Error) -> VutilsError {
    VutilsError::Message(format!("failed to write CSV: {error}"))
}

fn parse_dotenv_value(value: &str, line: usize) -> Result<String> {
    if let Some(quoted) = value.strip_prefix('"') {
        let inner = quoted.strip_suffix('"').ok_or_else(|| {
            VutilsError::InvalidInput(format!("unterminated double quote on .env line {line}"))
        })?;
        return json_unescape(&format!("\"{inner}\""));
    }
    if let Some(quoted) = value.strip_prefix('\'') {
        return quoted.strip_suffix('\'').map(str::to_owned).ok_or_else(|| {
            VutilsError::InvalidInput(format!("unterminated single quote on .env line {line}"))
        });
    }
    Ok(value
        .split(" #")
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_owned())
}

fn quote_dotenv(value: &str) -> String {
    if value.is_empty()
        || value
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '#' | '"' | '\''))
    {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_round_trip() {
        let source = r#"{"a":{"b":[1,2]},"empty":{}}"#;
        let flat = json_flatten(source).unwrap();
        let restored = parse_json(&json_unflatten(&flat).unwrap()).unwrap();
        assert_eq!(restored, parse_json(source).unwrap());
    }

    #[test]
    fn json_patch_reports_changes() {
        let patch =
            parse_json(&json_diff(r#"{"a":1}"#, r#"{"a":2,"b":3}"#, true).unwrap()).unwrap();
        assert_eq!(patch.as_array().unwrap().len(), 2);
    }

    #[test]
    fn dotenv_rejects_duplicates_and_redacts_diff() {
        assert!(dotenv_parse("A=1\nA=2").is_err());
        assert!(
            dotenv_diff("TOKEN=a", "TOKEN=b", false)
                .unwrap()
                .contains("<redacted>")
        );
    }

    #[test]
    fn csv_round_trip_for_flat_rows() {
        let csv = json_to_csv(r#"[{"a":"x","b":2}]"#, false).unwrap();
        let json = parse_json(&csv_to_json(&csv).unwrap()).unwrap();
        assert_eq!(json[0]["a"], "x");
    }

    #[test]
    fn unflatten_rejects_overlapping_paths_in_any_order() {
        assert!(json_unflatten(r#"{"/a/b":1,"/a":2}"#).is_err());
        assert!(json_unflatten(r#"{"/a":2,"/a/b":1}"#).is_err());
    }
}
