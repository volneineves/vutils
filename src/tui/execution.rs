use std::{
    env,
    io::{self, Write as _},
    process::{Command, Stdio},
    str,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const MAX_PREVIEW_BYTES: usize = 1_000_000;

pub(super) struct Execution {
    pub(super) status: Option<i32>,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) elapsed: Duration,
}

pub(super) fn spawn(
    args: Vec<String>,
    input: Vec<u8>,
) -> std::result::Result<Receiver<std::result::Result<Execution, String>>, String> {
    let executable = env::current_exe()
        .map_err(|error| format!("cannot locate the vutils executable: {error}"))?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let started = Instant::now();
        let result = (|| {
            let (args, sensitive_environment) = protect_direct_secrets(args);
            let mut child = Command::new(executable)
                .args(args)
                .envs(sensitive_environment)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| format!("failed to start vutils: {error}"))?;
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "failed to open command input".to_string())?;
            let write_result = stdin.write_all(&input);
            drop(stdin);
            let output = child
                .wait_with_output()
                .map_err(|error| format!("failed to wait for vutils: {error}"))?;
            if let Err(error) = write_result
                && error.kind() != io::ErrorKind::BrokenPipe
            {
                return Err(format!("failed to write command input: {error}"));
            }
            Ok(Execution {
                status: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
                elapsed: started.elapsed(),
            })
        })();
        let _ = sender.send(result);
    });
    Ok(receiver)
}

fn protect_direct_secrets(args: Vec<String>) -> (Vec<String>, Vec<(String, String)>) {
    const KEY_FLAG: &[(&str, &str, &str)] = &[("--key", "--key-env", "VUTILS_TUI_KEY")];
    const SECRET_FLAG: &[(&str, &str, &str)] = &[("--secret", "--secret-env", "VUTILS_TUI_SECRET")];
    let sensitive_flags = match args.first().map(String::as_str) {
        Some("enc" | "dec") => KEY_FLAG,
        Some("hmac" | "password-hash" | "totp") => SECRET_FLAG,
        _ => return (args, Vec::new()),
    };
    let mut protected = Vec::with_capacity(args.len());
    let mut environment = Vec::new();
    let mut arguments = args.into_iter();
    while let Some(argument) = arguments.next() {
        let Some((_, environment_flag, environment_name)) = sensitive_flags
            .iter()
            .find(|(sensitive_flag, _, _)| *sensitive_flag == argument)
        else {
            protected.push(argument);
            continue;
        };
        let Some(secret) = arguments.next() else {
            protected.push(argument);
            continue;
        };
        protected.push((*environment_flag).into());
        protected.push((*environment_name).into());
        environment.push(((*environment_name).into(), secret));
    }
    (protected, environment)
}

pub(super) fn clipboard_text(execution: &Execution) -> Option<String> {
    str::from_utf8(&execution.stdout)
        .ok()
        .map(ToOwned::to_owned)
}

pub(super) fn format_execution(execution: &Execution) -> String {
    let mut sections = Vec::new();
    if !execution.stdout.is_empty() {
        sections.push(preview_bytes(&execution.stdout));
    }
    if !execution.stderr.is_empty() {
        sections.push(format!("stderr:\n{}", preview_bytes(&execution.stderr)));
    }
    if sections.is_empty() {
        sections.push("(command produced no output)".into());
    }
    sections.join("\n\n")
}

fn preview_bytes(bytes: &[u8]) -> String {
    let visible = &bytes[..bytes.len().min(MAX_PREVIEW_BYTES)];
    let truncated = bytes.len() > visible.len();
    let mut output = match str::from_utf8(visible) {
        Ok(value) => sanitize_text(value),
        Err(_) => {
            let mut rendered = String::from("binary output (hex preview):\n");
            for (offset, chunk) in visible.chunks(16).enumerate() {
                rendered.push_str(&format!("{:08x}  ", offset * 16));
                for byte in chunk {
                    rendered.push_str(&format!("{byte:02x} "));
                }
                rendered.push('\n');
            }
            rendered
        }
    };
    if truncated {
        output.push_str(&format!(
            "\n… preview truncated ({} bytes total)",
            bytes.len()
        ));
    }
    output.trim_end_matches('\n').to_string()
}

fn sanitize_text(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\n' | '\t' => character.to_string(),
            character if character.is_control() => format!("\\u{{{:x}}}", u32::from(character)),
            character => character.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_output_is_rendered_as_hex() {
        let preview = preview_bytes(&[0xff, 0x00, 0x1b]);
        assert!(preview.contains("binary output (hex preview)"));
        assert!(preview.contains("ff 00 1b"));
    }

    #[test]
    fn text_output_escapes_terminal_controls() {
        assert_eq!(sanitize_text("safe\u{1b}[31m"), "safe\\u{1b}[31m");
    }

    #[test]
    fn direct_secrets_are_moved_out_of_child_process_arguments() {
        let (args, environment) = protect_direct_secrets(vec![
            "enc".into(),
            "--key".into(),
            "not-visible-in-argv".into(),
        ]);

        assert_eq!(args, ["enc", "--key-env", "VUTILS_TUI_KEY"]);
        assert_eq!(
            environment,
            [("VUTILS_TUI_KEY".into(), "not-visible-in-argv".into())]
        );
        assert!(
            !args
                .iter()
                .any(|argument| argument == "not-visible-in-argv")
        );
    }

    #[test]
    fn secret_like_values_are_untouched_for_unrelated_commands() {
        let original = vec!["semver".into(), "compare".into(), "--secret".into()];
        let (args, environment) = protect_direct_secrets(original.clone());

        assert_eq!(args, original);
        assert!(environment.is_empty());
    }
}
