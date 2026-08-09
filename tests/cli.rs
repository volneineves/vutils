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
