use std::collections::BTreeMap;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpRequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<HttpBody>,
    #[serde(default)]
    pub follow_redirects: bool,
    #[serde(default)]
    pub compressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HttpBody {
    Text(String),
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    File { path: String, binary: bool },
}

#[derive(Debug, Clone, Copy)]
pub enum HttpRenderer {
    Curl,
    Httpie,
    Fetch,
    Axios,
    Json,
}

#[derive(Debug, Clone, Copy)]
pub enum Shell {
    Posix,
    PowerShell,
}

impl HttpRequestSpec {
    pub fn new(method: &str, url: &str) -> Result<Self> {
        validate_http_url(url)?;
        let method = method.trim().to_ascii_uppercase();
        if method.is_empty()
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte == b'-')
        {
            return Err(VutilsError::InvalidInput("invalid HTTP method".into()));
        }
        Ok(Self {
            method,
            url: url.to_owned(),
            headers: Vec::new(),
            body: None,
            follow_redirects: false,
            compressed: false,
        })
    }
}

pub fn render(request: &HttpRequestSpec, renderer: HttpRenderer, shell: Shell) -> Result<String> {
    validate_request(request)?;
    match renderer {
        HttpRenderer::Curl => render_curl(request, shell),
        HttpRenderer::Httpie => render_httpie(request, shell),
        HttpRenderer::Fetch => render_fetch(request),
        HttpRenderer::Axios => render_axios(request),
        HttpRenderer::Json => serde_json::to_string_pretty(request)
            .map_err(|error| VutilsError::Message(error.to_string())),
    }
}

pub fn parse_curl(command: &str) -> Result<HttpRequestSpec> {
    reject_shell_syntax(command)?;
    let arguments = shell_words::split(command)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid POSIX quoting: {error}")))?;
    if arguments.first().map(String::as_str) != Some("curl") {
        return Err(VutilsError::InvalidInput(
            "command must start with `curl`".into(),
        ));
    }
    let mut request = HttpRequestSpec {
        method: "GET".into(),
        url: String::new(),
        headers: Vec::new(),
        body: None,
        follow_redirects: false,
        compressed: false,
    };
    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.as_str() {
            "-X" | "--request" => {
                request.method =
                    required_argument(&arguments, &mut index, argument)?.to_ascii_uppercase();
            }
            "-H" | "--header" => {
                let header = required_argument(&arguments, &mut index, argument)?;
                let (name, value) = header.split_once(':').ok_or_else(|| {
                    VutilsError::InvalidInput(format!("header `{header}` must contain a colon"))
                })?;
                request
                    .headers
                    .push((name.trim().to_owned(), value.trim().to_owned()));
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                let data = required_argument(&arguments, &mut index, argument)?;
                let file_path = if argument == "--data-raw" {
                    None
                } else {
                    data.strip_prefix('@')
                };
                request.body = Some(if let Some(path) = file_path {
                    HttpBody::File {
                        path: path.to_owned(),
                        binary: argument == "--data-binary",
                    }
                } else {
                    HttpBody::Text(data.to_owned())
                });
                if request.method == "GET" {
                    request.method = "POST".into();
                }
            }
            "-F" | "--form" => {
                let form = required_argument(&arguments, &mut index, argument)?;
                let (name, value) = form.split_once('=').ok_or_else(|| {
                    VutilsError::InvalidInput(format!("form value `{form}` must contain `=`"))
                })?;
                match &mut request.body {
                    Some(HttpBody::Form(values)) => values.push((name.into(), value.into())),
                    None => request.body = Some(HttpBody::Form(vec![(name.into(), value.into())])),
                    _ => {
                        return Err(VutilsError::InvalidInput(
                            "cannot mix form and data bodies".into(),
                        ));
                    }
                }
                if request.method == "GET" {
                    request.method = "POST".into();
                }
            }
            "-u" | "--user" => {
                let credentials = required_argument(&arguments, &mut index, argument)?;
                if !credentials.contains(':') {
                    return Err(VutilsError::Unsupported(
                        "interactive cURL password prompts are not supported; provide user:password"
                            .into(),
                    ));
                }
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                request
                    .headers
                    .push(("Authorization".into(), format!("Basic {encoded}")));
            }
            "-b" | "--cookie" => {
                let cookie = required_argument(&arguments, &mut index, argument)?;
                if !cookie.contains('=') {
                    return Err(VutilsError::Unsupported(
                        "cURL cookie files are not supported; provide a name=value cookie".into(),
                    ));
                }
                request.headers.push(("Cookie".into(), cookie.to_owned()));
            }
            "-L" | "--location" => request.follow_redirects = true,
            "--compressed" => request.compressed = true,
            "--url" => {
                request.url = required_argument(&arguments, &mut index, argument)?.to_owned()
            }
            value if value.starts_with("-X") && value.len() > 2 => {
                request.method = value[2..].to_ascii_uppercase();
            }
            value if value.starts_with('-') => {
                return Err(VutilsError::Unsupported(format!(
                    "cURL option `{value}` is not supported and was not ignored"
                )));
            }
            value if request.url.is_empty() => request.url = value.to_owned(),
            value => {
                return Err(VutilsError::InvalidInput(format!(
                    "unexpected cURL argument `{value}`"
                )));
            }
        }
        index += 1;
    }
    promote_json_body(&mut request);
    validate_request(&request)?;
    Ok(request)
}

fn promote_json_body(request: &mut HttpRequestSpec) {
    let is_json = request.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type")
            && value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
    });
    if is_json {
        let parsed = match &request.body {
            Some(HttpBody::Text(value)) => serde_json::from_str(value).ok(),
            _ => None,
        };
        if let Some(json) = parsed {
            request.body = Some(HttpBody::Json(json));
        }
    }
}

pub fn format_curl(command: &str, shell: Shell) -> Result<String> {
    render_curl(&parse_curl(command)?, shell)
}

pub fn explain_curl(command: &str, show_secrets: bool) -> Result<String> {
    let request = parse_curl(command)?;
    let headers: Vec<_> = request.headers.iter().map(|(name, value)| {
        let sensitive = matches!(name.to_ascii_lowercase().as_str(), "authorization" | "cookie" | "proxy-authorization");
        serde_json::json!({"name": name, "value": if sensitive && !show_secrets { "<redacted>" } else { value }})
    }).collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "method": request.method,
        "url": request.url,
        "headers": headers,
        "body": request.body,
        "follow_redirects": request.follow_redirects,
        "compressed": request.compressed
    }))
    .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn request_from_har(input: &str, entry: Option<usize>) -> Result<HttpRequestSpec> {
    let value: serde_json::Value = serde_json::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid HAR JSON: {error}")))?;
    let entries = value
        .pointer("/log/entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| VutilsError::InvalidInput("HAR must contain log.entries".into()))?;
    let selected = match (entries.len(), entry) {
        (1, None) => &entries[0],
        (_, Some(index)) => entries.get(index).ok_or_else(|| {
            VutilsError::InvalidInput(format!("HAR entry index {index} is out of range"))
        })?,
        (_, None) => {
            return Err(VutilsError::InvalidInput(format!(
                "HAR contains {} entries; select one with --entry",
                entries.len()
            )));
        }
    };
    let request = selected
        .get("request")
        .ok_or_else(|| VutilsError::InvalidInput("HAR entry has no request".into()))?;
    let method = request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("GET");
    let url = request
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| VutilsError::InvalidInput("HAR request has no URL".into()))?;
    let mut spec = HttpRequestSpec::new(method, url)?;
    if let Some(headers) = request.get("headers").and_then(serde_json::Value::as_array) {
        for header in headers {
            if let (Some(name), Some(value)) = (
                header.get("name").and_then(serde_json::Value::as_str),
                header.get("value").and_then(serde_json::Value::as_str),
            ) {
                spec.headers.push((name.into(), value.into()));
            }
        }
    }
    if let Some(post_data) = request.get("postData")
        && let Some(text) = post_data.get("text").and_then(serde_json::Value::as_str)
    {
        spec.body = Some(
            if post_data
                .get("mimeType")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|mime| mime.contains("json"))
            {
                HttpBody::Json(
                    serde_json::from_str(text)
                        .unwrap_or_else(|_| serde_json::Value::String(text.into())),
                )
            } else {
                HttpBody::Text(text.into())
            },
        );
    }
    Ok(spec)
}

pub fn inspect_url(input: &str) -> Result<String> {
    let url = Url::parse(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid URL: {error}")))?;
    let query: BTreeMap<_, Vec<_>> =
        url.query_pairs()
            .fold(BTreeMap::new(), |mut values, (key, value)| {
                values
                    .entry(key.into_owned())
                    .or_default()
                    .push(value.into_owned());
                values
            });
    serde_json::to_string_pretty(&serde_json::json!({
        "scheme": url.scheme(),
        "username": url.username(),
        "has_password": url.password().is_some(),
        "host": url.host_str(),
        "port": url.port_or_known_default(),
        "path": url.path(),
        "query": query,
        "fragment": url.fragment()
    }))
    .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn http_status(code: u16) -> Result<&'static str> {
    match code {
        100 => Ok("Continue"),
        101 => Ok("Switching Protocols"),
        200 => Ok("OK"),
        201 => Ok("Created"),
        202 => Ok("Accepted"),
        204 => Ok("No Content"),
        206 => Ok("Partial Content"),
        300 => Ok("Multiple Choices"),
        301 => Ok("Moved Permanently"),
        302 => Ok("Found"),
        304 => Ok("Not Modified"),
        307 => Ok("Temporary Redirect"),
        308 => Ok("Permanent Redirect"),
        400 => Ok("Bad Request"),
        401 => Ok("Unauthorized"),
        403 => Ok("Forbidden"),
        404 => Ok("Not Found"),
        405 => Ok("Method Not Allowed"),
        409 => Ok("Conflict"),
        410 => Ok("Gone"),
        415 => Ok("Unsupported Media Type"),
        418 => Ok("I'm a teapot"),
        422 => Ok("Unprocessable Content"),
        429 => Ok("Too Many Requests"),
        500 => Ok("Internal Server Error"),
        501 => Ok("Not Implemented"),
        502 => Ok("Bad Gateway"),
        503 => Ok("Service Unavailable"),
        504 => Ok("Gateway Timeout"),
        _ => Err(VutilsError::InvalidInput(format!(
            "unknown HTTP status code {code}"
        ))),
    }
}

pub fn mime_lookup(extension: &str) -> &'static str {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "wasm" => "application/wasm",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "toml" => "application/toml",
        "md" => "text/markdown",
        _ => "application/octet-stream",
    }
}

fn render_curl(request: &HttpRequestSpec, shell: Shell) -> Result<String> {
    let quote = |value: &str| quote_shell(value, shell);
    let mut arguments = vec![
        "curl".to_owned(),
        "--request".into(),
        request.method.clone(),
    ];
    if request.follow_redirects {
        arguments.push("--location".into());
    }
    if request.compressed {
        arguments.push("--compressed".into());
    }
    for (name, value) in &request.headers {
        arguments.extend(["--header".into(), format!("{name}: {value}")]);
    }
    if let Some(body) = &request.body {
        match body {
            HttpBody::Text(value) => arguments.extend(["--data-raw".into(), value.clone()]),
            HttpBody::Json(value) => {
                if !request
                    .headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    arguments.extend(["--header".into(), "Content-Type: application/json".into()]);
                }
                arguments.extend([
                    "--data-raw".into(),
                    serde_json::to_string(value)
                        .map_err(|error| VutilsError::Message(error.to_string()))?,
                ]);
            }
            HttpBody::Form(values) => {
                for (name, value) in values {
                    arguments.extend(["--form".into(), format!("{name}={value}")]);
                }
            }
            HttpBody::File { path, binary } => arguments.extend([
                if *binary { "--data-binary" } else { "--data" }.into(),
                format!("@{path}"),
            ]),
        }
    }
    arguments.extend(["--url".into(), request.url.clone()]);
    Ok(arguments
        .iter()
        .map(|value| quote(value))
        .collect::<Vec<_>>()
        .join(" "))
}

fn render_httpie(request: &HttpRequestSpec, shell: Shell) -> Result<String> {
    let mut values = vec![
        "http".to_owned(),
        request.method.clone(),
        request.url.clone(),
    ];
    values.extend(
        request
            .headers
            .iter()
            .map(|(name, value)| format!("{name}:{value}")),
    );
    if let Some(body) = &request.body {
        match body {
            HttpBody::Json(value) => {
                if let Some(object) = value.as_object() {
                    values.extend(object.iter().map(|(key, value)| format!("{key}:={value}")));
                } else {
                    values.extend(["--raw".into(), value.to_string()]);
                }
            }
            HttpBody::Text(value) => values.extend(["--raw".into(), value.clone()]),
            HttpBody::File { path, .. } => values.push(format!("@{path}")),
            HttpBody::Form(form) => {
                values.extend(form.iter().map(|(key, value)| format!("{key}={value}")))
            }
        }
    }
    Ok(values
        .iter()
        .map(|value| quote_shell(value, shell))
        .collect::<Vec<_>>()
        .join(" "))
}

fn render_fetch(request: &HttpRequestSpec) -> Result<String> {
    let mut options = serde_json::Map::new();
    options.insert("method".into(), request.method.clone().into());
    let mut request_headers = request.headers.clone();
    if matches!(request.body.as_ref(), Some(HttpBody::Json(_)))
        && !request_headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    {
        request_headers.push(("Content-Type".into(), "application/json".into()));
    }
    let headers = serde_json::to_value(&request_headers).map_err(message)?;
    options.insert("headers".into(), headers.clone());
    if let Some(body) = &request.body {
        let prefix = if matches!(body, HttpBody::File { .. }) {
            "import { readFile } from \"node:fs/promises\";\n\n"
        } else {
            ""
        };
        let value = match body {
            HttpBody::Json(value) => format!(
                "JSON.stringify({})",
                serde_json::to_string_pretty(value).map_err(message)?
            ),
            HttpBody::Text(value) => serde_json::to_string(value).map_err(message)?,
            HttpBody::File { path, .. } => format!(
                "await readFile({})",
                serde_json::to_string(path).map_err(message)?
            ),
            HttpBody::Form(values) => format!(
                "new URLSearchParams({})",
                serde_json::to_string(values).map_err(message)?
            ),
        };
        let headers = serde_json::to_string_pretty(&headers).map_err(message)?;
        return Ok(format!(
            "{prefix}const response = await fetch({}, {{\n  method: {},\n  headers: {},\n  body: {value},\n}});",
            serde_json::to_string(&request.url).map_err(message)?,
            serde_json::to_string(&request.method).map_err(message)?,
            indent(&headers, 2)
        ));
    }
    Ok(format!(
        "const response = await fetch({}, {});",
        serde_json::to_string(&request.url).map_err(message)?,
        serde_json::to_string_pretty(&options).map_err(message)?
    ))
}

fn render_axios(request: &HttpRequestSpec) -> Result<String> {
    ensure_unique_headers(request)?;
    let prefix = if matches!(request.body.as_ref(), Some(HttpBody::File { .. })) {
        "import { readFile } from \"node:fs/promises\";\n\n"
    } else {
        ""
    };
    let body = match &request.body {
        None => "undefined".into(),
        Some(HttpBody::Json(value)) => serde_json::to_string_pretty(value).map_err(message)?,
        Some(HttpBody::Text(value)) => serde_json::to_string(value).map_err(message)?,
        Some(HttpBody::Form(values)) => format!(
            "new URLSearchParams({})",
            serde_json::to_string(values).map_err(message)?
        ),
        Some(HttpBody::File { path, .. }) => format!(
            "await readFile({})",
            serde_json::to_string(path).map_err(message)?
        ),
    };
    let headers = request.headers.iter().cloned().collect::<BTreeMap<_, _>>();
    Ok(format!(
        "{prefix}const response = await axios({{\n  method: {},\n  url: {},\n  headers: {},\n  data: {body},\n}});",
        serde_json::to_string(&request.method.to_ascii_lowercase()).map_err(message)?,
        serde_json::to_string(&request.url).map_err(message)?,
        indent(&serde_json::to_string_pretty(&headers).map_err(message)?, 2)
    ))
}

fn ensure_unique_headers(request: &HttpRequestSpec) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    for (name, _) in &request.headers {
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(VutilsError::Unsupported(format!(
                "Axios rendering cannot preserve repeated header `{name}`"
            )));
        }
    }
    Ok(())
}

fn validate_request(request: &HttpRequestSpec) -> Result<()> {
    validate_http_url(&request.url)?;
    if request.method.is_empty() {
        return Err(VutilsError::InvalidInput(
            "HTTP method cannot be empty".into(),
        ));
    }
    for (name, value) in &request.headers {
        let valid_name = !name.is_empty()
            && name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            });
        let valid_value = !value
            .bytes()
            .any(|byte| (byte < b' ' && byte != b'\t') || byte == 0x7f);
        if !valid_name || !valid_value {
            return Err(VutilsError::InvalidInput(format!(
                "invalid HTTP header `{name}`"
            )));
        }
    }
    Ok(())
}

fn validate_http_url(value: &str) -> Result<()> {
    let url = Url::parse(value)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(VutilsError::Unsupported(
            "only HTTP and HTTPS URLs are supported for request generation".into(),
        ));
    }
    Ok(())
}

fn reject_shell_syntax(command: &str) -> Result<()> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quote {
        None,
        Single,
        Double,
    }

    if command.contains(['\n', '\r']) {
        return shell_syntax_error();
    }
    let characters: Vec<_> = command.chars().collect();
    let mut quote = Quote::None;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        match quote {
            Quote::Single => {
                if character == '\'' {
                    quote = Quote::None;
                }
            }
            Quote::Double => match character {
                '"' => quote = Quote::None,
                '\\' => index += 1,
                '`' => return shell_syntax_error(),
                '$' if characters.get(index + 1) == Some(&'(') => return shell_syntax_error(),
                _ => {}
            },
            Quote::None => match character {
                '\'' => quote = Quote::Single,
                '"' => quote = Quote::Double,
                '\\' => index += 1,
                '`' | ';' | '|' | '&' | '>' | '<' | '(' | ')' => {
                    return shell_syntax_error();
                }
                '$' if characters.get(index + 1) == Some(&'(') => return shell_syntax_error(),
                _ => {}
            },
        }
        index += 1;
    }
    Ok(())
}

fn shell_syntax_error() -> Result<()> {
    Err(VutilsError::InvalidInput(
        "unquoted shell operators, substitutions, and redirections are not accepted".into(),
    ))
}

fn message(error: serde_json::Error) -> VutilsError {
    VutilsError::Message(error.to_string())
}

fn required_argument<'a>(
    arguments: &'a [String],
    index: &mut usize,
    option: &str,
) -> Result<&'a str> {
    *index += 1;
    arguments
        .get(*index)
        .map(String::as_str)
        .ok_or_else(|| VutilsError::InvalidInput(format!("{option} requires a value")))
}

fn quote_shell(value: &str, shell: Shell) -> String {
    match shell {
        Shell::Posix => shell_words::quote(value).into_owned(),
        Shell::PowerShell => format!("'{}'", value.replace('\'', "''")),
    }
}

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line.to_owned()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_round_trip_preserves_core_request() {
        let parsed = parse_curl("curl -X POST -H 'Content-Type: application/json' -d '{\"a\":1}' https://example.com/api").unwrap();
        let rendered = render(&parsed, HttpRenderer::Curl, Shell::Posix).unwrap();
        let reparsed = parse_curl(&rendered).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn curl_parser_rejects_shell_execution() {
        assert!(parse_curl("curl https://example.com | sh").is_err());
        assert!(parse_curl("curl $(cat url)").is_err());
    }

    #[test]
    fn curl_parser_allows_shell_metacharacters_when_safely_quoted() {
        let request = parse_curl(
            "curl -H 'Content-Type: application/json; charset=utf-8' 'https://example.com/?a=1&b=2'",
        )
        .unwrap();
        assert_eq!(request.url, "https://example.com/?a=1&b=2");
        assert_eq!(
            request.headers,
            vec![(
                "Content-Type".into(),
                "application/json; charset=utf-8".into()
            )]
        );
    }

    #[test]
    fn explain_redacts_authorization() {
        let output = explain_curl(
            "curl -H 'Authorization: Bearer secret' https://example.com",
            false,
        )
        .unwrap();
        assert!(output.contains("<redacted>"));
        assert!(!output.contains("Bearer secret"));
    }

    #[test]
    fn curl_parser_supports_attached_method_and_basic_auth() {
        let request = parse_curl("curl -XPOST -u 'user:pass' https://example.com").unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.headers,
            vec![("Authorization".into(), "Basic dXNlcjpwYXNz".into())]
        );
    }

    #[test]
    fn curl_parser_preserves_raw_data_semantics() {
        let request = parse_curl("curl -d '{\"a\":1}' https://example.com").unwrap();
        assert_eq!(request.body, Some(HttpBody::Text("{\"a\":1}".into())));

        let request = parse_curl("curl --data-raw '@literal' https://example.com").unwrap();
        assert_eq!(request.body, Some(HttpBody::Text("@literal".into())));

        let request = parse_curl("curl -d @payload.txt https://example.com").unwrap();
        let rendered = render(&request, HttpRenderer::Curl, Shell::Posix).unwrap();
        assert!(rendered.contains("--data @payload.txt"));
        assert!(!rendered.contains("--data-binary"));

        let request = parse_curl(
            "curl -H 'Content-Type: application/json' -d '{\"a\":1}' https://example.com",
        )
        .unwrap();
        assert_eq!(
            request.body,
            Some(HttpBody::Json(serde_json::json!({"a": 1})))
        );
    }

    #[test]
    fn curl_parser_rejects_interactive_auth_and_cookie_files() {
        assert!(parse_curl("curl -u user https://example.com").is_err());
        assert!(parse_curl("curl -b cookies.txt https://example.com").is_err());
    }

    #[test]
    fn fetch_preserves_repeated_headers_and_axios_rejects_them() {
        let mut request = HttpRequestSpec::new("GET", "https://example.com").unwrap();
        request.headers = vec![
            ("X-Tag".into(), "one".into()),
            ("X-Tag".into(), "two".into()),
        ];

        let fetch = render(&request, HttpRenderer::Fetch, Shell::Posix).unwrap();
        assert!(fetch.contains("[\n      \"X-Tag\",\n      \"one\""));
        assert!(fetch.contains("[\n      \"X-Tag\",\n      \"two\""));
        assert!(render(&request, HttpRenderer::Axios, Shell::Posix).is_err());
    }

    #[test]
    fn javascript_file_bodies_are_rendered_as_valid_node_expressions() {
        let mut request = HttpRequestSpec::new("POST", "https://example.com").unwrap();
        request.body = Some(HttpBody::File {
            path: "payload.bin".into(),
            binary: true,
        });
        for renderer in [HttpRenderer::Fetch, HttpRenderer::Axios] {
            let output = render(&request, renderer, Shell::Posix).unwrap();
            assert!(output.starts_with("import { readFile }"));
            assert!(output.contains("await readFile(\"payload.bin\")"));
        }
    }

    #[test]
    fn fetch_json_body_includes_content_type() {
        let mut request = HttpRequestSpec::new("POST", "https://example.com").unwrap();
        request.body = Some(HttpBody::Json(serde_json::json!({"ok": true})));
        let output = render(&request, HttpRenderer::Fetch, Shell::Posix).unwrap();
        assert!(output.contains("Content-Type"));
        assert!(output.contains("application/json"));
    }

    #[test]
    fn request_validation_rejects_invalid_header_syntax() {
        let mut request = HttpRequestSpec::new("GET", "https://example.com").unwrap();
        request.headers.push(("Bad Header".into(), "value".into()));
        assert!(render(&request, HttpRenderer::Curl, Shell::Posix).is_err());
        request.headers = vec![("X-Test".into(), "bad\0value".into())];
        assert!(render(&request, HttpRenderer::Curl, Shell::Posix).is_err());
    }
}
