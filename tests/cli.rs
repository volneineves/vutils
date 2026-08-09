use std::{fs, process::Command};

fn vutils() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vutils"))
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
fn curl_parser_rejects_shell_operators() {
    let output = vutils()
        .args(["curl", "parse", "curl https://example.com | sh"])
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
fn sql_insert_does_not_interpolate_values() {
    let output = vutils()
        .args([
            "sql",
            "insert",
            "users",
            r#"{"name":"O'Reilly"}"#,
            "--dialect",
            "postgres",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["sql"].as_str().unwrap().contains("$1"));
    assert!(!value["sql"].as_str().unwrap().contains("O'Reilly"));
    assert_eq!(value["params"][0], "O'Reilly");
}

#[test]
fn sql_insert_accepts_csv_and_returns_parameters() {
    let output = vutils()
        .args([
            "sql",
            "insert",
            "users",
            "name,role\nAna,admin\n",
            "--csv",
            "--dialect",
            "postgres",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["params"], serde_json::json!(["Ana", "admin"]));
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
fn encrypts_and_decrypts_with_password_and_selected_algorithm() {
    for algorithm in ["aes-256-gcm", "xchacha20-poly1305"] {
        let encrypted = vutils()
            .args([
                "enc",
                "Texto secreto",
                "--passwd",
                "123",
                "--alg",
                algorithm,
            ])
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
            .args(["dec", envelope.trim(), "--passwd", "wrong"])
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
fn encryption_help_lists_reversible_algorithms_and_sha_guidance() {
    let output = vutils().args(["enc", "--help"]).output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("aes-256-gcm"));
    assert!(help.contains("xchacha20-poly1305"));
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
