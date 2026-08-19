use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use vutils::{Result, VutilsError};
use walkdir::{DirEntry, WalkDir};

const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];
const MAX_REMOTE_OPENAPI_BYTES: u64 = 10 * 1024 * 1024;
const REMOTE_OPENAPI_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncMode {
    Check,
    Preview,
    Sync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupBy {
    Tags,
    Path,
}

#[derive(Debug)]
pub(crate) struct SyncRequest {
    pub(crate) collection: PathBuf,
    pub(crate) openapi: PathBuf,
    pub(crate) mode: SyncMode,
    pub(crate) json: bool,
    pub(crate) group_by: GroupBy,
}

#[derive(Debug)]
pub(crate) struct SyncOutput {
    pub(crate) success: bool,
    pub(crate) stdout: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Endpoint {
    method: String,
    path: String,
    name: String,
    folder: Option<String>,
    content: String,
    signature: RequestSignature,
}

impl Endpoint {
    fn id(&self) -> String {
        format!("{}:{}", self.method, self.path)
    }
}

#[derive(Clone, Debug)]
struct ExistingRequest {
    file: PathBuf,
    relative_file: PathBuf,
    content: String,
    method: String,
    path: String,
    signature: RequestSignature,
}

impl ExistingRequest {
    fn id(&self) -> String {
        format!("{}:{}", self.method, self.path)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RequestSignature {
    params: BTreeSet<String>,
    headers: BTreeSet<String>,
    body_mode: String,
    body_shape: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
struct DriftReport {
    in_sync: bool,
    missing: Vec<DriftItem>,
    modified: Vec<DriftItem>,
    stale: Vec<DriftItem>,
}

#[derive(Debug, Serialize)]
struct DriftItem {
    method: String,
    path: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    changes: Vec<String>,
}

#[derive(Clone, Debug)]
struct Pair {
    name: String,
    value: String,
    enabled: bool,
}

#[derive(Clone, Debug)]
struct Block<'a> {
    name: &'a str,
    source: &'a str,
}

pub(crate) fn validate_collection(path: &Path) -> Result<PathBuf> {
    let path = local_file_path(path, "Bruno collection")?;
    let mut path = canonicalize(&path, "Bruno collection")?;
    if path.is_file()
        && path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("bruno.json"))
    {
        path.pop();
    }
    if !path.is_dir() {
        return Err(VutilsError::InvalidInput(format!(
            "Bruno collection `{}` is not a directory",
            path.display()
        )));
    }
    if path.join("opencollection.yml").is_file() {
        return Err(VutilsError::Unsupported(
            "Vruno currently supports classic Bruno collections (`bruno.json` + `.bru`) only; OpenCollection YAML is not modified to avoid lossy conversions".into(),
        ));
    }
    let config_path = path.join("bruno.json");
    if !config_path.is_file() {
        return Err(VutilsError::InvalidInput(format!(
            "`{}` is not a classic Bruno collection: bruno.json was not found",
            path.display()
        )));
    }
    let config = fs::read_to_string(&config_path).map_err(|source| VutilsError::Read {
        path: config_path.clone(),
        source,
    })?;
    serde_json::from_str::<Value>(&config).map_err(|error| {
        VutilsError::InvalidInput(format!(
            "invalid Bruno collection config `{}`: {error}",
            config_path.display()
        ))
    })?;
    Ok(path)
}

pub(crate) fn validate_openapi(path: &Path) -> Result<PathBuf> {
    let (path, document) = load_openapi(path)?;
    validate_document(&path, &document)?;
    Ok(path)
}

pub(crate) fn run(request: &SyncRequest) -> Result<SyncOutput> {
    let collection = validate_collection(&request.collection)?;
    let (openapi, document) = load_openapi(&request.openapi)?;
    validate_document(&openapi, &document)?;
    let endpoints = build_endpoints(&document, request.group_by)?;
    let existing = scan_collection(&collection)?;
    reject_duplicate_requests(&existing)?;
    let report = compare(&endpoints, &existing);

    if request.mode == SyncMode::Sync {
        apply(&collection, &endpoints, &existing, &report)?;
    }

    let stdout = if request.json {
        serde_json::to_vec_pretty(&report).map_err(|error| {
            VutilsError::Message(format!("failed to serialize drift report: {error}"))
        })?
    } else {
        render_text(&report, request.mode).into_bytes()
    };
    Ok(SyncOutput {
        success: request.mode != SyncMode::Check || report.in_sync,
        stdout,
    })
}

fn load_openapi(path: &Path) -> Result<(PathBuf, Value)> {
    if let Some(url) = remote_url(path)? {
        let format = openapi_format(Path::new(url.path()))?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(REMOTE_OPENAPI_TIMEOUT))
            .user_agent(concat!("vutils/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let mut response = agent.get(url.as_str()).call().map_err(|error| {
            VutilsError::Message(format!("failed to download OpenAPI URL `{url}`: {error}"))
        })?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_REMOTE_OPENAPI_BYTES)
            .read_to_vec()
            .map_err(|error| {
                VutilsError::Message(format!("failed to read OpenAPI URL `{url}`: {error}"))
            })?;
        let contents = String::from_utf8(bytes).map_err(|error| {
            VutilsError::InvalidInput(format!(
                "OpenAPI URL `{url}` did not return valid UTF-8: {error}"
            ))
        })?;
        let location = PathBuf::from(url.as_str());
        return parse_openapi(&location, &contents, format).map(|document| (location, document));
    }

    let path = local_file_path(path, "OpenAPI file")?;
    let path = canonicalize(&path, "OpenAPI file")?;
    if !path.is_file() {
        return Err(VutilsError::InvalidInput(format!(
            "OpenAPI path `{}` is not a file",
            path.display()
        )));
    }
    let format = openapi_format(&path)?;
    let contents = fs::read_to_string(&path).map_err(|source| VutilsError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse_openapi(&path, &contents, format).map(|document| (path, document))
}

#[derive(Clone, Copy)]
enum OpenApiFormat {
    Json,
    Yaml,
}

fn remote_url(path: &Path) -> Result<Option<url::Url>> {
    let Some(value) = path.to_str() else {
        return Ok(None);
    };
    let Ok(url) = url::Url::parse(value) else {
        return Ok(None);
    };
    match url.scheme() {
        "http" | "https" => Ok(Some(url)),
        "file" => Ok(None),
        scheme if value.contains("://") => Err(VutilsError::Unsupported(format!(
            "OpenAPI URL scheme `{scheme}` is not supported; use http:// or https://"
        ))),
        _ => Ok(None),
    }
}

fn local_file_path(path: &Path, label: &str) -> Result<PathBuf> {
    let Some(value) = path.to_str() else {
        return Ok(path.to_path_buf());
    };
    let Ok(url) = url::Url::parse(value) else {
        return Ok(path.to_path_buf());
    };
    match url.scheme() {
        "file" => url.to_file_path().map_err(|()| {
            VutilsError::InvalidInput(format!(
                "{label} URL `{url}` cannot be converted to a local path"
            ))
        }),
        "http" | "https" => Err(VutilsError::Unsupported(format!(
            "remote {label} URL `{url}` is not a writable local collection"
        ))),
        scheme if value.contains("://") => Err(VutilsError::Unsupported(format!(
            "{label} URL scheme `{scheme}` is not supported"
        ))),
        _ => Ok(path.to_path_buf()),
    }
}

fn openapi_format(path: &Path) -> Result<OpenApiFormat> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => Ok(OpenApiFormat::Json),
        Some("yaml" | "yml") => Ok(OpenApiFormat::Yaml),
        _ => Err(VutilsError::InvalidInput(
            "OpenAPI path or URL must use a .json, .yaml, or .yml extension".into(),
        )),
    }
}

fn parse_openapi(path: &Path, contents: &str, format: OpenApiFormat) -> Result<Value> {
    match format {
        OpenApiFormat::Json => serde_json::from_str(contents).map_err(|error| {
            VutilsError::InvalidInput(format!(
                "invalid OpenAPI JSON `{}`: {error}",
                path.display()
            ))
        }),
        OpenApiFormat::Yaml => serde_yaml_ng::from_str(contents).map_err(|error| {
            VutilsError::InvalidInput(format!(
                "invalid OpenAPI YAML `{}`: {error}",
                path.display()
            ))
        }),
    }
}

fn validate_document(path: &Path, document: &Value) -> Result<()> {
    let valid_version = document
        .get("openapi")
        .and_then(Value::as_str)
        .is_some_and(|version| version.starts_with("3."));
    let has_paths = document.get("paths").is_some_and(Value::is_object);
    if !valid_version || !has_paths {
        return Err(VutilsError::InvalidInput(format!(
            "`{}` must be an OpenAPI 3.x document with a paths object; Swagger 2.0 is not supported",
            path.display()
        )));
    }
    validate_references(document, document)?;
    Ok(())
}

fn validate_references(document: &Value, value: &Value) -> Result<()> {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                if !reference.starts_with("#/") {
                    return Err(VutilsError::Unsupported(format!(
                        "external OpenAPI reference `{reference}` is not supported yet; bundle the specification into one JSON or YAML file before syncing"
                    )));
                }
                let mut target = document;
                for component in reference.trim_start_matches("#/").split('/') {
                    let key = component.replace("~1", "/").replace("~0", "~");
                    target = target.get(&key).ok_or_else(|| {
                        VutilsError::InvalidInput(format!(
                            "OpenAPI reference `{reference}` does not resolve"
                        ))
                    })?;
                }
            }
            for nested in object.values() {
                validate_references(document, nested)?;
            }
        }
        Value::Array(values) => {
            for nested in values {
                validate_references(document, nested)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn build_endpoints(document: &Value, group_by: GroupBy) -> Result<Vec<Endpoint>> {
    let paths = document["paths"]
        .as_object()
        .expect("validated OpenAPI paths");
    let mut endpoints = Vec::new();
    let mut used_names = BTreeSet::new();
    let mut sequence_by_folder = HashMap::<Option<String>, u64>::new();

    for (raw_path, path_item) in paths {
        let Some(path_object) = path_item.as_object() else {
            continue;
        };
        let inherited = parameters(document, path_object.get("parameters"));
        for method in HTTP_METHODS {
            let Some(operation) = path_object.get(*method).and_then(Value::as_object) else {
                continue;
            };
            let operation_parameters = parameters(document, operation.get("parameters"));
            let merged_parameters = merge_parameters(inherited.clone(), operation_parameters);
            let path = normalize_spec_path(raw_path);
            let mut name = operation
                .get("summary")
                .or_else(|| operation.get("operationId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{} {raw_path}", method.to_uppercase()));
            name = unique_name(name, method, &mut used_names);
            let folder = endpoint_folder(operation, raw_path, group_by);
            let sequence = sequence_by_folder.entry(folder.clone()).or_default();
            *sequence += 1;
            let generated = generate_request(
                document,
                method,
                &path,
                &name,
                *sequence,
                operation,
                &merged_parameters,
            );
            let signature = signature_from_content(&generated).ok_or_else(|| {
                VutilsError::Message(format!(
                    "failed to analyze generated endpoint {method} {path}"
                ))
            })?;
            endpoints.push(Endpoint {
                method: method.to_uppercase(),
                path,
                name,
                folder,
                content: generated,
                signature,
            });
        }
    }
    endpoints.sort_by_key(Endpoint::id);
    Ok(endpoints)
}

fn parameters(document: &Value, value: Option<&Value>) -> Vec<Map<String, Value>> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|parameter| resolve_value(document, parameter).as_object().cloned())
        .collect()
}

fn merge_parameters(
    inherited: Vec<Map<String, Value>>,
    operation: Vec<Map<String, Value>>,
) -> Vec<Map<String, Value>> {
    let overridden = operation
        .iter()
        .map(|parameter| {
            format!(
                "{}:{}",
                parameter
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                parameter
                    .get("in")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        })
        .collect::<BTreeSet<_>>();
    inherited
        .into_iter()
        .filter(|parameter| {
            !overridden.contains(&format!(
                "{}:{}",
                parameter
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                parameter
                    .get("in")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ))
        })
        .chain(operation)
        .collect()
}

fn resolve_value(document: &Value, value: &Value) -> Value {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return value.clone();
    };
    if !reference.starts_with("#/") {
        return value.clone();
    }
    let mut current = document;
    for component in reference.trim_start_matches("#/").split('/') {
        let key = component.replace("~1", "/").replace("~0", "~");
        let Some(next) = current.get(&key) else {
            return value.clone();
        };
        current = next;
    }
    current.clone()
}

fn endpoint_folder(
    operation: &Map<String, Value>,
    raw_path: &str,
    group_by: GroupBy,
) -> Option<String> {
    let candidate = match group_by {
        GroupBy::Tags => operation
            .get("tags")
            .and_then(Value::as_array)
            .and_then(|tags| tags.first())
            .and_then(Value::as_str),
        GroupBy::Path => raw_path
            .split('/')
            .find(|segment| !segment.is_empty() && !segment.starts_with('{')),
    }?;
    let mut sanitized = sanitize_name(candidate);
    if sanitized.starts_with('.')
        || matches!(
            sanitized.to_ascii_lowercase().as_str(),
            "node_modules" | "environments"
        )
    {
        sanitized = format!("vruno-{sanitized}");
    }
    (!sanitized.is_empty()).then_some(sanitized)
}

fn unique_name(mut name: String, method: &str, used: &mut BTreeSet<String>) -> String {
    if used.insert(name.clone()) {
        return name;
    }
    let base = name.clone();
    name = format!("{base} ({})", method.to_uppercase());
    let mut counter = 2;
    while !used.insert(name.clone()) {
        name = format!("{base} ({counter})");
        counter += 1;
    }
    name
}

fn generate_request(
    document: &Value,
    method: &str,
    path: &str,
    name: &str,
    sequence: u64,
    operation: &Map<String, Value>,
    parameters: &[Map<String, Value>],
) -> String {
    let mut query = Vec::new();
    let mut path_params = Vec::new();
    let mut headers = Vec::new();
    for parameter in parameters {
        let Some(parameter_name) = parameter.get("name").and_then(Value::as_str) else {
            continue;
        };
        let pair = Pair {
            name: parameter_name.into(),
            value: parameter_value(document, parameter),
            enabled: parameter
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || parameter.get("example").is_some()
                || parameter.get("schema").is_some_and(|schema| {
                    ["example", "default", "enum"]
                        .iter()
                        .any(|key| schema.get(*key).is_some())
                }),
        };
        match parameter.get("in").and_then(Value::as_str) {
            Some("query") => query.push(pair),
            Some("path") => path_params.push(Pair {
                enabled: true,
                ..pair
            }),
            Some("header") => headers.push(pair),
            _ => {}
        }
    }
    let body = request_body(document, operation.get("requestBody"));
    if let Some((content_type, _)) = &body
        && !headers
            .iter()
            .any(|header| header.name.eq_ignore_ascii_case("content-type"))
    {
        headers.push(Pair {
            name: "content-type".into(),
            value: content_type.clone(),
            enabled: true,
        });
    }

    let body_mode = body.as_ref().map_or("none", |(_, body)| body.mode());
    let mut output = format!(
        "meta {{\n  name: {}\n  type: http\n  seq: {sequence}\n}}\n\n{method} {{\n  url: {}{}\n  body: {body_mode}\n  auth: inherit\n}}\n",
        sanitize_meta(name),
        "{{baseUrl}}",
        path
    );
    append_pairs_block(&mut output, "params:query", &query);
    append_pairs_block(&mut output, "params:path", &path_params);
    append_pairs_block(&mut output, "headers", &headers);
    if let Some((_, body)) = body {
        output.push('\n');
        output.push_str(&body.render());
    }
    if let Some(docs) = operation.get("description").and_then(Value::as_str)
        && !docs.trim().is_empty()
    {
        output.push_str("\ndocs {\n");
        for line in docs.trim().lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("}\n");
    }
    output
}

enum GeneratedBody {
    Json(String),
    Text(&'static str, String),
    Pairs(&'static str, Vec<Pair>),
}

impl GeneratedBody {
    fn mode(&self) -> &'static str {
        match self {
            Self::Json(_) => "json",
            Self::Text(mode, _) => mode,
            Self::Pairs("form-urlencoded", _) => "formUrlEncoded",
            Self::Pairs("multipart-form", _) => "multipartForm",
            Self::Pairs(mode, _) => mode,
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Json(value) => format!("body:json {{\n{}\n}}\n", indent(value, 2)),
            Self::Text(mode, value) => format!("body:{mode} {{\n{}\n}}\n", indent(value, 2)),
            Self::Pairs(mode, pairs) => {
                let mut output = String::new();
                append_pairs_block(&mut output, &format!("body:{mode}"), pairs);
                output.trim_start_matches('\n').to_owned()
            }
        }
    }
}

fn request_body(document: &Value, request_body: Option<&Value>) -> Option<(String, GeneratedBody)> {
    let request_body = request_body.map(|value| resolve_value(document, value))?;
    let content = request_body.get("content")?.as_object()?;
    let preferred = [
        "application/json",
        "application/x-www-form-urlencoded",
        "multipart/form-data",
        "application/xml",
        "text/xml",
        "text/plain",
    ];
    let (content_type, media) = preferred
        .iter()
        .find_map(|kind| content.get(*kind).map(|media| ((*kind).to_owned(), media)))
        .or_else(|| {
            content
                .iter()
                .next()
                .map(|(kind, media)| (kind.clone(), media))
        })?;
    let schema = media
        .get("schema")
        .map(|value| resolve_value(document, value))
        .unwrap_or(Value::Null);
    let example = media
        .get("example")
        .cloned()
        .unwrap_or_else(|| schema_example(document, &schema, 0));
    let body = match content_type.as_str() {
        value if value.contains("json") => GeneratedBody::Json(
            serde_json::to_string_pretty(&example).unwrap_or_else(|_| "{}".into()),
        ),
        "application/x-www-form-urlencoded" => {
            GeneratedBody::Pairs("form-urlencoded", object_pairs(&example))
        }
        "multipart/form-data" => GeneratedBody::Pairs("multipart-form", object_pairs(&example)),
        value if value.contains("xml") => GeneratedBody::Text("xml", value_as_text(&example)),
        _ => GeneratedBody::Text("text", value_as_text(&example)),
    };
    Some((content_type, body))
}

fn parameter_value(document: &Value, parameter: &Map<String, Value>) -> String {
    if let Some(example) = parameter.get("example") {
        return value_as_text(example);
    }
    let schema = parameter
        .get("schema")
        .map(|value| resolve_value(document, value))
        .unwrap_or(Value::Null);
    schema
        .get("example")
        .or_else(|| schema.get("default"))
        .or_else(|| schema.get("enum").and_then(Value::as_array)?.first())
        .map(value_as_text)
        .unwrap_or_default()
}

fn schema_example(document: &Value, schema: &Value, depth: usize) -> Value {
    if depth > 20 {
        return Value::Null;
    }
    let schema = resolve_value(document, schema);
    if let Some(value) = schema.get("example").or_else(|| schema.get("default")) {
        return value.clone();
    }
    if let Some(value) = schema
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    {
        return value.clone();
    }
    if let Some(value) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
    {
        return value.clone();
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        return Value::Object(
            properties
                .iter()
                .map(|(name, value)| (name.clone(), schema_example(document, value, depth + 1)))
                .collect(),
        );
    }
    if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
        return parts
            .iter()
            .fold(Value::Object(Map::new()), |merged, part| {
                merge_example_objects(merged, schema_example(document, part, depth + 1))
            });
    }
    if let Some(part) = ["oneOf", "anyOf"]
        .into_iter()
        .find_map(|key| schema.get(key).and_then(Value::as_array))
        .and_then(|parts| parts.first())
    {
        return schema_example(document, part, depth + 1);
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("array") => Value::Array(vec![schema_example(
            document,
            schema.get("items").unwrap_or(&Value::Null),
            depth + 1,
        )]),
        Some("boolean") => Value::Bool(false),
        Some("integer") | Some("number") => Value::Number(0.into()),
        Some("object") => Value::Object(Map::new()),
        _ => Value::String(String::new()),
    }
}

fn merge_example_objects(left: Value, right: Value) -> Value {
    match (left, right) {
        (Value::Object(mut left), Value::Object(right)) => {
            left.extend(right);
            Value::Object(left)
        }
        (_, right) => right,
    }
}

fn object_pairs(value: &Value) -> Vec<Pair> {
    value
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .map(|(name, value)| Pair {
            name: name.clone(),
            value: value_as_text(value),
            enabled: true,
        })
        .collect()
}

fn value_as_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn append_pairs_block(output: &mut String, name: &str, pairs: &[Pair]) {
    if pairs.is_empty() {
        return;
    }
    output.push_str(&format!("\n{name} {{\n"));
    for pair in pairs {
        output.push_str("  ");
        if !pair.enabled {
            output.push('~');
        }
        output.push_str(&quote_pair_name(&pair.name));
        output.push_str(": ");
        output.push_str(&pair.value.replace('\n', "\\n"));
        output.push('\n');
    }
    output.push_str("}\n");
}

fn quote_pair_name(name: &str) -> String {
    if name.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '[' | ']')
    }) {
        name.into()
    } else {
        format!("\"{}\"", name.replace('"', "\\\""))
    }
}

fn sanitize_meta(value: &str) -> String {
    value.replace(['\n', '\r'], " ").trim().to_owned()
}

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scan_collection(collection: &Path) -> Result<Vec<ExistingRequest>> {
    let mut requests = Vec::new();
    for entry in WalkDir::new(collection)
        .follow_links(false)
        .into_iter()
        .filter_entry(is_collection_entry)
    {
        let entry = entry.map_err(|error| VutilsError::Message(error.to_string()))?;
        if !is_request_file(&entry) {
            continue;
        }
        let content = fs::read_to_string(entry.path()).map_err(|source| VutilsError::Read {
            path: entry.path().to_path_buf(),
            source,
        })?;
        let Some((method, path, signature)) = analyze_request(&content) else {
            continue;
        };
        requests.push(ExistingRequest {
            file: entry.path().to_path_buf(),
            relative_file: entry
                .path()
                .strip_prefix(collection)
                .unwrap_or(entry.path())
                .to_path_buf(),
            content,
            method,
            path,
            signature,
        });
    }
    requests.sort_by(|left, right| left.relative_file.cmp(&right.relative_file));
    Ok(requests)
}

fn reject_duplicate_requests(requests: &[ExistingRequest]) -> Result<()> {
    let mut seen = BTreeMap::<String, &Path>::new();
    for request in requests {
        let id = request.id();
        if let Some(previous) = seen.insert(id.clone(), &request.relative_file) {
            return Err(VutilsError::InvalidInput(format!(
                "Bruno collection has duplicate endpoint `{id}` in `{}` and `{}`; Vruno will not guess which request to update",
                previous.display(),
                request.relative_file.display()
            )));
        }
    }
    Ok(())
}

fn is_collection_entry(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() || entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !name.starts_with('.') && !matches!(name.as_ref(), "node_modules" | "environments")
}

fn is_request_file(entry: &DirEntry) -> bool {
    if !entry.file_type().is_file()
        || entry.path().extension().and_then(|value| value.to_str()) != Some("bru")
    {
        return false;
    }
    !matches!(
        entry.file_name().to_string_lossy().as_ref(),
        "collection.bru" | "folder.bru"
    )
}

fn analyze_request(content: &str) -> Option<(String, String, RequestSignature)> {
    let blocks = top_level_blocks(content);
    let method = blocks
        .iter()
        .find(|block| HTTP_METHODS.contains(&block.name))?;
    let url = block_field(method.source, "url")?;
    Some((
        method.name.to_uppercase(),
        normalize_url_path(&url),
        signature_from_blocks(&blocks, method),
    ))
}

fn signature_from_content(content: &str) -> Option<RequestSignature> {
    let blocks = top_level_blocks(content);
    let method = blocks
        .iter()
        .find(|block| HTTP_METHODS.contains(&block.name))?;
    Some(signature_from_blocks(&blocks, method))
}

fn signature_from_blocks(blocks: &[Block<'_>], method: &Block<'_>) -> RequestSignature {
    let mut params = BTreeSet::new();
    for (block_name, location) in [("params:query", "query"), ("params:path", "path")] {
        if let Some(block) = blocks.iter().find(|block| block.name == block_name) {
            params.extend(
                parse_pairs(block.source)
                    .into_iter()
                    .map(|pair| format!("{location}:{}", pair.name.to_ascii_lowercase())),
            );
        }
    }
    let headers = blocks
        .iter()
        .find(|block| block.name == "headers")
        .into_iter()
        .flat_map(|block| parse_pairs(block.source))
        .map(|pair| pair.name.to_ascii_lowercase())
        .collect();
    let body_mode = block_field(method.source, "body").unwrap_or_else(|| "none".into());
    let body_shape = if body_mode == "json" {
        blocks
            .iter()
            .find(|block| block.name == "body:json")
            .and_then(|block| json_body(block.source))
            .and_then(|body| parse_json_with_vars(&body).ok())
            .map(|value| json_shape(&value))
            .unwrap_or_default()
    } else if matches!(body_mode.as_str(), "formUrlEncoded" | "multipartForm") {
        let block_name = match body_mode.as_str() {
            "formUrlEncoded" => "body:form-urlencoded",
            _ => "body:multipart-form",
        };
        blocks
            .iter()
            .find(|block| block.name == block_name)
            .into_iter()
            .flat_map(|block| parse_pairs(block.source))
            .map(|pair| pair.name.to_ascii_lowercase())
            .collect()
    } else {
        BTreeSet::new()
    };
    RequestSignature {
        params,
        headers,
        body_mode,
        body_shape,
    }
}

fn top_level_blocks(content: &str) -> Vec<Block<'_>> {
    let mut blocks = Vec::new();
    let mut offset = 0;
    let mut start: Option<(usize, &str)> = None;
    for line in content.split_inclusive('\n') {
        let clean = line.trim_end_matches(['\r', '\n']);
        if start.is_none() && !clean.starts_with(char::is_whitespace) && clean.ends_with(" {") {
            let name = clean.trim_end_matches(" {");
            start = Some((offset, name));
        } else if clean == "}"
            && let Some((block_start, name)) = start.take()
        {
            let end = offset + line.len();
            blocks.push(Block {
                name,
                source: &content[block_start..end],
            });
        }
        offset += line.len();
    }
    blocks
}

fn block_field(block: &str, field: &str) -> Option<String> {
    block.lines().skip(1).find_map(|line| {
        let line = line.trim();
        let (name, value) = line.split_once(':')?;
        (name.trim() == field).then(|| value.trim().to_owned())
    })
}

fn parse_pairs(block: &str) -> Vec<Pair> {
    block
        .lines()
        .skip(1)
        .take_while(|line| line.trim() != "}")
        .filter_map(parse_pair)
        .collect()
}

fn parse_pair(line: &str) -> Option<Pair> {
    let mut line = line.trim();
    let enabled = !line.starts_with('~');
    if !enabled {
        line = line.trim_start_matches('~');
    }
    let separator = if line.starts_with('"') {
        let mut escaped = false;
        let mut closing = None;
        for (index, character) in line.char_indices().skip(1) {
            if character == '"' && !escaped {
                closing = Some(index);
                break;
            }
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
        }
        closing.and_then(|index| line[index + 1..].find(':').map(|tail| index + 1 + tail))?
    } else {
        line.find(':')?
    };
    let name = line[..separator]
        .trim()
        .trim_matches('"')
        .replace("\\\"", "\"");
    Some(Pair {
        name,
        value: line[separator + 1..].trim().to_owned(),
        enabled,
    })
}

fn json_body(block: &str) -> Option<String> {
    let start = block.find('{')? + 1;
    let end = block.rfind('}')?;
    Some(block[start..end].trim().to_owned())
}

fn json_shape(value: &Value) -> BTreeSet<String> {
    fn visit(value: &Value, prefix: &str, output: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    output.insert(path.clone());
                    visit(value, &path, output);
                }
            }
            Value::Array(values) => {
                output.insert(format!("{prefix}[]"));
                if let Some(value) = values.first() {
                    visit(value, &format!("{prefix}[]"), output);
                }
            }
            _ => {}
        }
    }
    let mut output = BTreeSet::new();
    visit(value, "", &mut output);
    output
}

fn compare(endpoints: &[Endpoint], existing: &[ExistingRequest]) -> DriftReport {
    let spec = endpoints
        .iter()
        .map(|endpoint| (endpoint.id(), endpoint))
        .collect::<BTreeMap<_, _>>();
    let collection = existing
        .iter()
        .map(|request| (request.id(), request))
        .collect::<BTreeMap<_, _>>();
    let mut missing = Vec::new();
    let mut modified = Vec::new();
    let mut stale = Vec::new();

    for (id, endpoint) in &spec {
        let Some(request) = collection.get(id) else {
            missing.push(item_for_endpoint(endpoint));
            continue;
        };
        let changes = signature_changes(&request.signature, &endpoint.signature);
        if !changes.is_empty() {
            modified.push(DriftItem {
                method: endpoint.method.clone(),
                path: endpoint.path.clone(),
                name: endpoint.name.clone(),
                file: Some(request.relative_file.display().to_string()),
                changes,
            });
        }
    }
    for (id, request) in &collection {
        if !spec.contains_key(id) {
            stale.push(DriftItem {
                method: request.method.clone(),
                path: request.path.clone(),
                name: request
                    .relative_file
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("request")
                    .into(),
                file: Some(request.relative_file.display().to_string()),
                changes: Vec::new(),
            });
        }
    }
    DriftReport {
        in_sync: missing.is_empty() && modified.is_empty() && stale.is_empty(),
        missing,
        modified,
        stale,
    }
}

fn item_for_endpoint(endpoint: &Endpoint) -> DriftItem {
    DriftItem {
        method: endpoint.method.clone(),
        path: endpoint.path.clone(),
        name: endpoint.name.clone(),
        file: None,
        changes: Vec::new(),
    }
}

fn signature_changes(actual: &RequestSignature, expected: &RequestSignature) -> Vec<String> {
    let mut changes = Vec::new();
    if actual.params != expected.params {
        changes.push("parameters".into());
    }
    if actual.headers != expected.headers {
        changes.push("headers".into());
    }
    if actual.body_mode != expected.body_mode {
        changes.push(format!(
            "body: {} -> {}",
            actual.body_mode, expected.body_mode
        ));
    } else if actual.body_shape != expected.body_shape {
        changes.push("body schema".into());
    }
    changes
}

fn apply(
    collection: &Path,
    endpoints: &[Endpoint],
    existing: &[ExistingRequest],
    report: &DriftReport,
) -> Result<()> {
    let by_id = endpoints
        .iter()
        .map(|endpoint| (endpoint.id(), endpoint))
        .collect::<HashMap<_, _>>();
    let existing_by_id = existing
        .iter()
        .map(|request| (request.id(), request))
        .collect::<HashMap<_, _>>();

    for item in &report.modified {
        let id = format!("{}:{}", item.method, item.path);
        let endpoint = by_id[&id];
        let request = existing_by_id[&id];
        let merged = merge_request(&request.content, &endpoint.content);
        atomic_write(&request.file, merged.as_bytes())?;
    }
    for item in &report.missing {
        let id = format!("{}:{}", item.method, item.path);
        let endpoint = by_id[&id];
        let directory = endpoint.folder.as_ref().map_or_else(
            || collection.to_path_buf(),
            |folder| collection.join(folder),
        );
        ensure_folder(&directory, endpoint.folder.as_deref())?;
        let file = unique_file(&directory, &sanitize_name(&endpoint.name), &endpoint.method);
        atomic_write(&file, endpoint.content.as_bytes())?;
    }
    Ok(())
}

fn merge_request(existing: &str, generated: &str) -> String {
    let generated_blocks = top_level_blocks(generated);
    let existing_blocks = top_level_blocks(existing);
    let generated_method = generated_blocks
        .iter()
        .find(|block| HTTP_METHODS.contains(&block.name));
    let existing_method = existing_blocks
        .iter()
        .find(|block| HTTP_METHODS.contains(&block.name));
    let Some(generated_method) = generated_method else {
        return existing.to_owned();
    };
    let Some(existing_method) = existing_method else {
        return generated.to_owned();
    };

    let managed = |name: &str| {
        HTTP_METHODS.contains(&name)
            || matches!(name, "params:query" | "params:path" | "headers")
            || name.starts_with("body:")
    };
    let mut desired = BTreeMap::<String, String>::new();
    for block in &generated_blocks {
        if managed(block.name) {
            desired.insert(block.name.to_owned(), block.source.to_owned());
        }
    }
    let auth = block_field(existing_method.source, "auth").unwrap_or_else(|| "inherit".into());
    desired.insert(
        generated_method.name.to_owned(),
        replace_block_field(generated_method.source, "auth", &auth),
    );

    for name in [
        "params:query",
        "params:path",
        "headers",
        "body:form-urlencoded",
        "body:multipart-form",
    ] {
        let Some(spec_block) = desired.get(name).cloned() else {
            continue;
        };
        if let Some(user_block) = existing_blocks.iter().find(|block| block.name == name) {
            let merged = merge_pairs(&parse_pairs(&spec_block), &parse_pairs(user_block.source));
            let mut rendered = String::new();
            append_pairs_block(&mut rendered, name, &merged);
            desired.insert(name.into(), rendered.trim_start_matches('\n').to_owned());
        }
    }
    if let (Some(spec), Some(user)) = (
        desired.get("body:json").cloned(),
        existing_blocks
            .iter()
            .find(|block| block.name == "body:json"),
    ) {
        desired.insert("body:json".into(), merge_json_block(user.source, &spec));
    }

    let mut output = existing.to_owned();
    for block in existing_blocks.iter().rev() {
        if !managed(block.name) {
            continue;
        }
        let replacement = desired.remove(block.name).unwrap_or_default();
        if let Some(start) = output.find(block.source) {
            output.replace_range(start..start + block.source.len(), &replacement);
        }
    }
    if !desired.is_empty() {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        for block in generated_blocks {
            if let Some(source) = desired.remove(block.name) {
                output.push('\n');
                output.push_str(&source);
            }
        }
    }
    normalize_spacing(output)
}

fn replace_block_field(block: &str, field: &str, value: &str) -> String {
    block
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed
                .split_once(':')
                .is_some_and(|(name, _)| name.trim() == field)
            {
                format!("  {field}: {value}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn merge_pairs(spec: &[Pair], user: &[Pair]) -> Vec<Pair> {
    let mut cursors = HashMap::<&str, usize>::new();
    spec.iter()
        .map(|item| {
            let candidates = user
                .iter()
                .filter(|candidate| candidate.name.eq_ignore_ascii_case(&item.name))
                .collect::<Vec<_>>();
            let cursor = cursors.entry(item.name.as_str()).or_default();
            let result = candidates.get(*cursor).map_or_else(
                || item.clone(),
                |candidate| Pair {
                    name: item.name.clone(),
                    value: candidate.value.clone(),
                    enabled: candidate.enabled,
                },
            );
            *cursor += 1;
            result
        })
        .collect()
}

fn merge_json_block(user: &str, spec: &str) -> String {
    let Some(user_body) = json_body(user) else {
        return spec.to_owned();
    };
    let Some(spec_body) = json_body(spec) else {
        return spec.to_owned();
    };
    let (user_masked, user_vars) = mask_json_vars(&user_body, "USER");
    let (spec_masked, spec_vars) = mask_json_vars(&spec_body, "SPEC");
    let (Ok(user_value), Ok(spec_value)) = (
        serde_json::from_str::<Value>(&user_masked),
        serde_json::from_str::<Value>(&spec_masked),
    ) else {
        return user.to_owned();
    };
    let merged = merge_json_value(&user_value, &spec_value);
    let value = serde_json::to_string_pretty(&merged).unwrap_or(spec_body);
    let value = unmask_json_vars(&unmask_json_vars(&value, &user_vars), &spec_vars);
    format!("body:json {{\n{}\n}}\n", indent(&value, 2))
}

fn parse_json_with_vars(value: &str) -> std::result::Result<Value, serde_json::Error> {
    let (masked, _) = mask_json_vars(value, "SHAPE");
    serde_json::from_str(&masked)
}

fn mask_json_vars(value: &str, prefix: &str) -> (String, Vec<(String, String, bool)>) {
    let mut output = String::new();
    let mut variables = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            let mut backslashes = 0;
            let mut cursor = index;
            while cursor > 0 && bytes[cursor - 1] == b'\\' {
                backslashes += 1;
                cursor -= 1;
            }
            if backslashes % 2 == 0 {
                in_string = !in_string;
            }
            output.push('"');
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"{{")
            && let Some(relative_end) = value[index + 2..].find("}}")
        {
            let end = index + 2 + relative_end + 2;
            let token = value[index..end].to_owned();
            let marker = format!("\u{e000}{prefix}_{}\u{e001}", variables.len());
            if in_string {
                output.push_str(&marker);
            } else {
                output.push('"');
                output.push_str(&marker);
                output.push('"');
            }
            variables.push((marker, token, in_string));
            index = end;
            continue;
        }
        let character = value[index..].chars().next().expect("valid UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }
    (output, variables)
}

fn unmask_json_vars(value: &str, variables: &[(String, String, bool)]) -> String {
    variables
        .iter()
        .fold(value.to_owned(), |output, (marker, token, in_string)| {
            if *in_string {
                output.replace(marker, token)
            } else {
                output.replace(&format!("\"{marker}\""), token)
            }
        })
}

fn merge_json_value(user: &Value, spec: &Value) -> Value {
    match (user, spec) {
        (Value::Object(user), Value::Object(spec)) => Value::Object(
            spec.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        user.get(key)
                            .map_or_else(|| value.clone(), |user| merge_json_value(user, value)),
                    )
                })
                .collect(),
        ),
        (Value::Array(user), Value::Array(spec)) if !user.is_empty() && !spec.is_empty() => {
            Value::Array(
                user.iter()
                    .map(|value| merge_json_value(value, &spec[0]))
                    .collect(),
            )
        }
        (Value::Array(user), Value::Array(spec)) if user.is_empty() => Value::Array(spec.clone()),
        _ => user.clone(),
    }
}

fn normalize_spacing(mut content: String) -> String {
    while content.contains("\n\n\n") {
        content = content.replace("\n\n\n", "\n\n");
    }
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

fn ensure_folder(directory: &Path, name: Option<&str>) -> Result<()> {
    if directory.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(directory).map_err(|source| VutilsError::Write {
        path: directory.to_path_buf(),
        source,
    })?;
    if let Some(name) = name {
        atomic_write(
            &directory.join("folder.bru"),
            format!("meta {{\n  name: {}\n  seq: 1\n}}\n", sanitize_meta(name)).as_bytes(),
        )?;
    }
    Ok(())
}

fn unique_file(directory: &Path, name: &str, method: &str) -> PathBuf {
    let base = if name.is_empty() {
        method.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    let mut candidate = directory.join(format!("{base}.bru"));
    if !candidate.exists() {
        return candidate;
    }
    candidate = directory.join(format!("{base} ({}).bru", method.to_uppercase()));
    let mut counter = 2;
    while candidate.exists() {
        candidate = directory.join(format!("{base} ({counter}).bru"));
        counter += 1;
    }
    candidate
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent).map_err(|source| VutilsError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temporary
        .write_all(bytes)
        .map_err(|source| VutilsError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|source| VutilsError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    set_output_permissions(path, temporary.path())?;
    temporary
        .persist(path)
        .map_err(|error| VutilsError::Write {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    Ok(())
}

#[cfg(unix)]
fn set_output_permissions(destination: &Path, temporary: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = fs::metadata(destination)
        .map(|metadata| metadata.permissions().mode())
        .unwrap_or(0o644);
    fs::set_permissions(temporary, fs::Permissions::from_mode(mode)).map_err(|source| {
        VutilsError::Write {
            path: destination.to_path_buf(),
            source,
        }
    })
}

#[cfg(not(unix))]
fn set_output_permissions(destination: &Path, temporary: &Path) -> Result<()> {
    if let Ok(metadata) = fs::metadata(destination) {
        fs::set_permissions(temporary, metadata.permissions()).map_err(|source| {
            VutilsError::Write {
                path: destination.to_path_buf(),
                source,
            }
        })?;
    }
    Ok(())
}

fn normalize_spec_path(path: &str) -> String {
    let path = path.split('?').next().unwrap_or(path).trim_end_matches('/');
    let mut output = String::new();
    let mut chars = path.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '{' {
            let name = chars
                .by_ref()
                .take_while(|value| *value != '}')
                .collect::<String>();
            output.push(':');
            output.push_str(&name);
        } else {
            output.push(character);
        }
    }
    if output.is_empty() {
        "/".into()
    } else {
        output
    }
}

fn normalize_url_path(url: &str) -> String {
    let mut value = url.trim().split('?').next().unwrap_or(url).to_owned();
    if let Some(rest) = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
    {
        value = rest
            .find('/')
            .map_or_else(|| "/".into(), |index| rest[index..].into());
    }
    while let Some(start) = value.find("{{") {
        let Some(relative_end) = value[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + relative_end;
        let token = &value[start + 2..end];
        let replacement = if start == 0 || token.to_ascii_lowercase().contains("baseurl") {
            String::new()
        } else {
            format!(":{}", token.rsplit('_').next().unwrap_or(token))
        };
        value.replace_range(start..end + 2, &replacement);
    }
    value = normalize_spec_path(&value);
    while value.contains("//") {
        value = value.replace("//", "/");
    }
    if !value.starts_with('/') {
        value.insert(0, '/');
    }
    if value.len() > 1 {
        value = value.trim_end_matches('/').into();
    }
    value
}

fn sanitize_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    sanitized.trim().trim_matches('.').to_owned()
}

fn render_text(report: &DriftReport, mode: SyncMode) -> String {
    let mut output = if report.in_sync {
        "Vruno: collection is in sync.\n".to_owned()
    } else {
        format!(
            "Vruno: {} missing, {} modified, {} stale.\n",
            report.missing.len(),
            report.modified.len(),
            report.stale.len()
        )
    };
    for (label, items) in [
        ("CREATE", &report.missing),
        ("UPDATE", &report.modified),
        ("KEEP STALE", &report.stale),
    ] {
        for item in items {
            output.push_str(&format!("{label:10} {} {}", item.method, item.path));
            if !item.changes.is_empty() {
                output.push_str(&format!(" ({})", item.changes.join(", ")));
            }
            if let Some(file) = &item.file {
                output.push_str(&format!(" [{file}]"));
            }
            output.push('\n');
        }
    }
    if mode == SyncMode::Preview && !report.in_sync {
        output.push_str("Preview only: no collection files were changed.\n");
    } else if mode == SyncMode::Sync {
        output.push_str(&format!(
            "Applied: {} created, {} updated; {} stale kept.\n",
            report.missing.len(),
            report.modified.len(),
            report.stale.len()
        ));
    }
    output
}

fn canonicalize(path: &Path, label: &str) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        VutilsError::InvalidInput(format!(
            "{label} `{}` cannot be resolved: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read as _, net::TcpListener, thread};

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let collection = directory.path().join("collection");
        fs::create_dir(&collection).unwrap();
        fs::write(
            collection.join("bruno.json"),
            r#"{"version":"1","name":"API","type":"collection"}"#,
        )
        .unwrap();
        let openapi = directory.path().join("openapi.yaml");
        fs::write(
            &openapi,
            r#"openapi: 3.1.0
info:
  title: API
servers:
  - url: https://api.example.test/v1
paths:
  /users/{id}:
    get:
      summary: Get user
      tags: [Users]
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
        - { name: verbose, in: query, schema: { type: boolean, default: false } }
      responses: { '200': { description: OK } }
  /users:
    post:
      summary: Create user
      tags: [Users]
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                name: { type: string }
                active: { type: boolean }
      responses: { '201': { description: Created } }
"#,
        )
        .unwrap();
        (directory, collection, openapi)
    }

    fn request(collection: &Path, openapi: &Path, mode: SyncMode) -> SyncRequest {
        SyncRequest {
            collection: collection.into(),
            openapi: openapi.into(),
            mode,
            json: false,
            group_by: GroupBy::Tags,
        }
    }

    #[test]
    fn validates_openapi_from_http_url() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let document = b"openapi: 3.1.0\ninfo: { title: Remote API }\npaths: {}\n";
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let received = stream.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..received]).starts_with("GET /openapi.yaml ")
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                document.len()
            )
            .unwrap();
            stream.write_all(document).unwrap();
        });
        let url = format!("http://{address}/openapi.yaml");

        assert_eq!(
            validate_openapi(Path::new(&url)).unwrap(),
            PathBuf::from(&url)
        );
        server.join().unwrap();
    }

    #[test]
    fn accepts_collection_directory_bruno_json_and_file_url() {
        let (_directory, collection, _openapi) = fixture();
        let expected = fs::canonicalize(&collection).unwrap();
        let config = collection.join("bruno.json");
        let file_url = url::Url::from_directory_path(&collection).unwrap();

        assert_eq!(validate_collection(&collection).unwrap(), expected);
        assert_eq!(validate_collection(&config).unwrap(), expected);
        assert_eq!(
            validate_collection(Path::new(file_url.as_str())).unwrap(),
            expected
        );
    }

    #[test]
    fn rejects_remote_collection_url_with_actionable_guidance() {
        let error = validate_collection(Path::new("https://example.com/bruno.json"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("not a writable local collection"));
    }

    #[test]
    fn native_sync_creates_requests_and_then_reports_clean() {
        let (_directory, collection, openapi) = fixture();
        let check = run(&request(&collection, &openapi, SyncMode::Check)).unwrap();
        assert!(!check.success);
        assert!(
            String::from_utf8(check.stdout)
                .unwrap()
                .contains("2 missing")
        );

        run(&request(&collection, &openapi, SyncMode::Sync)).unwrap();
        assert!(collection.join("Users/Get user.bru").is_file());
        assert!(collection.join("Users/Create user.bru").is_file());

        let clean = run(&request(&collection, &openapi, SyncMode::Check)).unwrap();
        assert!(
            clean.success,
            "{}",
            String::from_utf8(clean.stdout).unwrap()
        );
    }

    #[test]
    fn merge_preserves_values_and_local_blocks_but_updates_schema() {
        let (_directory, collection, openapi) = fixture();
        run(&request(&collection, &openapi, SyncMode::Sync)).unwrap();
        let file = collection.join("Users/Create user.bru");
        let original = fs::read_to_string(&file).unwrap();
        let edited = original.replace("\"name\": \"\"", "\"name\": {{userName}}")
            + "\ntests {\n  test(\"local\", function() {\n    expect(res.status).to.equal(201);\n  });\n}\n";
        fs::write(&file, edited).unwrap();
        let mut document = fs::read_to_string(&openapi).unwrap();
        document = document.replace("active: { type: boolean }", "email: { type: string }");
        fs::write(&openapi, document).unwrap();

        run(&request(&collection, &openapi, SyncMode::Sync)).unwrap();
        let merged = fs::read_to_string(file).unwrap();
        assert!(merged.contains("\"name\": {{userName}}"));
        assert!(merged.contains("\"email\": \"\""));
        assert!(!merged.contains("\"active\""));
        assert!(merged.contains("test(\"local\""));
    }

    #[test]
    fn stale_requests_are_reported_and_never_deleted() {
        let (_directory, collection, openapi) = fixture();
        let stale = collection.join("old.bru");
        fs::write(
            &stale,
            "meta {\n  name: old\n  type: http\n  seq: 1\n}\n\nget {\n  url: {{baseUrl}}/old\n  body: none\n  auth: inherit\n}\n",
        )
        .unwrap();
        let preview = run(&request(&collection, &openapi, SyncMode::Preview)).unwrap();
        assert!(
            String::from_utf8(preview.stdout)
                .unwrap()
                .contains("KEEP STALE")
        );
        run(&request(&collection, &openapi, SyncMode::Sync)).unwrap();
        assert!(stale.is_file());
    }

    #[test]
    fn rejects_open_collection_yaml_without_modifying_it() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("opencollection.yml"), "version: 1\n").unwrap();
        assert!(matches!(
            validate_collection(directory.path()),
            Err(VutilsError::Unsupported(_))
        ));
    }

    #[test]
    fn rejects_external_references_with_actionable_guidance() {
        let directory = tempfile::tempdir().unwrap();
        let openapi = directory.path().join("openapi.yaml");
        fs::write(
            &openapi,
            "openapi: 3.1.0\npaths: {}\ncomponents:\n  schemas:\n    User:\n      $ref: schemas.yaml#/User\n",
        )
        .unwrap();

        let error = validate_openapi(&openapi).unwrap_err().to_string();
        assert!(error.contains("bundle the specification"));
    }

    #[test]
    fn refuses_ambiguous_duplicate_collection_endpoints() {
        let (_directory, collection, openapi) = fixture();
        let duplicate = "meta {\n  name: duplicate\n  type: http\n  seq: 1\n}\n\nget {\n  url: {{baseUrl}}/users/:id\n  body: none\n  auth: inherit\n}\n";
        fs::write(collection.join("first.bru"), duplicate).unwrap();
        fs::write(collection.join("second.bru"), duplicate).unwrap();

        let error = run(&request(&collection, &openapi, SyncMode::Check))
            .unwrap_err()
            .to_string();
        assert!(error.contains("duplicate endpoint"));
        assert!(error.contains("first.bru"));
        assert!(error.contains("second.bru"));
    }

    #[test]
    fn form_body_uses_bruno_request_mode_and_block_names() {
        let (_directory, collection, openapi) = fixture();
        fs::write(
            &openapi,
            "openapi: 3.1.0\ninfo: { title: API }\npaths:\n  /login:\n    post:\n      summary: Login\n      requestBody:\n        content:\n          application/x-www-form-urlencoded:\n            schema:\n              type: object\n              properties:\n                username: { type: string }\n      responses: { '200': { description: OK } }\n",
        )
        .unwrap();

        run(&request(&collection, &openapi, SyncMode::Sync)).unwrap();
        let generated = fs::read_to_string(collection.join("Login.bru")).unwrap();
        assert!(generated.contains("body: formUrlEncoded"));
        assert!(generated.contains("body:form-urlencoded {"));
        assert!(
            run(&request(&collection, &openapi, SyncMode::Check))
                .unwrap()
                .success
        );
    }
}
