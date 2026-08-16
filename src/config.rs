use std::{
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{Result, VutilsError};

pub const CONFIG_ENV: &str = "VUTILS_CONFIG";
pub const DEFAULT_SQL_DIALECT: &str = "generic";
pub const DEFAULT_UUID_VERSION: &str = "v7";
pub const DEFAULT_UUID_FORMAT: &str = "hyphenated";
pub const DEFAULT_CRYPTO_ALGORITHM: &str = "xchacha20-poly1305";
pub const DEFAULT_TUI_HOME: &[&str] = &[
    "json.pretty",
    "uuid",
    "gen.password",
    "enc",
    "dec",
    "sql.format",
];

const MAX_TUI_HOME_ITEMS: usize = 20;

const CONFIG_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConfigData {
    version: u8,
    #[serde(skip_serializing_if = "SqlDefaults::is_empty")]
    sql: SqlDefaults,
    #[serde(skip_serializing_if = "UuidDefaults::is_empty")]
    uuid: UuidDefaults,
    #[serde(skip_serializing_if = "CryptoDefaults::is_empty")]
    crypto: CryptoDefaults,
    #[serde(skip_serializing_if = "TuiDefaults::is_empty")]
    tui: TuiDefaults,
    #[serde(skip_serializing_if = "VrunoDefaults::is_empty")]
    vruno: VrunoDefaults,
}

impl Default for ConfigData {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            sql: SqlDefaults::default(),
            uuid: UuidDefaults::default(),
            crypto: CryptoDefaults::default(),
            tui: TuiDefaults::default(),
            vruno: VrunoDefaults::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SqlDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    dialect: Option<String>,
}

impl SqlDefaults {
    fn is_empty(&self) -> bool {
        self.dialect.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UuidDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

impl UuidDefaults {
    fn is_empty(&self) -> bool {
        self.version.is_none() && self.format.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CryptoDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    algorithm: Option<String>,
    #[serde(rename = "password-env", skip_serializing_if = "Option::is_none")]
    password_env: Option<String>,
    #[serde(rename = "password-file", skip_serializing_if = "Option::is_none")]
    password_file: Option<String>,
}

impl CryptoDefaults {
    fn is_empty(&self) -> bool {
        self.algorithm.is_none() && self.password_env.is_none() && self.password_file.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TuiDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    home: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct VrunoDefaults {
    #[serde(rename = "bruno", skip_serializing)]
    legacy_bruno: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    openapi: Option<String>,
}

impl VrunoDefaults {
    fn is_empty(&self) -> bool {
        self.collection.is_none() && self.openapi.is_none()
    }
}

impl TuiDefaults {
    fn is_empty(&self) -> bool {
        self.home.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct UserConfig {
    path: PathBuf,
    data: ConfigData,
}

impl UserConfig {
    pub fn load() -> Result<Self> {
        Self::load_from(config_path()?)
    }

    pub fn load_from(path: PathBuf) -> Result<Self> {
        let mut data = match fs::read_to_string(&path) {
            Ok(contents) => toml::from_str::<ConfigData>(&contents).map_err(|error| {
                VutilsError::InvalidInput(format!(
                    "invalid config file `{}`: {error}",
                    path.display()
                ))
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ConfigData::default(),
            Err(source) => {
                return Err(VutilsError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        normalize_and_validate(&mut data)?;
        Ok(Self { path, data })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sql_dialect(&self) -> &str {
        self.data
            .sql
            .dialect
            .as_deref()
            .unwrap_or(DEFAULT_SQL_DIALECT)
    }

    pub fn uuid_version(&self) -> &str {
        self.data
            .uuid
            .version
            .as_deref()
            .unwrap_or(DEFAULT_UUID_VERSION)
    }

    pub fn uuid_format(&self) -> &str {
        self.data
            .uuid
            .format
            .as_deref()
            .unwrap_or(DEFAULT_UUID_FORMAT)
    }

    pub fn crypto_algorithm(&self) -> &str {
        self.data
            .crypto
            .algorithm
            .as_deref()
            .unwrap_or(DEFAULT_CRYPTO_ALGORITHM)
    }

    pub fn password_env(&self) -> Option<&str> {
        self.data.crypto.password_env.as_deref()
    }

    pub fn password_file(&self) -> Option<PathBuf> {
        self.data.crypto.password_file.as_deref().map(|value| {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                self.path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(path)
            }
        })
    }

    pub fn tui_home(&self) -> Vec<String> {
        self.data.tui.home.clone().unwrap_or_else(|| {
            DEFAULT_TUI_HOME
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        })
    }

    pub fn vruno_collection(&self) -> Option<PathBuf> {
        self.data
            .vruno
            .collection
            .as_deref()
            .map(Path::new)
            .map(|path| self.resolve_relative_path(path))
    }

    pub fn vruno_openapi(&self) -> Option<PathBuf> {
        self.data
            .vruno
            .openapi
            .as_deref()
            .map(Path::new)
            .map(|path| self.resolve_relative_path(path))
    }

    fn resolve_relative_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        }
    }

    pub fn get(&self, key: &str) -> Result<String> {
        match canonical_key(key)? {
            "sql.dialect" => Ok(self.sql_dialect().to_owned()),
            "uuid.version" => Ok(self.uuid_version().to_owned()),
            "uuid.format" => Ok(self.uuid_format().to_owned()),
            "crypto.algorithm" => Ok(self.crypto_algorithm().to_owned()),
            "crypto.password-env" => self.data.crypto.password_env.clone().ok_or_else(|| {
                VutilsError::InvalidInput("config key `crypto.password-env` is not set".into())
            }),
            "crypto.password-file" => self.data.crypto.password_file.clone().ok_or_else(|| {
                VutilsError::InvalidInput("config key `crypto.password-file` is not set".into())
            }),
            "tui.home" => Ok(self.tui_home().join(",")),
            "vruno.collection" => self.data.vruno.collection.clone().ok_or_else(|| {
                VutilsError::InvalidInput("config key `vruno.collection` is not set".into())
            }),
            "vruno.openapi" => self.data.vruno.openapi.clone().ok_or_else(|| {
                VutilsError::InvalidInput("config key `vruno.openapi` is not set".into())
            }),
            _ => unreachable!("canonical_key returned an unsupported key"),
        }
    }

    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut entries = vec![
            ("sql.dialect", self.sql_dialect().to_owned()),
            ("uuid.version", self.uuid_version().to_owned()),
            ("uuid.format", self.uuid_format().to_owned()),
            ("crypto.algorithm", self.crypto_algorithm().to_owned()),
        ];
        if let Some(value) = &self.data.crypto.password_env {
            entries.push(("crypto.password-env", value.clone()));
        }
        if let Some(value) = &self.data.crypto.password_file {
            entries.push(("crypto.password-file", value.clone()));
        }
        entries.push(("tui.home", self.tui_home().join(",")));
        if let Some(value) = &self.data.vruno.collection {
            entries.push(("vruno.collection", value.clone()));
        }
        if let Some(value) = &self.data.vruno.openapi {
            entries.push(("vruno.openapi", value.clone()));
        }
        entries
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let key = canonical_key(key)?;
        let value = value.trim();
        if value.is_empty() {
            return Err(VutilsError::InvalidInput(format!(
                "config value for `{key}` cannot be empty"
            )));
        }
        match key {
            "sql.dialect" => self.data.sql.dialect = Some(parse_sql_dialect(value)?.into()),
            "uuid.version" => self.data.uuid.version = Some(parse_uuid_version(value)?.into()),
            "uuid.format" => self.data.uuid.format = Some(parse_uuid_format(value)?.into()),
            "crypto.algorithm" => {
                self.data.crypto.algorithm = Some(parse_crypto_algorithm(value)?.into());
            }
            "crypto.password-env" => {
                validate_environment_name(value)?;
                self.data.crypto.password_env = Some(value.to_owned());
                self.data.crypto.password_file = None;
            }
            "crypto.password-file" => {
                self.data.crypto.password_file = Some(value.to_owned());
                self.data.crypto.password_env = None;
            }
            "tui.home" => self.data.tui.home = Some(parse_tui_home(value)?),
            "vruno.collection" => self.data.vruno.collection = Some(value.to_owned()),
            "vruno.openapi" => self.data.vruno.openapi = Some(value.to_owned()),
            _ => unreachable!("canonical_key returned an unsupported key"),
        }
        Ok(())
    }

    pub fn unset(&mut self, key: &str) -> Result<()> {
        match canonical_key(key)? {
            "sql.dialect" => self.data.sql.dialect = None,
            "uuid.version" => self.data.uuid.version = None,
            "uuid.format" => self.data.uuid.format = None,
            "crypto.algorithm" => self.data.crypto.algorithm = None,
            "crypto.password-env" => self.data.crypto.password_env = None,
            "crypto.password-file" => self.data.crypto.password_file = None,
            "tui.home" => self.data.tui.home = None,
            "vruno.collection" => self.data.vruno.collection = None,
            "vruno.openapi" => self.data.vruno.openapi = None,
            _ => unreachable!("canonical_key returned an unsupported key"),
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| VutilsError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
        let contents = toml::to_string_pretty(&self.data).map_err(|error| {
            VutilsError::Message(format!("failed to serialize config: {error}"))
        })?;
        let mut temporary = NamedTempFile::new_in(parent).map_err(|source| VutilsError::Write {
            path: self.path.clone(),
            source,
        })?;
        temporary
            .as_file_mut()
            .write_all(contents.as_bytes())
            .and_then(|()| temporary.as_file_mut().sync_all())
            .map_err(|source| VutilsError::Write {
                path: self.path.clone(),
                source,
            })?;
        set_private_permissions(temporary.as_file(), &self.path)?;
        temporary
            .persist(&self.path)
            .map_err(|error| VutilsError::Write {
                path: self.path.clone(),
                source: error.error,
            })?;
        Ok(())
    }
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os(CONFIG_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    #[cfg(target_os = "windows")]
    {
        let base = env::var_os("APPDATA").ok_or_else(missing_home_error)?;
        return Ok(PathBuf::from(base).join("vutils").join("config.toml"));
    }

    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME").ok_or_else(missing_home_error)?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("vutils")
            .join("config.toml"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(base) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(base).join("vutils").join("config.toml"));
        }
        let home = env::var_os("HOME").ok_or_else(missing_home_error)?;
        Ok(PathBuf::from(home)
            .join(".config")
            .join("vutils")
            .join("config.toml"))
    }
}

fn normalize_and_validate(data: &mut ConfigData) -> Result<()> {
    if data.version != CONFIG_VERSION {
        return Err(VutilsError::InvalidInput(format!(
            "unsupported config version {}; expected {CONFIG_VERSION}",
            data.version
        )));
    }
    if let Some(value) = &mut data.sql.dialect {
        *value = parse_sql_dialect(value)?.into();
    }
    if let Some(value) = &mut data.uuid.version {
        *value = parse_uuid_version(value)?.into();
    }
    if let Some(value) = &mut data.uuid.format {
        *value = parse_uuid_format(value)?.into();
    }
    if let Some(value) = &mut data.crypto.algorithm {
        *value = parse_crypto_algorithm(value)?.into();
    }
    if let Some(value) = &data.crypto.password_env {
        validate_environment_name(value)?;
    }
    if data.crypto.password_env.is_some() && data.crypto.password_file.is_some() {
        return Err(VutilsError::InvalidInput(
            "config keys `crypto.password-env` and `crypto.password-file` are mutually exclusive"
                .into(),
        ));
    }
    if let Some(home) = &mut data.tui.home {
        *home = normalize_tui_home(home.iter().map(String::as_str))?;
    }
    normalize_optional_text("vruno.collection", &mut data.vruno.collection)?;
    normalize_optional_text("vruno.openapi", &mut data.vruno.openapi)?;
    data.vruno.legacy_bruno = None;
    Ok(())
}

fn normalize_optional_text(key: &str, value: &mut Option<String>) -> Result<()> {
    let Some(current) = value else {
        return Ok(());
    };
    *current = current.trim().to_owned();
    if current.is_empty() {
        return Err(VutilsError::InvalidInput(format!(
            "config value for `{key}` cannot be empty"
        )));
    }
    Ok(())
}

fn canonical_key(key: &str) -> Result<&'static str> {
    match key.trim().to_ascii_lowercase().as_str() {
        "sql.dialect" | "sql-dialect" => Ok("sql.dialect"),
        "uuid.version" | "uuid-version" => Ok("uuid.version"),
        "uuid.format" | "uuid-format" => Ok("uuid.format"),
        "crypto.algorithm" | "enc.algorithm" | "crypto-algorithm" => Ok("crypto.algorithm"),
        "crypto.password-env" | "enc.password-env" => Ok("crypto.password-env"),
        "crypto.password-file" | "enc.password-file" => Ok("crypto.password-file"),
        "tui.home" | "tui-home" => Ok("tui.home"),
        "vruno.collection" | "vruno-collection" => Ok("vruno.collection"),
        "vruno.openapi" | "vruno-openapi" => Ok("vruno.openapi"),
        _ => Err(VutilsError::InvalidInput(format!(
            "unknown config key `{key}`; supported keys: sql.dialect, uuid.version, uuid.format, crypto.algorithm, crypto.password-env, crypto.password-file, tui.home, vruno.collection, vruno.openapi"
        ))),
    }
}

fn parse_tui_home(value: &str) -> Result<Vec<String>> {
    normalize_tui_home(value.split(','))
}

fn normalize_tui_home<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim().to_ascii_lowercase();
        if value.is_empty()
            || !value.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '.' | '-')
            })
        {
            return Err(VutilsError::InvalidInput(format!(
                "invalid TUI Home shortcut `{value}`; use a command id such as json.pretty or uuid"
            )));
        }
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    if normalized.is_empty() {
        return Err(VutilsError::InvalidInput(
            "config key `tui.home` requires at least one shortcut".into(),
        ));
    }
    if normalized.len() > MAX_TUI_HOME_ITEMS {
        return Err(VutilsError::InvalidInput(format!(
            "config key `tui.home` supports at most {MAX_TUI_HOME_ITEMS} shortcuts"
        )));
    }
    Ok(normalized)
}

fn parse_sql_dialect(value: &str) -> Result<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "generic" => Ok("generic"),
        "postgres" | "postgresql" => Ok("postgres"),
        "mysql" => Ok("mysql"),
        "sqlite" => Ok("sqlite"),
        "mssql" | "sqlserver" | "sql-server" => Ok("mssql"),
        _ => Err(invalid_config_value(
            "sql.dialect",
            value,
            "generic, postgres, mysql, sqlite, mssql",
        )),
    }
}

fn parse_uuid_version(value: &str) -> Result<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "v1" => Ok("v1"),
        "v2" => Ok("v2"),
        "v3" => Ok("v3"),
        "v4" => Ok("v4"),
        "v5" => Ok("v5"),
        "v6" => Ok("v6"),
        "v7" => Ok("v7"),
        "v8" => Ok("v8"),
        _ => Err(invalid_config_value(
            "uuid.version",
            value,
            "v1, v2, v3, v4, v5, v6, v7, v8",
        )),
    }
}

fn parse_uuid_format(value: &str) -> Result<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "hyphenated" | "hyphen" => Ok("hyphenated"),
        "simple" => Ok("simple"),
        "urn" => Ok("urn"),
        "braced" | "brace" => Ok("braced"),
        _ => Err(invalid_config_value(
            "uuid.format",
            value,
            "hyphenated, simple, urn, braced",
        )),
    }
}

fn parse_crypto_algorithm(value: &str) -> Result<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "xchacha20-poly1305" | "xchacha20" | "xchacha" => Ok("xchacha20-poly1305"),
        "aes-256-gcm" | "aes256-gcm" | "aes" => Ok("aes-256-gcm"),
        _ => Err(invalid_config_value(
            "crypto.algorithm",
            value,
            "xchacha20-poly1305, aes-256-gcm",
        )),
    }
}

fn validate_environment_name(value: &str) -> Result<()> {
    let mut characters = value.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic());
    if !valid_first
        || !characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(VutilsError::InvalidInput(format!(
            "invalid environment variable name `{value}`"
        )));
    }
    Ok(())
}

fn invalid_config_value(key: &str, value: &str, expected: &str) -> VutilsError {
    VutilsError::InvalidInput(format!(
        "invalid value `{value}` for config key `{key}`; expected one of: {expected}"
    ))
}

fn missing_home_error() -> VutilsError {
    VutilsError::InvalidInput(format!(
        "cannot determine the user config directory; set {CONFIG_ENV} to an explicit path"
    ))
}

#[cfg(unix)]
fn set_private_permissions(file: &fs::File, path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|source| VutilsError::Write {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(not(unix))]
fn set_private_permissions(_file: &fs::File, _path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip_normalizes_values_and_protects_password_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = UserConfig::load_from(path.clone()).unwrap();

        config.set("sql.dialect", "PostgreSQL").unwrap();
        config.set("uuid.version", "V4").unwrap();
        config.set("crypto.password-file", "password.txt").unwrap();
        config
            .set("crypto.password-env", "VUTILS_PASSWORD")
            .unwrap();
        config.save().unwrap();

        let loaded = UserConfig::load_from(path).unwrap();
        assert_eq!(loaded.sql_dialect(), "postgres");
        assert_eq!(loaded.uuid_version(), "v4");
        assert_eq!(loaded.password_env(), Some("VUTILS_PASSWORD"));
        assert_eq!(loaded.password_file(), None);
    }

    #[test]
    fn config_uses_effective_defaults_and_rejects_plaintext_password_key() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();

        assert_eq!(config.sql_dialect(), DEFAULT_SQL_DIALECT);
        assert_eq!(config.uuid_version(), DEFAULT_UUID_VERSION);
        assert_eq!(config.crypto_algorithm(), DEFAULT_CRYPTO_ALGORITHM);
        assert_eq!(config.tui_home(), DEFAULT_TUI_HOME);
        assert_eq!(config.vruno_collection(), None);
        assert_eq!(config.vruno_openapi(), None);
        assert!(config.set("crypto.password", "secret").is_err());
    }

    #[test]
    fn vruno_paths_round_trip_and_resolve_from_the_config_directory() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = UserConfig::load_from(path.clone()).unwrap();
        config.set("vruno.collection", "collections/api").unwrap();
        config.set("vruno.openapi", "specs/openapi.yaml").unwrap();
        config.save().unwrap();

        let mut loaded = UserConfig::load_from(path).unwrap();
        assert_eq!(
            loaded.vruno_collection(),
            Some(directory.path().join("collections/api"))
        );
        assert_eq!(
            loaded.vruno_openapi(),
            Some(directory.path().join("specs/openapi.yaml"))
        );
        loaded.unset("vruno.collection").unwrap();
        assert_eq!(loaded.vruno_collection(), None);
    }

    #[test]
    fn legacy_vruno_bruno_key_is_accepted_and_removed_on_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "version = 1\n\n[vruno]\nbruno = \"bru\"\ncollection = \"api\"\n",
        )
        .unwrap();

        let config = UserConfig::load_from(path.clone()).unwrap();
        config.save().unwrap();

        let saved = fs::read_to_string(path).unwrap();
        assert!(!saved.contains("bruno"));
        assert!(saved.contains("collection = \"api\""));
    }

    #[test]
    fn tui_home_round_trips_normalized_unique_shortcuts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = UserConfig::load_from(path.clone()).unwrap();
        config
            .set("tui.home", " UUID, json.pretty,uuid, SQL.FORMAT ")
            .unwrap();
        config.save().unwrap();

        let mut loaded = UserConfig::load_from(path).unwrap();
        assert_eq!(loaded.tui_home(), ["uuid", "json.pretty", "sql.format"]);
        assert_eq!(
            loaded.get("tui-home").unwrap(),
            "uuid,json.pretty,sql.format"
        );
        loaded.unset("tui.home").unwrap();
        assert_eq!(loaded.tui_home(), DEFAULT_TUI_HOME);
    }

    #[test]
    fn tui_home_rejects_empty_or_unsafe_shortcuts() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        assert!(config.set("tui.home", " , ").is_err());
        assert!(config.set("tui.home", "uuid; rm -rf").is_err());
    }

    #[test]
    fn relative_password_file_is_resolved_from_config_directory() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = UserConfig::load_from(path).unwrap();
        config
            .set("crypto.password-file", "secrets/password")
            .unwrap();

        assert_eq!(
            config.password_file(),
            Some(directory.path().join("secrets/password"))
        );
    }
}
