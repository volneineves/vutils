use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

const CONFIG_LONG_HELP: &str = r#"Manage validated defaults used by vutils commands.

Precedence: explicit command flag > persisted config > built-in default.

CONFIG KEYS:
  sql.dialect
    Values: generic, postgres, mysql, sqlite, mssql
    Default: generic
    Aliases: postgresql; sqlserver and sql-server normalize to mssql

  uuid.version
    Values: v1, v2, v3, v4, v5, v6, v7, v8
    Default: v7

  uuid.format
    Values: hyphenated, simple, urn, braced
    Default: hyphenated
    Aliases: hyphen, brace

  crypto.algorithm
    Values: xchacha20-poly1305, aes-256-gcm
    Default for new encryption: xchacha20-poly1305
    Aliases: xchacha20, xchacha, aes256-gcm, aes

  crypto.password-env
    Value: name of an environment variable containing the password
    Default: not set

  crypto.password-file
    Value: path to a file containing the password
    Default: not set; relative paths use the config file directory

Passwords are never stored directly in config.toml. password-env and password-file
are mutually exclusive; setting either one clears the other. Decryption detects the
algorithm stored in the encrypted envelope.

KEY ALIASES:
  sql-dialect, uuid-version, uuid-format, crypto-algorithm
  enc.algorithm, enc.password-env, enc.password-file

CONFIG LOCATION:
  Linux: $XDG_CONFIG_HOME/vutils/config.toml or ~/.config/vutils/config.toml
  macOS: ~/Library/Application Support/vutils/config.toml
  Override: set VUTILS_CONFIG to an explicit file path

EXAMPLES:
  vutils config path
  vutils config list
  vutils config get sql.dialect
  vutils config set sql.dialect postgres
  vutils config set uuid.version v4
  vutils config set crypto.password-env VUTILS_PASSWORD
  vutils config unset uuid.version"#;

#[derive(Debug, Parser)]
#[command(
    name = "vutils",
    version,
    about = "Offline, pipeline-friendly developer utilities"
)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[arg(long, exclusive = true, help = "Print package author information")]
    pub author: bool,
    #[arg(short, long, global = true, help = "Write output to a file")]
    pub output: Option<PathBuf>,
    #[arg(long, global = true, help = "Replace the input file atomically")]
    pub in_place: bool,
    #[arg(short, long, global = true, help = "Overwrite an existing output file")]
    pub force: bool,
    #[arg(long, global = true, help = "Also copy UTF-8 output to the clipboard")]
    pub copy: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Generate UUIDs v1 through v8")]
    Uuid(UuidArgs),
    #[command(subcommand, about = "Generate other identifiers")]
    Id(IdCommand),
    #[command(subcommand, about = "Generate local test data")]
    Gen(GenCommand),
    #[command(about = "Generate and validate Brazilian development fixtures")]
    Br(BrArgs),
    #[command(
        subcommand,
        about = "Manage persistent per-user defaults",
        long_about = CONFIG_LONG_HELP
    )]
    Config(ConfigCommand),
    #[command(subcommand, about = "Encode and decode Base64 data")]
    Base64(Base64Command),
    #[command(
        subcommand,
        name = "binary",
        alias = "bin",
        about = "Encode bytes as bits or decode bits back to bytes"
    )]
    Binary(BinaryCommand),
    #[command(subcommand, about = "Encode and decode hexadecimal data")]
    Hex(HexCommand),
    #[command(subcommand, about = "Encode, decode, and inspect URLs")]
    Url(UrlCommand),
    #[command(subcommand, about = "Encode and decode HTML entities")]
    Html(TextCodecCommand),
    #[command(subcommand, about = "Compress and decompress GZip data")]
    Gzip(GzipCommand),
    #[command(about = "Encrypt data with a password using authenticated encryption")]
    Enc(EncryptionArgs),
    #[command(about = "Decrypt a vutils encrypted envelope with its password")]
    Dec(DecryptionArgs),
    #[command(subcommand, about = "Format, query, validate, and convert JSON")]
    Json(JsonCommand),
    #[command(subcommand, about = "Format, validate, split, join, and convert YAML")]
    Yaml(YamlCommand),
    #[command(subcommand, about = "Validate and convert CSV")]
    Csv(CsvCommand),
    #[command(subcommand, about = "Format, validate, and convert TOML")]
    Toml(TomlCommand),
    #[command(subcommand, about = "Format and validate XML")]
    Xml(XmlCommand),
    #[command(subcommand, about = "Parse, validate, sort, and compare dotenv files")]
    Dotenv(DotenvCommand),
    #[command(
        subcommand,
        about = "Generate strongly typed models from JSON examples"
    )]
    Code(CodeCommand),
    #[command(subcommand, about = "Format static cURL commands")]
    Curl(CurlCommand),
    #[command(subcommand, about = "Format SQL")]
    Sql(SqlCommand),
    #[command(subcommand, about = "Transform and compare text")]
    Text(TextCommand),
    #[command(subcommand, about = "Test and replace regular expressions")]
    Regex(RegexCommand),
    #[command(
        subcommand,
        name = "string",
        about = "Escape and unescape string literals"
    )]
    StringValue(StringCommand),
    #[command(subcommand, about = "Convert integers between common bases")]
    Number(NumberCommand),
    #[command(subcommand, about = "Format and parse byte sizes")]
    Bytes(BytesCommand),
    #[command(subcommand, about = "Calculate cryptographic hashes")]
    Hash(HashCommand),
    #[command(about = "Calculate a keyed message authentication code")]
    Hmac(HmacArgs),
    #[command(
        subcommand,
        name = "password-hash",
        about = "Hash and verify passwords"
    )]
    PasswordHash(PasswordHashCommand),
    #[command(subcommand, about = "Generate and verify offline TOTP codes")]
    Totp(TotpCommand),
    #[command(subcommand, about = "Decode JWTs without verifying signatures")]
    Jwt(JwtCommand),
    #[command(subcommand, about = "Calculate file or directory checksums")]
    Checksum(ChecksumCommand),
    #[command(subcommand, about = "Inspect PEM containers")]
    Pem(PemCommand),
    #[command(subcommand, about = "Inspect local PEM-encoded X.509 certificates")]
    Cert(CertCommand),
    #[command(subcommand, about = "Convert timestamps, durations, and time zones")]
    Time(TimeCommand),
    #[command(subcommand, about = "Explain and calculate cron schedules")]
    Cron(CronCommand),
    #[command(subcommand, about = "Encode and decode Unix permission bits")]
    Chmod(ChmodCommand),
    #[command(subcommand, about = "Normalize and relativize paths")]
    Path(PathCommand),
    #[command(subcommand, about = "Compare and evaluate semantic versions")]
    Semver(SemverCommand),
    #[command(subcommand, about = "Inspect IP addresses and CIDR ranges")]
    Ip(IpCommand),
    #[command(subcommand, about = "Render QR codes locally")]
    Qr(QrCommand),
    #[command(about = "Generate shell completions")]
    Completion {
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    #[command(about = "Generate the vutils manual page")]
    Man,
    #[command(about = "Look up a MIME type by file extension")]
    Mime { extension: String },
}

#[derive(Debug, Args, Clone, Default)]
pub struct InputOptions {
    #[arg(value_name = "VALUE", conflicts_with = "input")]
    pub value: Option<String>,
    #[arg(short, long, value_name = "PATH", conflicts_with = "value")]
    pub input: Option<PathBuf>,
}

#[derive(Debug, Args, Clone, Default)]
pub struct SecretOptions {
    #[arg(long, conflicts_with_all = ["secret_file", "secret_env"], help = "Secret value (may be recorded in shell history)")]
    pub secret: Option<String>,
    #[arg(long, value_name = "PATH", conflicts_with_all = ["secret", "secret_env"])]
    pub secret_file: Option<PathBuf>,
    #[arg(long, value_name = "NAME", conflicts_with_all = ["secret", "secret_file"])]
    pub secret_env: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
pub struct PasswordOptions {
    #[arg(
        long,
        value_name = "PASSWORD",
        conflicts_with_all = ["passwd_file", "passwd_env"],
        help = "Password value (may be recorded in shell history)"
    )]
    pub passwd: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with_all = ["passwd", "passwd_env"],
        help = "Read the password from a local file"
    )]
    pub passwd_file: Option<PathBuf>,
    #[arg(
        long,
        value_name = "NAME",
        conflicts_with_all = ["passwd", "passwd_file"],
        help = "Read the password from an environment variable"
    )]
    pub passwd_env: Option<String>,
}

#[derive(Debug, Args)]
pub struct EncryptionArgs {
    #[arg(
        long = "alg",
        value_enum,
        help = "Encryption algorithm; SHA algorithms are hashes and are not reversible (config: crypto.algorithm; built-in: xchacha20-poly1305)"
    )]
    pub algorithm: Option<EncryptionAlgorithmArg>,
    #[command(flatten)]
    pub password: PasswordOptions,
    #[command(flatten)]
    pub input: InputOptions,
}

#[derive(Debug, Args)]
pub struct DecryptionArgs {
    #[arg(
        long = "alg",
        value_enum,
        help = "Require this algorithm; otherwise use the algorithm stored in the envelope"
    )]
    pub algorithm: Option<EncryptionAlgorithmArg>,
    #[command(flatten)]
    pub password: PasswordOptions,
    #[command(flatten)]
    pub input: InputOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EncryptionAlgorithmArg {
    #[value(name = "aes-256-gcm", alias = "aes256-gcm", alias = "aes")]
    Aes256Gcm,
    #[value(name = "xchacha20-poly1305", alias = "xchacha20", alias = "xchacha")]
    XChaCha20Poly1305,
}

#[derive(Debug, Args)]
pub struct UuidArgs {
    #[arg(
        short,
        long,
        value_enum,
        help = "UUID version (config: uuid.version; built-in: v7)"
    )]
    pub version: Option<UuidVersionArg>,
    #[arg(short, long, default_value_t = 1)]
    pub count: u32,
    #[arg(
        long,
        value_enum,
        help = "Output format (config: uuid.format; built-in: hyphenated)"
    )]
    pub format: Option<UuidFormatArg>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub node_id: Option<String>,
    #[arg(long)]
    pub custom_bytes: Option<String>,
    #[arg(long, value_enum)]
    pub domain: Option<DceDomainArg>,
    #[arg(long)]
    pub local_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum UuidVersionArg {
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum UuidFormatArg {
    Hyphenated,
    Simple,
    Urn,
    Braced,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DceDomainArg {
    Person,
    Group,
    Organization,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Print the active config file path")]
    Path,
    #[command(about = "List effective defaults")]
    List,
    #[command(about = "Print one effective config value")]
    Get { key: String },
    #[command(about = "Persist a validated config value")]
    Set { key: String, value: String },
    #[command(about = "Remove a persisted value and restore its built-in default")]
    Unset { key: String },
}

#[derive(Debug, Subcommand)]
pub enum IdCommand {
    Ulid {
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
    Nanoid {
        #[arg(short, long, default_value_t = 21)]
        length: usize,
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
    Objectid {
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
}

#[derive(Debug, Subcommand)]
pub enum GenCommand {
    Password {
        #[arg(short, long, default_value_t = 20)]
        length: usize,
        #[arg(short, long, default_value_t = 1)]
        count: u32,
        #[arg(long)]
        no_symbols: bool,
        #[arg(long)]
        exclude_ambiguous: bool,
    },
    Token {
        #[arg(short, long, default_value_t = 32)]
        length: usize,
        #[arg(short, long, default_value_t = 1)]
        count: u32,
        #[arg(long)]
        alphabet: Option<String>,
    },
    Email {
        #[arg(long, default_value = "example.com")]
        domain: String,
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
    Name {
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
    Lorem {
        #[arg(short, long, default_value_t = 24)]
        words: usize,
    },
}

#[derive(Debug, Args)]
pub struct BrArgs {
    #[command(subcommand)]
    pub command: Option<BrCommand>,
}

#[derive(Debug, Subcommand)]
pub enum BrCommand {
    #[command(about = "Generate or validate Brazilian CPF values")]
    Cpf(BrDocumentArgs),
    #[command(about = "Generate or validate Brazilian CNPJ values")]
    Cnpj(BrDocumentArgs),
    #[command(about = "Generate synthetic Brazilian CEP values")]
    Cep(BrFixtureArgs),
    #[command(about = "Generate synthetic Brazilian mobile phone values")]
    Phone(BrFixtureArgs),
    #[command(about = "Generate synthetic Brazilian PIX keys")]
    Pix {
        #[arg(long, default_value = "random")]
        kind: String,
        #[arg(short, long, default_value_t = 1)]
        count: u32,
    },
}

#[derive(Debug, Args)]
pub struct BrDocumentArgs {
    #[arg(long, conflicts_with_all = ["count", "formatted"])]
    pub validate: Option<String>,
    #[arg(short, long, default_value_t = 1)]
    pub count: u32,
    #[arg(long)]
    pub formatted: bool,
}

#[derive(Debug, Args)]
pub struct BrFixtureArgs {
    #[arg(short, long, default_value_t = 1)]
    pub count: u32,
    #[arg(long)]
    pub formatted: bool,
}

#[derive(Debug, Subcommand)]
pub enum Base64Command {
    Encode {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        url_safe: bool,
        #[arg(long)]
        no_padding: bool,
    },
    Decode {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        url_safe: bool,
        #[arg(long)]
        no_padding: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum BinaryCommand {
    Encode {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long, help = "Separate each encoded byte with a space")]
        spaced: bool,
    },
    Decode(InputOptions),
}

#[derive(Debug, Subcommand)]
pub enum TextCodecCommand {
    Encode(InputOptions),
    Decode(InputOptions),
}

#[derive(Debug, Subcommand)]
pub enum HexCommand {
    Encode {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        uppercase: bool,
    },
    Decode(InputOptions),
}

#[derive(Debug, Subcommand)]
pub enum UrlCommand {
    Encode {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        form: bool,
    },
    Decode {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        form: bool,
    },
    Inspect(InputOptions),
}

#[derive(Debug, Subcommand)]
pub enum GzipCommand {
    Compress {
        #[command(flatten)]
        input: InputOptions,
        #[arg(short, long, default_value_t = 6)]
        level: u32,
    },
    Decompress(InputOptions),
}

#[derive(Debug, Subcommand)]
pub enum JsonCommand {
    Pretty(InputOptions),
    Minify(InputOptions),
    Validate(InputOptions),
    Escape(InputOptions),
    Unescape(InputOptions),
    SortKeys(InputOptions),
    Flatten(InputOptions),
    Unflatten(InputOptions),
    Path {
        expression: String,
        #[command(flatten)]
        input: InputOptions,
    },
    Diff(DiffArgs),
    ToYaml(InputOptions),
    ToCsv {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        stringify_nested: bool,
    },
    ToToml(InputOptions),
    SchemaValidate {
        #[arg(long)]
        schema: PathBuf,
        #[command(flatten)]
        input: InputOptions,
    },
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    #[arg(long, conflicts_with = "left_file")]
    pub left: Option<String>,
    #[arg(long, conflicts_with = "left")]
    pub left_file: Option<PathBuf>,
    #[arg(long, conflicts_with = "right_file")]
    pub right: Option<String>,
    #[arg(long, conflicts_with = "right")]
    pub right_file: Option<PathBuf>,
    #[arg(long)]
    pub patch: bool,
}

#[derive(Debug, Subcommand)]
pub enum YamlCommand {
    Pretty(InputOptions),
    Validate(InputOptions),
    ToJson(InputOptions),
    Split {
        #[command(flatten)]
        input: InputOptions,
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
    Join {
        files: Vec<PathBuf>,
    },
}
#[derive(Debug, Subcommand)]
pub enum CsvCommand {
    Validate(InputOptions),
    ToJson(InputOptions),
}
#[derive(Debug, Subcommand)]
pub enum TomlCommand {
    Pretty(InputOptions),
    Validate(InputOptions),
    ToJson(InputOptions),
}
#[derive(Debug, Subcommand)]
pub enum XmlCommand {
    Pretty(InputOptions),
    Validate(InputOptions),
}
#[derive(Debug, Subcommand)]
pub enum DotenvCommand {
    Parse(InputOptions),
    Validate(InputOptions),
    Sort(InputOptions),
    Diff {
        #[command(flatten)]
        diff: DiffArgs,
        #[arg(long)]
        show_values: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum CodeCommand {
    Types {
        #[arg(short, long, value_enum)]
        lang: LanguageArg,
        #[arg(short, long, default_value = "Root")]
        name: String,
        #[command(flatten)]
        input: InputOptions,
    },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LanguageArg {
    Rust,
    Kotlin,
    Csharp,
    Typescript,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ShellArg {
    Posix,
    Powershell,
}

#[derive(Debug, Subcommand)]
pub enum CurlCommand {
    Format {
        #[arg(long, value_enum, default_value_t = ShellArg::Posix)]
        shell: ShellArg,
        #[command(flatten)]
        input: InputOptions,
    },
}

#[derive(Debug, Subcommand)]
pub enum SqlCommand {
    Format(SqlFormatArgs),
}

#[derive(Debug, Args)]
pub struct SqlCommonArgs {
    #[arg(
        long,
        value_enum,
        help = "SQL dialect (config: sql.dialect; built-in: generic)"
    )]
    pub dialect: Option<SqlDialectArg>,
    #[command(flatten)]
    pub input: InputOptions,
}
#[derive(Debug, Args)]
pub struct SqlFormatArgs {
    #[command(flatten)]
    pub common: SqlCommonArgs,
    #[arg(long, value_enum, default_value_t = KeywordCaseArg::Upper)]
    pub keyword_case: KeywordCaseArg,
    #[arg(long, default_value_t = 2)]
    pub indent: u8,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SqlDialectArg {
    Generic,
    Postgres,
    Mysql,
    Sqlite,
    Mssql,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KeywordCaseArg {
    Upper,
    Lower,
    Preserve,
}

#[derive(Debug, Subcommand)]
pub enum TextCommand {
    #[command(about = "Convert text to camelCase, PascalCase, snake_case, and other styles")]
    Case {
        #[arg(value_enum, help = "Target case style")]
        style: CaseArg,
        #[command(flatten)]
        input: InputOptions,
    },
    Slug(InputOptions),
    Trim(InputOptions),
    SortLines {
        #[arg(long)]
        unique: bool,
        #[arg(long)]
        descending: bool,
        #[command(flatten)]
        input: InputOptions,
    },
    UniqueLines(InputOptions),
    NormalizeEol {
        #[arg(long)]
        crlf: bool,
        #[command(flatten)]
        input: InputOptions,
    },
    Diff(DiffArgs),
    Unicode(InputOptions),
    OnlyDigits(InputOptions),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CaseArg {
    #[value(
        alias = "camel-case",
        alias = "camelcase",
        alias = "camelCase",
        alias = "camel_case"
    )]
    Camel,
    #[value(
        alias = "pascal-case",
        alias = "pascalcase",
        alias = "PascalCase",
        alias = "pascal_case"
    )]
    Pascal,
    #[value(alias = "snake-case", alias = "snakecase", alias = "snake_case")]
    Snake,
    #[value(
        alias = "kebab-case",
        alias = "kebabcase",
        alias = "kebabCase",
        alias = "kebab_case"
    )]
    Kebab,
    #[value(
        alias = "constant-case",
        alias = "screaming-snake",
        alias = "CONSTANT_CASE",
        alias = "constant_case"
    )]
    Constant,
    #[value(
        alias = "title-case",
        alias = "titlecase",
        alias = "TitleCase",
        alias = "title_case"
    )]
    Title,
}

#[derive(Debug, Subcommand)]
pub enum RegexCommand {
    Test {
        pattern: String,
        #[command(flatten)]
        input: InputOptions,
    },
    Replace {
        pattern: String,
        replacement: String,
        #[arg(long)]
        first_only: bool,
        #[command(flatten)]
        input: InputOptions,
    },
}
#[derive(Debug, Subcommand)]
pub enum StringCommand {
    Escape {
        #[arg(long, value_enum)]
        language: EscapeLanguageArg,
        #[command(flatten)]
        input: InputOptions,
    },
    Unescape {
        #[arg(long, value_enum)]
        language: EscapeLanguageArg,
        #[command(flatten)]
        input: InputOptions,
    },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EscapeLanguageArg {
    Json,
    Rust,
    Kotlin,
    Java,
    Csharp,
    Javascript,
    Typescript,
    Python,
    Sql,
    PosixShell,
}
#[derive(Debug, Subcommand)]
pub enum NumberCommand {
    Convert {
        value: String,
        #[arg(long)]
        from: u32,
        #[arg(long)]
        to: u32,
    },
}
#[derive(Debug, Subcommand)]
pub enum BytesCommand {
    Format {
        value: u128,
        #[arg(long)]
        iec: bool,
        #[arg(long, default_value_t = 2)]
        precision: usize,
    },
    Parse {
        value: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum HashCommand {
    Sha256(InputOptions),
    Sha512(InputOptions),
}
#[derive(Debug, Args)]
pub struct HmacArgs {
    #[arg(long, value_enum, default_value_t = HashAlgorithmArg::Sha256)]
    pub algorithm: HashAlgorithmArg,
    #[command(flatten)]
    pub secret: SecretOptions,
    #[command(flatten)]
    pub input: InputOptions,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum HashAlgorithmArg {
    Sha256,
    Sha512,
}

#[derive(Debug, Subcommand)]
pub enum PasswordHashCommand {
    Argon2Hash {
        #[command(flatten)]
        secret: SecretOptions,
    },
    Argon2Verify {
        encoded: String,
        #[command(flatten)]
        secret: SecretOptions,
    },
    BcryptHash {
        #[arg(long, default_value_t = 12)]
        cost: u32,
        #[command(flatten)]
        secret: SecretOptions,
    },
    BcryptVerify {
        encoded: String,
        #[command(flatten)]
        secret: SecretOptions,
    },
}

#[derive(Debug, Subcommand)]
pub enum TotpCommand {
    GenerateSecret {
        #[arg(long, default_value_t = 20)]
        bytes: usize,
    },
    Code {
        #[command(flatten)]
        secret: SecretOptions,
        #[arg(long, value_enum, default_value_t = TotpAlgorithmArg::Sha1)]
        algorithm: TotpAlgorithmArg,
        #[arg(long, default_value_t = 6)]
        digits: u32,
        #[arg(long, default_value_t = 30)]
        period: u64,
        #[arg(long)]
        timestamp: Option<u64>,
    },
    Verify {
        code: String,
        #[command(flatten)]
        secret: SecretOptions,
        #[arg(long, value_enum, default_value_t = TotpAlgorithmArg::Sha1)]
        algorithm: TotpAlgorithmArg,
        #[arg(long, default_value_t = 6)]
        digits: u32,
        #[arg(long, default_value_t = 30)]
        period: u64,
        #[arg(long)]
        timestamp: Option<u64>,
        #[arg(long, default_value_t = 1)]
        window: u64,
    },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TotpAlgorithmArg {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug, Subcommand)]
pub enum JwtCommand {
    Decode(InputOptions),
}
#[derive(Debug, Subcommand)]
pub enum ChecksumCommand {
    File {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = HashAlgorithmArg::Sha256)]
        algorithm: HashAlgorithmArg,
    },
    Directory {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = HashAlgorithmArg::Sha256)]
        algorithm: HashAlgorithmArg,
        #[arg(long)]
        follow_links: bool,
    },
}
#[derive(Debug, Subcommand)]
pub enum PemCommand {
    Inspect(InputOptions),
}
#[derive(Debug, Subcommand)]
pub enum CertCommand {
    Inspect(InputOptions),
}

#[derive(Debug, Subcommand)]
pub enum TimeCommand {
    #[command(about = "Show the current local time, or a Unix timestamp with --unix")]
    Now {
        #[arg(long, help = "Return a Unix timestamp instead of RFC 3339")]
        unix: bool,
        #[arg(
            long,
            value_enum,
            requires = "unix",
            help = "Unix timestamp unit (defaults to seconds)"
        )]
        unit: Option<TimeUnitArg>,
        #[arg(
            long,
            conflicts_with = "unix",
            help = "Format the time in UTC instead of the machine's local timezone"
        )]
        utc: bool,
    },
    #[command(about = "Convert a Unix timestamp to local RFC 3339 time")]
    ToIso {
        value: i64,
        #[arg(long, value_enum, default_value_t = TimeUnitArg::Seconds)]
        unit: TimeUnitArg,
        #[arg(
            long,
            help = "Format the result in UTC instead of the machine's local timezone"
        )]
        utc: bool,
    },
    #[command(about = "Convert an RFC 3339 time with an explicit offset to Unix time")]
    ToUnix {
        value: String,
        #[arg(long, value_enum, default_value_t = TimeUnitArg::Seconds)]
        unit: TimeUnitArg,
    },
    #[command(about = "Parse a human-readable duration into milliseconds")]
    Duration { value: String },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TimeUnitArg {
    Seconds,
    Milliseconds,
}
#[derive(Debug, Subcommand)]
pub enum CronCommand {
    #[command(about = "List upcoming occurrences in the machine's local timezone")]
    Next {
        expression: String,
        #[arg(short, long, default_value_t = 5)]
        count: usize,
        #[arg(
            long,
            help = "Return occurrences in UTC instead of the machine's local timezone"
        )]
        utc: bool,
    },
    #[command(about = "Explain a schedule and list upcoming local occurrences")]
    Explain {
        expression: String,
        #[arg(short, long, default_value_t = 5)]
        count: usize,
        #[arg(
            long,
            help = "Return occurrences in UTC instead of the machine's local timezone"
        )]
        utc: bool,
    },
}
#[derive(Debug, Subcommand)]
pub enum ChmodCommand {
    Encode { value: String },
    Decode { value: String },
}
#[derive(Debug, Subcommand)]
pub enum PathCommand {
    Normalize { value: PathBuf },
    Relative { from: PathBuf, to: PathBuf },
}
#[derive(Debug, Subcommand)]
pub enum SemverCommand {
    Compare { left: String, right: String },
    Sort { versions: Vec<String> },
    Bump { value: String, kind: String },
}
#[derive(Debug, Subcommand)]
pub enum IpCommand {
    Cidr { value: String },
}
#[derive(Debug, Subcommand)]
pub enum QrCommand {
    Generate {
        #[arg(long, value_enum, default_value_t = QrFormatArg::Terminal)]
        format: QrFormatArg,
        #[arg(
            long,
            default_value_t = 256,
            help = "Image size in pixels (SVG/PNG only)"
        )]
        size: u32,
        #[command(flatten)]
        input: InputOptions,
    },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum QrFormatArg {
    Terminal,
    Svg,
    Png,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory as _, Parser as _};

    use super::{CaseArg, Cli, Command, TextCommand};

    #[test]
    fn command_tree_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn author_flag_does_not_require_a_subcommand() {
        let cli = Cli::try_parse_from(["vutils", "--author"]).unwrap();

        assert!(cli.author);
        assert!(cli.command.is_none());
    }

    #[test]
    fn case_styles_accept_code_spelling_aliases() {
        let examples = [
            ("camelCase", CaseArg::Camel),
            ("PascalCase", CaseArg::Pascal),
            ("snake_case", CaseArg::Snake),
            ("kebabCase", CaseArg::Kebab),
            ("CONSTANT_CASE", CaseArg::Constant),
            ("TitleCase", CaseArg::Title),
        ];

        for (alias, expected) in examples {
            let cli =
                Cli::try_parse_from(["vutils", "text", "case", alias, "hello world"]).unwrap();
            let Some(Command::Text(TextCommand::Case { style, .. })) = cli.command else {
                panic!("expected text case command");
            };
            assert_eq!(style, expected);
        }
    }
}
