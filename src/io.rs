use std::{
    borrow::Cow,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
};

use tempfile::NamedTempFile;

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Default)]
pub struct InputArgs {
    pub value: Option<String>,
    pub input: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct OutputArgs {
    pub output: Option<PathBuf>,
    pub in_place: bool,
    pub force: bool,
    pub copy: bool,
}

pub fn read_input(args: &InputArgs) -> Result<Vec<u8>> {
    match (&args.value, &args.input) {
        (Some(_), Some(_)) => Err(VutilsError::InvalidInput(
            "a positional value and --input cannot be used together".into(),
        )),
        (Some(value), None) => Ok(value.as_bytes().to_vec()),
        (None, Some(path)) => fs::read(path).map_err(|source| VutilsError::Read {
            path: path.clone(),
            source,
        }),
        (None, None) if io::stdin().is_terminal() => Err(VutilsError::InvalidInput(
            "provide a value, --input <path>, or pipe data through stdin".into(),
        )),
        (None, None) => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            Ok(bytes)
        }
    }
}

pub fn read_text(args: &InputArgs) -> Result<String> {
    String::from_utf8(read_input(args)?)
        .map_err(|error| VutilsError::InvalidInput(format!("input is not valid UTF-8: {error}")))
}

pub fn emit(bytes: &[u8], input: &InputArgs, output: &OutputArgs, textual: bool) -> Result<()> {
    if output.in_place && output.output.is_some() {
        return Err(VutilsError::InvalidInput(
            "--in-place and --output cannot be used together".into(),
        ));
    }

    let bytes: Cow<'_, [u8]> = if textual && !bytes.ends_with(b"\n") {
        let mut value = bytes.to_vec();
        value.push(b'\n');
        Cow::Owned(value)
    } else {
        Cow::Borrowed(bytes)
    };

    if output.in_place {
        let path = input.input.as_deref().ok_or_else(|| {
            VutilsError::InvalidInput("--in-place requires --input <path>".into())
        })?;
        atomic_write(path, &bytes)?;
    } else if let Some(path) = &output.output {
        write_output(path, &bytes, output.force)?;
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&bytes)?;
        stdout.flush()?;
    }

    if output.copy {
        let value = std::str::from_utf8(&bytes)
            .map_err(|_| VutilsError::InvalidInput("--copy only supports UTF-8 output".into()))?;
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|error| VutilsError::Message(format!("clipboard unavailable: {error}")))?;
        clipboard
            .set_text(value)
            .map_err(|error| VutilsError::Message(format!("failed to copy output: {error}")))?;
    }

    Ok(())
}

fn write_output(path: &Path, bytes: &[u8], force: bool) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(force);
    if !force {
        options.create_new(true);
    }
    let mut file = options.open(path).map_err(|source| VutilsError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(bytes).map_err(|source| VutilsError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| VutilsError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let permissions = fs::metadata(path)
        .map_err(|source| VutilsError::Read {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    let mut temporary = NamedTempFile::new_in(parent).map_err(|source| VutilsError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temporary
        .as_file_mut()
        .write_all(bytes)
        .and_then(|()| temporary.as_file_mut().sync_all())
        .map_err(|source| VutilsError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    temporary
        .as_file_mut()
        .set_permissions(permissions)
        .map_err(|source| VutilsError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    temporary
        .persist(path)
        .map_err(|error| VutilsError::Write {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_content_and_preserves_file_on_success() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("value.txt");
        fs::write(&path, "before").unwrap();
        atomic_write(&path, b"after").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "after");
    }
}
