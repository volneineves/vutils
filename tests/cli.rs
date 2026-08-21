use std::{fs, process::Command};

fn vutils() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vutils"));
    command.env("VUTILS_TEST_DISABLE_KEYRING", "1");
    command
}

fn vu() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vu"));
    command.env("VUTILS_TEST_DISABLE_KEYRING", "1");
    command
}

fn vutils_with_config(path: &std::path::Path) -> Command {
    let mut command = vutils();
    command.env("VUTILS_CONFIG", path);
    command
}

#[test]
fn author_flag_prints_package_metadata_without_a_subcommand() {
    let output = vutils().arg("--author").output().unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}\n", env!("CARGO_PKG_AUTHORS"))
    );
    assert!(output.stderr.is_empty());

    let help = vutils().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("--author"));

    let missing_subcommand = vutils().arg("--copy").output().unwrap();
    assert!(!missing_subcommand.status.success());
    assert!(
        String::from_utf8(missing_subcommand.stderr)
            .unwrap()
            .contains("a subcommand is required")
    );
}

#[test]
fn vu_is_a_first_class_alias_for_the_cli() {
    let version = vu().arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!("vu {}\n", env!("CARGO_PKG_VERSION"))
    );

    let help = vu().arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.starts_with("Fast, pipeline-friendly developer utilities"));
    assert!(help.contains("Usage: vu"));
}

#[test]
fn tui_requires_an_interactive_terminal_and_rejects_output_flags() {
    let non_interactive = vutils().arg("tui").output().unwrap();
    assert!(!non_interactive.status.success());
    assert!(
        String::from_utf8(non_interactive.stderr)
            .unwrap()
            .contains("requires an interactive terminal")
    );

    let incompatible = vutils().args(["--copy", "tui"]).output().unwrap();
    assert!(!incompatible.status.success());
    assert!(
        String::from_utf8(incompatible.stderr)
            .unwrap()
            .contains("output flags cannot be used")
    );
}

#[test]
fn vruno_configures_checks_and_syncs_natively() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let collection = directory.path().join("Bruno collection");
    fs::create_dir(&collection).unwrap();
    fs::write(
        collection.join("bruno.json"),
        r#"{"version":"1","name":"API","type":"collection"}"#,
    )
    .unwrap();
    let openapi = directory.path().join("openapi.yaml");
    fs::write(
        &openapi,
        "openapi: 3.1.0\ninfo: { title: API }\npaths:\n  /health:\n    get:\n      summary: Health\n      tags: [System]\n      responses: { '200': { description: OK } }\n",
    )
    .unwrap();
    // macOS exposes temporary directories through /private/var; compare the
    // same canonical locations that Vruno persists in its configuration.
    let collection = fs::canonicalize(collection).unwrap();
    let openapi = fs::canonicalize(openapi).unwrap();

    let configured = vutils_with_config(&config)
        .args([
            "vruno",
            "configure",
            "--collection",
            collection.to_str().unwrap(),
            "--openapi",
            openapi.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(configured.status.success());

    let shown = vutils_with_config(&config)
        .args(["vruno", "show"])
        .output()
        .unwrap();
    let shown = String::from_utf8(shown.stdout).unwrap();
    assert!(shown.contains("engine=native"));
    assert!(shown.contains(&format!("collection={}", collection.display())));
    assert!(shown.contains(&format!("openapi={}", openapi.display())));

    let checked = vutils_with_config(&config)
        .args(["vruno", "check", "--format", "json", "--group-by", "path"])
        .output()
        .unwrap();
    assert!(!checked.status.success(), "drift must remain observable");
    let checked = String::from_utf8(checked.stdout).unwrap();
    let report: serde_json::Value = serde_json::from_str(&checked).unwrap();
    assert_eq!(report["missing"].as_array().unwrap().len(), 1);

    let previewed = vutils_with_config(&config)
        .args(["vruno", "preview"])
        .output()
        .unwrap();
    assert!(previewed.status.success());
    assert!(
        String::from_utf8(previewed.stdout)
            .unwrap()
            .contains("Preview only")
    );
    assert!(!collection.join("System/Health.bru").exists());

    let unconfirmed = vutils_with_config(&config)
        .args(["vruno", "sync"])
        .output()
        .unwrap();
    assert!(!unconfirmed.status.success());
    assert!(
        String::from_utf8(unconfirmed.stderr)
            .unwrap()
            .contains("pass --yes to confirm")
    );

    let synced = vutils_with_config(&config)
        .args(["vruno", "sync", "--yes"])
        .output()
        .unwrap();
    assert!(synced.status.success());
    let synced = String::from_utf8(synced.stdout).unwrap();
    assert!(synced.contains("1 created"));
    assert!(collection.join("System/Health.bru").is_file());

    let clean = vutils_with_config(&config)
        .args(["vruno", "check"])
        .output()
        .unwrap();
    assert!(clean.status.success());
    assert!(String::from_utf8(clean.stdout).unwrap().contains("in sync"));
}

#[test]
fn vruno_rejects_in_place_before_loading_or_running_the_integration() {
    let output = vutils()
        .args(["--in-place", "vruno", "show"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--in-place cannot be used with Vruno")
    );
}

#[test]
fn vruno_rejects_deferred_output_failures_for_mutating_commands() {
    let output = vutils()
        .args(["--output", "report.txt", "vruno", "sync", "--yes"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("output flags cannot be used with Vruno configure or sync")
    );
}

#[test]
fn config_help_documents_every_supported_default() {
    let output = vutils().args(["config", "--help"]).output().unwrap();

    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for key in [
        "sql.dialect",
        "uuid.version",
        "uuid.format",
        "crypto.algorithm",
        "crypto.password-env",
        "crypto.password-file",
        "tui.home",
        "vruno.collection",
        "vruno.openapi",
    ] {
        assert!(help.contains(key), "config help is missing key {key}");
    }
    for value in [
        "generic",
        "postgres",
        "mysql",
        "sqlite",
        "mssql",
        "v1, v2, v3, v4, v5, v6, v7, v8",
        "hyphenated, simple, urn, braced",
        "xchacha20-poly1305, aes-256-gcm",
        "json.pretty, uuid, gen.password, enc, dec, sql.format",
    ] {
        assert!(help.contains(value), "config help is missing value {value}");
    }
    for guidance in [
        "explicit command flag > persisted config > built-in default",
        "Encryption keys are never stored directly",
        "stores only the environment-variable name",
        "automate enc and dec",
        "mutually exclusive",
        "VUTILS_CONFIG",
        "vu config set",
    ] {
        assert!(
            help.contains(guidance),
            "config help is missing guidance {guidance}"
        );
    }

    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let documented_values = [
        (
            "sql.dialect",
            "generic postgres mysql sqlite mssql postgresql sqlserver sql-server",
        ),
        ("uuid.version", "v1 v2 v3 v4 v5 v6 v7 v8"),
        ("uuid.format", "hyphenated simple urn braced hyphen brace"),
        (
            "crypto.algorithm",
            "xchacha20-poly1305 aes-256-gcm xchacha20 xchacha aes256-gcm aes",
        ),
        ("tui.home", "json.pretty,uuid,sql.format"),
    ];
    for (key, values) in documented_values {
        for value in values.split_ascii_whitespace() {
            let set = vutils_with_config(&config_path)
                .args(["config", "set", key, value])
                .output()
                .unwrap();
            assert!(
                set.status.success(),
                "documented config value {key}={value} was rejected: {}",
                String::from_utf8_lossy(&set.stderr)
            );
        }
    }

    for (key, value) in [
        ("sql-dialect", "generic"),
        ("uuid-version", "v7"),
        ("uuid-format", "hyphenated"),
        ("crypto-algorithm", "xchacha20-poly1305"),
        ("enc.algorithm", "aes-256-gcm"),
        ("enc.password-env", "VUTILS_PASSWORD"),
        ("enc.password-file", "password.txt"),
        ("tui-home", "json.pretty,uuid"),
        ("vruno-collection", "collections/api"),
        ("vruno-openapi", "specs/openapi.yaml"),
    ] {
        let set = vutils_with_config(&config_path)
            .args(["config", "set", key, value])
            .output()
            .unwrap();
        assert!(
            set.status.success(),
            "documented config alias {key} was rejected: {}",
            String::from_utf8_lossy(&set.stderr)
        );
    }
}

#[test]
fn formats_json_from_stdin() {
    let output = vutils()
        .args(["json", "pretty"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write as _;
            child.stdin.take().unwrap().write_all(br#"{"a":1}"#)?;
            child.wait_with_output()
        })
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\n  \"a\": 1\n}\n"
    );
}

#[test]
fn renders_mermaid_as_unicode_or_portable_ascii() {
    let source = "flowchart LR\n  edit[Edit] --> render[Rendered]";
    let unicode = vu()
        .args(["mermaid", "render", "--literal", source])
        .output()
        .unwrap();
    assert!(unicode.status.success());
    let unicode = String::from_utf8(unicode.stdout).unwrap();
    assert!(unicode.contains("Edit"));
    assert!(unicode.contains("Rendered"));
    assert!(unicode.contains('┌'));

    let ascii = vu()
        .args(["mermaid", "render", "--ascii", "--literal", source])
        .output()
        .unwrap();
    assert!(ascii.status.success());
    let ascii = String::from_utf8(ascii.stdout).unwrap();
    assert!(ascii.contains("Edit"));
    assert!(!ascii.contains(['┌', '─', '│', '▶']));
}

#[test]
fn mermaid_reports_unsupported_diagram_families() {
    let output = vu()
        .args(["mermaid", "render", "--literal", "gitGraph\n  commit"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("unsupported diagram type"));
    assert!(error.contains("flowchart"));
}

#[test]
fn existing_positional_file_is_read_and_can_be_updated_in_place() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("value.json");
    fs::write(&path, r#"{"name":"Ana"}"#).unwrap();

    let formatted = vutils()
        .args(["json", "pretty"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(formatted.status.success());
    assert_eq!(
        String::from_utf8(formatted.stdout).unwrap(),
        "{\n  \"name\": \"Ana\"\n}\n"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"name":"Ana"}"#);

    let updated = vutils()
        .args(["--in-place", "json", "pretty"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(updated.status.success());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\n  \"name\": \"Ana\"\n}\n"
    );
}

#[test]
fn literal_flag_disambiguates_an_existing_file_name() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.txt");
    fs::write(&path, "contents from file").unwrap();

    let detected_file = vutils().args(["text", "trim"]).arg(&path).output().unwrap();
    assert!(detected_file.status.success());
    assert_eq!(detected_file.stdout, b"contents from file\n");

    let forced_literal = vutils()
        .args(["text", "trim", "--literal"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(forced_literal.status.success());
    assert_eq!(
        String::from_utf8(forced_literal.stdout).unwrap(),
        format!("{}\n", path.display())
    );
}

#[test]
fn in_place_failure_preserves_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.json");
    fs::write(&path, "{broken").unwrap();
    let status = vutils()
        .args(["--in-place", "json", "pretty", "--input"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "{broken");
}

#[test]
fn output_refuses_overwrite_without_force() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("output.txt");
    fs::write(&path, "original").unwrap();
    let status = vutils()
        .args(["--output"])
        .arg(&path)
        .args(["base64", "encode", "value"])
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "original");
}

#[test]
fn text_file_output_has_a_final_newline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("output.txt");
    let status = vutils()
        .args(["--output"])
        .arg(&path)
        .args(["base64", "encode", "value"])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "dmFsdWU=\n");
}

#[test]
fn binary_bit_codec_round_trips_text_through_stdout() {
    let encoded = vutils()
        .args(["binary", "encode", "Codex", "--spaced"])
        .output()
        .unwrap();
    assert!(encoded.status.success());
    assert_eq!(
        String::from_utf8(encoded.stdout).unwrap(),
        "01000011 01101111 01100100 01100101 01111000\n"
    );

    let decoded = vutils()
        .args([
            "bin",
            "decode",
            "01000011 01101111 01100100 01100101 01111000",
        ])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, b"Codex");

    let partial = vutils().args(["binary", "decode", "101"]).output().unwrap();
    assert!(!partial.status.success());
    assert!(
        String::from_utf8(partial.stderr)
            .unwrap()
            .contains("complete 8-bit bytes")
    );
}

#[test]
fn successful_in_place_format_is_atomic_and_newline_terminated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("value.json");
    fs::write(&path, r#"{"a":1}"#).unwrap();
    let status = vutils()
        .args(["--in-place", "json", "pretty", "--input"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "{\n  \"a\": 1\n}\n");
}

#[test]
fn curl_formatter_rejects_shell_operators() {
    let output = vutils()
        .args(["curl", "format", "curl https://example.com | sh"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("shell operators")
    );
}

#[test]
fn curl_and_sql_expose_only_formatting_commands() {
    let root = vutils().arg("--help").output().unwrap();
    assert!(root.status.success());
    let root = String::from_utf8(root.stdout).unwrap();
    assert!(
        !root
            .lines()
            .any(|line| line.trim_start().starts_with("http "))
    );

    for command in ["curl", "sql"] {
        let help = vutils().args([command, "--help"]).output().unwrap();
        assert!(help.status.success());
        let help = String::from_utf8(help.stdout).unwrap();
        assert!(help.contains("format"));
        for removed in ["insert", "update", "parse", "convert", "explain"] {
            assert!(!help.lines().any(|line| {
                line.trim_start()
                    .strip_prefix(removed)
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            }));
        }
    }
}

#[test]
fn sql_formatter_uses_the_selected_dialect() {
    let output = vutils()
        .args([
            "sql",
            "format",
            "select `name` from `users`",
            "--dialect",
            "mysql",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("`name`"));

    let incompatible = vutils()
        .args([
            "sql",
            "format",
            "select `name` from `users`",
            "--dialect",
            "postgres",
        ])
        .output()
        .unwrap();
    assert!(!incompatible.status.success());
}

#[test]
fn fixed_node_uuid_v2_batch_is_unique_and_bounded() {
    let common = [
        "uuid",
        "--version",
        "v2",
        "--node-id",
        "001122334455",
        "--domain",
        "person",
        "--local-id",
        "42",
    ];
    let output = vutils()
        .args(common)
        .args(["--count", "64"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let values: std::collections::HashSet<_> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(values.len(), 64);

    let status = vutils()
        .args(common)
        .args(["--count", "65"])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn uuid_validator_accepts_supported_formats_and_rejects_invalid_input() {
    for value in [
        "018f1f4e-7b2c-7abc-8def-0123456789ab",
        "018f1f4e7b2c7abc8def0123456789ab",
        "urn:uuid:018f1f4e-7b2c-7abc-8def-0123456789ab",
        "{018f1f4e-7b2c-7abc-8def-0123456789ab}",
    ] {
        let output = vutils()
            .args(["uuid", "--validate", value])
            .output()
            .unwrap();
        assert!(output.status.success(), "expected {value} to be valid");
        assert_eq!(output.stdout, b"valid\n");
        assert!(output.stderr.is_empty());
    }

    let invalid = vutils()
        .args(["uuid", "--validate", "018f1f4e-7b2c-7abc-8def-0123456789ag"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert_eq!(invalid.stdout, b"invalid\n");
    assert!(invalid.stderr.is_empty());

    let conflicting = vutils()
        .args([
            "uuid",
            "--validate",
            "018f1f4e-7b2c-7abc-8def-0123456789ab",
            "--version",
            "v7",
        ])
        .output()
        .unwrap();
    assert!(!conflicting.status.success());
    assert!(
        String::from_utf8(conflicting.stderr)
            .unwrap()
            .contains("cannot be used with")
    );
}

#[test]
fn jwt_decode_warns_that_signature_is_unverified() {
    let output = vutils()
        .args(["jwt", "decode", "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0."])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("not verified")
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["verified"], false);
}

#[test]
fn converts_common_case_styles_and_aliases() {
    for (style, input, expected) in [
        ("camel", "hello world", "helloWorld\n"),
        ("camelCase", "hello world", "helloWorld\n"),
        ("PascalCase", "hello world", "HelloWorld\n"),
        ("snake_case", "helloWorld", "hello_world\n"),
        ("snake-case", "helloWorld", "hello_world\n"),
        ("pascalcase", "hello world", "HelloWorld\n"),
    ] {
        let output = vutils()
            .args(["text", "case", style, input])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }
}

#[test]
fn config_defaults_apply_and_explicit_flags_override_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");

    for (key, value) in [
        ("sql.dialect", "mysql"),
        ("uuid.version", "v4"),
        ("uuid.format", "simple"),
        ("crypto.algorithm", "aes-256-gcm"),
        ("crypto.password-env", "TEST_VUTILS_PASSWORD"),
    ] {
        let output = vutils_with_config(&path)
            .args(["config", "set", key, value])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let listed = vutils_with_config(&path)
        .args(["config", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed = String::from_utf8(listed.stdout).unwrap();
    assert!(listed.contains("sql.dialect=mysql"));
    assert!(listed.contains("crypto.password-env=TEST_VUTILS_PASSWORD"));
    assert!(listed.contains("tui.home=json.pretty,uuid,gen.password,enc,dec,sql.format"));
    assert!(!listed.contains("test-password"));

    let sql = vutils_with_config(&path)
        .args(["sql", "format", "select `name` from `users`"])
        .output()
        .unwrap();
    assert!(sql.status.success());
    assert!(String::from_utf8(sql.stdout).unwrap().contains("`name`"));

    let uuid = vutils_with_config(&path).arg("uuid").output().unwrap();
    assert!(uuid.status.success());
    let uuid = String::from_utf8(uuid.stdout).unwrap();
    assert_eq!(uuid.trim().len(), 32);
    assert_eq!(
        uuid::Uuid::parse_str(uuid.trim())
            .unwrap()
            .get_version_num(),
        4
    );

    let encrypted = vutils_with_config(&path)
        .env("TEST_VUTILS_PASSWORD", "test-password")
        .args(["enc", "message"])
        .output()
        .unwrap();
    assert!(encrypted.status.success());
    assert_eq!(encrypted.stderr, b"algorithm: aes-256-gcm\n");

    let overridden = vutils_with_config(&path)
        .env("TEST_VUTILS_PASSWORD", "test-password")
        .args(["enc", "message", "--alg", "xchacha20-poly1305"])
        .output()
        .unwrap();
    assert!(overridden.status.success());
    assert_eq!(overridden.stderr, b"algorithm: xchacha20-poly1305\n");
}

#[test]
fn config_rejects_plaintext_password_storage() {
    let directory = tempfile::tempdir().unwrap();
    let output = vutils_with_config(&directory.path().join("config.toml"))
        .args(["config", "set", "crypto.password", "secret"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown config key")
    );
}

#[test]
fn time_is_local_by_default_and_utc_on_request() {
    let local = vutils()
        .args(["time", "to-iso", "1700000000"])
        .output()
        .unwrap();
    assert!(local.status.success());
    let local_value = String::from_utf8(local.stdout).unwrap();
    let parsed_local = chrono::DateTime::parse_from_rfc3339(local_value.trim()).unwrap();
    let expected_local = chrono::DateTime::from_timestamp(1_700_000_000, 0)
        .unwrap()
        .with_timezone(&chrono::Local);
    assert_eq!(parsed_local.offset(), expected_local.offset());

    let utc = vutils()
        .args(["time", "to-iso", "1700000000", "--utc"])
        .output()
        .unwrap();
    assert!(utc.status.success());
    assert!(String::from_utf8(utc.stdout).unwrap().trim().ends_with('Z'));

    let now = vutils().args(["time", "now"]).output().unwrap();
    assert!(now.status.success());
    chrono::DateTime::parse_from_rfc3339(String::from_utf8(now.stdout).unwrap().trim()).unwrap();

    let unix = vutils().args(["time", "now", "--unix"]).output().unwrap();
    assert!(unix.status.success());
    String::from_utf8(unix.stdout)
        .unwrap()
        .trim()
        .parse::<i64>()
        .unwrap();
}

#[test]
fn encrypts_and_decrypts_with_key_and_legacy_passwd_alias() {
    for algorithm in ["aes-256-gcm", "xchacha20-poly1305"] {
        let encrypted = vutils()
            .args(["enc", "Texto secreto", "--key", "123", "--alg", algorithm])
            .output()
            .unwrap();
        assert!(encrypted.status.success());
        assert_eq!(
            String::from_utf8(encrypted.stderr).unwrap(),
            format!("algorithm: {algorithm}\n")
        );
        let envelope = String::from_utf8(encrypted.stdout).unwrap();
        assert!(envelope.starts_with("vutils:v1:"));

        let decrypted = vutils()
            .args(["dec", envelope.trim(), "--passwd", "123"])
            .output()
            .unwrap();
        assert!(decrypted.status.success());
        assert_eq!(decrypted.stdout, b"Texto secreto");
        assert_eq!(
            String::from_utf8(decrypted.stderr).unwrap(),
            format!("algorithm: {algorithm}\n")
        );

        let wrong_password = vutils()
            .args(["dec", envelope.trim(), "--key", "wrong"])
            .output()
            .unwrap();
        assert!(!wrong_password.status.success());
        assert!(
            String::from_utf8(wrong_password.stderr)
                .unwrap()
                .contains("wrong password or corrupted")
        );
    }
}

#[test]
fn encryption_and_config_expose_the_saved_key_lifecycle() {
    let missing = vutils().args(["enc", "message"]).output().unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .contains("first save a key with a successful enc/dec command")
    );

    let forgotten = vutils().args(["config", "forget-key"]).output().unwrap();
    assert!(forgotten.status.success());
    assert_eq!(forgotten.stdout, b"no saved encryption key\n");
}

#[test]
fn encryption_help_lists_reversible_algorithms_and_sha_guidance() {
    let output = vutils().args(["enc", "--help"]).output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("aes-256-gcm"));
    assert!(help.contains("xchacha20-poly1305"));
    assert!(help.contains("--key <KEY>"));
    assert!(help.contains("alias: --passwd"));
    assert!(help.contains("SHA algorithms are hashes and are not reversible"));
}

#[test]
fn encryption_defaults_to_xchacha20_poly1305() {
    let output = vutils()
        .args(["enc", "message", "--passwd", "123"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "algorithm: xchacha20-poly1305\n"
    );
}

#[test]
fn brazilian_profile_help_generation_and_validation_are_cohesive() {
    let help = vutils().args(["br", "--help"]).output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for command in ["cpf", "cnpj", "cep", "phone", "pix"] {
        assert!(help.contains(command));
    }

    let profile = vutils().arg("br").output().unwrap();
    assert!(profile.status.success());
    let profile: serde_json::Value = serde_json::from_slice(&profile.stdout).unwrap();
    let cpf = profile["cpf"].as_str().unwrap();
    let cnpj = profile["cnpj"].as_str().unwrap();
    assert!(vutils::countries::br::validate_cpf(cpf));
    assert!(vutils::countries::br::validate_cnpj(cnpj));
    assert!(profile["cep"].is_string());
    assert!(profile["phone"].is_string());
    assert!(profile["pix"].is_string());

    let valid = vutils()
        .args(["br", "cnpj", "--validate", cnpj])
        .output()
        .unwrap();
    assert!(valid.status.success());
    assert_eq!(valid.stdout, b"valid\n");

    let invalid = vutils()
        .args(["br", "cpf", "--validate", "111.111.111-11"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert_eq!(invalid.stdout, b"invalid\n");
}
