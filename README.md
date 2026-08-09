# vutils

`vutils` is a fast, offline, pipeline-friendly developer toolkit written in Rust. It keeps routine transformations on your machine: no HTTP client, database driver, telemetry, remote lookup, or shell execution is included.

## Install without Rust or Cargo

Download the artifact for your system from [GitHub Releases](https://github.com/volneineves/vutils/releases). Every release includes `SHA256SUMS`; verify the downloaded file before installing it.

### Debian and Ubuntu

```bash
sudo apt install ./vutils_<version>_amd64.deb
vutils --version
```

### Fedora, RHEL, Rocky Linux, and openSUSE

```bash
sudo dnf install ./vutils-<version>-1.x86_64.rpm
vutils --version
```

On systems without `dnf`, use the native RPM package manager, such as `sudo rpm -U ./vutils-<version>-1.x86_64.rpm`.

### Portable Linux binary

```bash
tar -xzf vutils-linux-x86_64.tar.gz
sudo install -m 0755 vutils /usr/local/bin/vutils
vutils --version
```

The raw `vutils-linux-x86_64` asset is also available; make it executable with `chmod +x` before placing it on `PATH`.

### macOS

Choose `vutils-macos-aarch64.tar.gz` for Apple Silicon or `vutils-macos-x86_64.tar.gz` for Intel Macs.

```bash
tar -xzf vutils-macos-aarch64.tar.gz
sudo install -m 0755 vutils /usr/local/bin/vutils
vutils --version
```

The binaries are not currently signed with an Apple Developer certificate, so macOS may request explicit approval before the first execution.

### Windows

Download `vutils-windows-x86_64.zip`, extract `vutils.exe`, and place it in a directory included in the user `PATH`. A PowerShell-only user installation can use:

```powershell
$install = "$env:LOCALAPPDATA\Programs\vutils"
New-Item -ItemType Directory -Force $install | Out-Null
Copy-Item .\vutils.exe "$install\vutils.exe"
$path = [Environment]::GetEnvironmentVariable("Path", "User")
if (($path -split ';') -notcontains $install) {
  [Environment]::SetEnvironmentVariable("Path", "$path;$install", "User")
}
```

Open a new terminal and run `vutils --version`.

### Verify a download

```bash
sha256sum -c SHA256SUMS --ignore-missing
```

On Windows, compare `Get-FileHash .\vutils-windows-x86_64.zip -Algorithm SHA256` with the corresponding entry in `SHA256SUMS`.

Rust 1.88 or newer is required only when building from source. Installed binaries run without Rust, Cargo, or a network connection.

## Input and output

Transformations accept a positional value, `--input <path>`, or stdin. Output is written to stdout by default.

```bash
printf '%s' '{"name":"Ana"}' | vutils json pretty
vutils json to-yaml --input payload.json --output payload.yaml
vutils json pretty --input payload.json --in-place
vutils base64 decode 'AAEC' --output bytes.bin
```

`--in-place` writes a temporary file beside the source and replaces the original only after successful transformation. `--output` refuses to overwrite an existing file unless `--force` is present. `--copy` additionally copies UTF-8 output to the local clipboard.

## Command groups

| Group | Examples |
| --- | --- |
| Identifiers | `uuid`, `id ulid`, `id nanoid`, `id objectid` |
| Test data | `gen password`, `gen token`, `gen cpf`, `gen cnpj`, `gen phone`, `gen pix`, `gen lorem` |
| Structured data | `json`, `yaml`, `csv`, `toml`, `xml`, `dotenv` |
| Codecs | `base64`, `hex`, `url`, `html`, `gzip`, `string escape` |
| Text | `text case`, `text slug`, `text diff`, `regex`, `number`, `bytes` |
| Code generation | `code types --lang rust|kotlin|csharp|typescript` |
| HTTP authoring | `http build`, `http render`, `http from-har`, `curl parse|format|explain|convert` |
| SQL authoring | `sql format|minify|validate|inspect|insert|update|placeholders|quote-*` |
| Security | `hash`, `hmac`, `password-hash`, `totp`, `jwt`, `checksum`, `pem`, `cert` |
| Offline calculators | `time`, `cron`, `chmod`, `path`, `semver`, `ip cidr`, `mime`, `qr` |

Use `vutils help <command>` or `vutils <command> --help` for the complete options.

## UUIDs

```bash
vutils uuid                         # v7 by default
vutils uuid --version v4 --count 5
vutils uuid --version v5 --namespace url --name https://example.com
vutils uuid --version v2 --domain person --local-id 1000
vutils uuid --version v8 --custom-bytes 00112233445566778899aabbccddeeff
```

UUID v2 is a legacy DCE Security format. `vutils` produces best-effort fixtures but cannot provide registry-backed global uniqueness. With a fixed node ID, a single batch is limited to 64 values.

## JSON, YAML, CSV, TOML, and XML

```bash
vutils json sort-keys '{"z":1,"a":2}'
vutils json path '$.users[0].name' --input response.json
vutils json schema-validate --schema schema.json --input value.json
vutils json to-csv --input rows.json
vutils yaml to-json --input config.yaml
vutils toml pretty --input Cargo.toml
vutils xml validate --input document.xml
```

YAML, XML, and dotenv formatting is semantic and may not preserve comments or original styling. YAML-to-JSON accepts one JSON-compatible document. JSON-to-CSV expects flat objects unless `--stringify-nested` is specified.

## Local code generation

```bash
vutils code types --lang rust --name User '{"id":1,"name":"Ana"}'
vutils code types --lang kotlin --name ApiResponse --input response.json
```

Generated types are inferred from examples, not schemas. Missing fields become optional; ambiguous values use the target language's safe dynamic type.

## Text case conversion

```bash
vutils text case camel 'customer account'       # customerAccount
vutils text case pascal 'customer account'      # CustomerAccount
vutils text case snake 'customerAccount'        # customer_account
vutils text case kebab 'customerAccount'        # customer-account
vutils text case constant 'customer account'    # CUSTOMER_ACCOUNT
vutils text case title 'customer account'       # Customer Account
```

The descriptive aliases `camel-case`, `camelcase`, `pascal-case`, `pascalcase`, `snake-case`, and `snakecase` are also accepted.

## cURL and HTTP authoring

No request is sent. The commands only parse or render text.

```bash
vutils http build https://example.com/users --method POST \
  --header 'Authorization: Bearer $TOKEN' \
  --json '{"name":"Ana"}' \
  --render curl

vutils curl format "curl -XPOST -H 'Accept: application/json' https://example.com"
vutils curl convert --to fetch --input request.curl
vutils curl explain --input request.curl       # secrets redacted
```

The cURL parser accepts one static POSIX command and rejects operators, substitutions, redirections, unsupported flags, and non-HTTP URLs. It never invokes a shell.

## SQL authoring

No database connection is made.

```bash
vutils sql format --dialect postgres 'select id,name from users where id=$1'
vutils sql insert users --dialect postgres '{"name":"Ana","active":true}'
vutils sql update users --dialect postgres --data '{"name":"Ana"}' --where-data '{"id":42}'
```

Insert and update output is parameterized JSON by default:

```json
{
  "sql": "INSERT INTO \"users\" (\"name\") VALUES ($1);",
  "params": ["Ana"]
}
```

`--literal` explicitly requests standalone SQL with dialect-aware quoting. Update refuses an empty `where` object.

## Secrets

HMAC, TOTP, and password hashing prefer stdin, `--secret-file`, or `--secret-env`. `--secret` is convenient but may be visible in shell history and process listings.

```bash
printf '%s' 'password' | vutils password-hash argon2-hash
vutils hmac --secret-env API_SECRET --input payload.bin
vutils totp code --secret-file totp.secret
```

JWT decoding never verifies a signature and emits a warning on stderr.

## Time and cron

Formatted dates use the machine's local timezone by default. Pass `--utc` when UTC output is required. Unix timestamps are timezone-independent.

```bash
vutils time now
vutils time now --utc
vutils time now --unix
vutils time now --unix --unit milliseconds
vutils time to-iso 1700000000
vutils time to-iso 1700000000 --utc
vutils cron next '0 0 9 * * MON-FRI *'
vutils cron next '0 0 9 * * MON-FRI *' --utc
```

## Exit codes

- `0`: successful operation or positive validation.
- `1`: invalid data, negative validation, or operational failure.
- `2`: invalid CLI usage reported by Clap.

## Development

Building from source is optional and intended for contributors:

```bash
cargo install --path .
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
cargo package
```

## Automated releases

Every push to `main` creates a uniquely versioned GitHub prerelease named `v<crate-version>-build.<run>.<attempt>`. Pushing a `v*` tag creates a stable release. Both paths build and attach Linux, Windows, and macOS binaries, Debian and RPM packages, plus `SHA256SUMS`.

## License

Licensed under either Apache License 2.0 or MIT, at your option.
