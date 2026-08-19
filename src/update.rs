use std::{env, fs, io::Write as _, path::Path, time::Duration};

use sha2::{Digest as _, Sha256};
use tempfile::NamedTempFile;

use crate::VutilsError;

const REPOSITORY: &str = "volneineves/vutils";
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/volneineves/vutils/releases/latest";
const MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;
const UPDATE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, serde::Deserialize)]
struct LatestRelease {
    tag_name: String,
}

pub fn run() -> Result<String, VutilsError> {
    let executable = env::current_exe().map_err(|error| {
        VutilsError::Message(format!("cannot locate the current executable: {error}"))
    })?;
    let executable_name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| VutilsError::Message("current executable has no valid file name".into()))?;
    let (asset_prefix, platform) = platform_asset()?;
    let asset_name = format!("{asset_prefix}-{platform}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(UPDATE_TIMEOUT))
        .user_agent(concat!("vutils/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let release = fetch_latest_release(&agent)?;
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let latest_version = semver::Version::parse(latest).map_err(|error| {
        VutilsError::Message(format!(
            "GitHub returned an invalid release tag `{}`: {error}",
            release.tag_name
        ))
    })?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("package version must be valid semver");
    if latest_version <= current_version {
        return Ok(format!(
            "{executable_name} is already up to date ({current_version})"
        ));
    }

    let base_url = format!("https://github.com/{REPOSITORY}/releases/download/v{latest}");
    let asset_url = format!("{base_url}/{asset_name}");
    let checksums_url = format!("{base_url}/SHA256SUMS");
    let asset = download(&agent, &asset_url)?;
    let checksums = download(&agent, &checksums_url)?;
    let expected = checksum_for(&checksums, &asset_name)?;
    let actual = Sha256::digest(&asset);
    if actual.as_slice() != expected.as_slice() {
        return Err(VutilsError::Message(format!(
            "checksum mismatch for downloaded {asset_name}; refusing to replace the executable"
        )));
    }

    replace_executable(&executable, &asset)?;
    Ok(format!(
        "updated {executable_name} from {current_version} to {latest_version}"
    ))
}

fn fetch_latest_release(agent: &ureq::Agent) -> Result<LatestRelease, VutilsError> {
    let mut response = agent.get(LATEST_RELEASE_URL).call().map_err(|error| {
        VutilsError::Message(format!(
            "failed to check the latest GitHub release: {error}"
        ))
    })?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_vec()
        .map_err(|error| {
            VutilsError::Message(format!("failed to read GitHub release metadata: {error}"))
        })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        VutilsError::Message(format!("GitHub returned invalid release metadata: {error}"))
    })
}

fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, VutilsError> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| VutilsError::Message(format!("failed to download `{url}`: {error}")))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_vec()
        .map_err(|error| VutilsError::Message(format!("failed to read `{url}`: {error}")))
}

fn checksum_for(checksums: &[u8], asset_name: &str) -> Result<Vec<u8>, VutilsError> {
    let text = std::str::from_utf8(checksums)
        .map_err(|error| VutilsError::Message(format!("SHA256SUMS is not valid UTF-8: {error}")))?;
    let line = text
        .lines()
        .find(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|name| name == asset_name)
        })
        .ok_or_else(|| {
            VutilsError::Message(format!("SHA256SUMS does not contain `{asset_name}`"))
        })?;
    let digest = line.split_whitespace().next().unwrap_or_default();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VutilsError::Message(format!(
            "SHA256SUMS contains an invalid digest for `{asset_name}`"
        )));
    }
    (0..digest.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&digest[index..index + 2], 16).map_err(|_| {
                VutilsError::Message(format!(
                    "SHA256SUMS contains an invalid digest for `{asset_name}`"
                ))
            })
        })
        .collect()
}

fn platform_asset() -> Result<(&'static str, &'static str), VutilsError> {
    let prefix = if env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name == "vu"))
        .unwrap_or(false)
    {
        "vu"
    } else {
        "vutils"
    };
    let platform = match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("macos", "x86_64") => "macos-x86_64",
        ("macos", "aarch64") => "macos-aarch64",
        (os, arch) => {
            return Err(VutilsError::Unsupported(format!(
                "automatic updates are not available for {os}/{arch}; download a supported release asset manually"
            )));
        }
    };
    Ok((prefix, platform))
}

fn replace_executable(path: &Path, bytes: &[u8]) -> Result<(), VutilsError> {
    let parent = path
        .parent()
        .ok_or_else(|| VutilsError::Message("current executable has no parent directory".into()))?;
    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|error| update_write_error(path, error))?;
    temporary
        .write_all(bytes)
        .map_err(|error| update_write_error(path, error))?;
    temporary
        .flush()
        .map_err(|error| update_write_error(path, error))?;
    let permissions = fs::metadata(path)
        .map_err(|error| update_write_error(path, error))?
        .permissions();
    fs::set_permissions(temporary.path(), permissions)
        .map_err(|error| update_write_error(path, error))?;
    temporary
        .persist(path)
        .map_err(|error| update_write_error(path, error.error))?;
    Ok(())
}

fn update_write_error(path: &Path, error: std::io::Error) -> VutilsError {
    VutilsError::Message(format!(
        "cannot replace `{}`; run the update with permission to write its directory ({error})",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::{checksum_for, platform_asset};

    #[test]
    fn parses_checksum_for_exact_asset_name() {
        let checksums = b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  vutils-linux-x86_64\n";
        let digest = checksum_for(checksums, "vutils-linux-x86_64").unwrap();
        assert_eq!(
            digest,
            vec![
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]
        );
    }

    #[test]
    fn rejects_malformed_checksum() {
        let checksums = b"abc  vutils-linux-x86_64\ndef  vu-linux-x86_64\n";
        let error = checksum_for(checksums, "vutils-linux-x86_64").unwrap_err();
        assert!(error.to_string().contains("invalid digest"));
    }

    #[test]
    fn rejects_missing_checksum() {
        let error = checksum_for(&[0; 0], "vutils-linux-x86_64").unwrap_err();
        assert!(error.to_string().contains("does not contain"));
    }

    #[test]
    fn detects_supported_platform() {
        let (_, platform) = platform_asset().unwrap();
        assert!(platform.starts_with("linux-") || platform.starts_with("macos-"));
    }
}
