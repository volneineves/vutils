# vutils

`vutils` is a fast, offline, pipeline-friendly developer toolkit written in Rust. It keeps routine transformations on your machine: no HTTP client, database driver, telemetry, remote lookup, or shell execution is included.

## Install without Rust or Cargo

Download the artifact for your system from [GitHub Releases](https://github.com/volneineves/vutils/releases). Every release includes `SHA256SUMS`; verify the downloaded file before installing it.

### Debian and Ubuntu

```bash
curl -fL -o vutils-latest-amd64.deb https://github.com/volneineves/vutils/releases/latest/download/vutils-latest-amd64.deb
sudo apt install ./vutils-latest-amd64.deb
rm -f ./vutils-latest-amd64.deb
vutils --version
```

The URL always follows the newest stable GitHub release and ignores prereleases created from `main`.

### Fedora, RHEL, Rocky Linux, and openSUSE

```bash
curl -fL -o vutils-latest-x86_64.rpm https://github.com/volneineves/vutils/releases/latest/download/vutils-latest-x86_64.rpm
sudo dnf install ./vutils-latest-x86_64.rpm
rm -f ./vutils-latest-x86_64.rpm
vutils --version
```

On systems without `dnf`, replace the install line with `sudo rpm -U ./vutils-latest-x86_64.rpm`.

### Portable Linux binary

```bash
curl -fL -o vutils-linux-x86_64.tar.gz https://github.com/volneineves/vutils/releases/latest/download/vutils-linux-x86_64.tar.gz
tar -xzf vutils-linux-x86_64.tar.gz ./vutils
sudo install -m 0755 vutils /usr/local/bin/vutils
rm -f ./vutils-linux-x86_64.tar.gz ./vutils
vutils --version
```

The raw `vutils-linux-x86_64` asset is also available; make it executable with `chmod +x` before placing it on `PATH`.

### macOS

Apple Silicon:

```bash
curl -fL -o vutils-macos-aarch64.tar.gz https://github.com/volneineves/vutils/releases/latest/download/vutils-macos-aarch64.tar.gz
tar -xzf vutils-macos-aarch64.tar.gz ./vutils
sudo install -m 0755 vutils /usr/local/bin/vutils
rm -f ./vutils-macos-aarch64.tar.gz ./vutils
vutils --version
```

Intel Mac:

```bash
curl -fL -o vutils-macos-x86_64.tar.gz https://github.com/volneineves/vutils/releases/latest/download/vutils-macos-x86_64.tar.gz
tar -xzf vutils-macos-x86_64.tar.gz ./vutils
sudo install -m 0755 vutils /usr/local/bin/vutils
rm -f ./vutils-macos-x86_64.tar.gz ./vutils
vutils --version
```

The binaries are not currently signed with an Apple Developer certificate, so macOS may request explicit approval before the first execution.

### Verify a download

For an asset downloaded manually into the current directory, stream the checksum list instead of saving another file:

```bash
curl -fsSL https://github.com/volneineves/vutils/releases/latest/download/SHA256SUMS | sha256sum -c - --ignore-missing
```

Rust 1.88 or newer is required only when building from source. Installed binaries run without Rust, Cargo, or a network connection.

Use `vutils --version`, `vutils --author`, or `vutils --help` to inspect the installed CLI metadata.

## Interactive terminal interface

Run the full-screen interface in any interactive terminal:

```bash
vutils tui
```

The TUI opens on a customizable **Home** with the most common backend actions: JSON formatting, UUID and password generation, encryption/decryption, and the effective vutils configuration. Its fixed workflow tabs are **Random**, **Formatters**, **Parsers**, **Codecs**, **Security**, **Vruno**, and **Configuration**. Formatters groups output normalization such as JSON, SQL, cURL, YAML, XML, text, and byte sizes; Parsers groups validation, extraction, conversion, regex inspection, model inference, cron, and time utilities. Each operation exposes only its relevant parameters, a multiline input editor when needed, the exact generated CLI command, and a scrollable output preview. The Random tab keeps one caveat visible in the UUID version help: v3 and v5 are deterministic for the same namespace and name.

Use `0` to return Home; `1` opens Random, `2` Formatters, `3` Parsers, `4` Codecs, `5` Security, `6` Vruno, and `7` Configuration. `[`/`]` or `h`/`l` from the operations panel also change tabs. Navigate operations and fields with `j`/`k` or the arrow keys; `h`/`l` changes choices and numeric values in the parameter panel. Choice fields—such as UUID v1 through v8—change with left/right; `Space` toggles options such as password special characters; `Enter` edits text and numeric values such as password length. Run with `Ctrl-R` or `F5`; close with `q`, `:q`, `:qa`, or `:qall`; and press `?` for the complete shortcut reference. Vim keys remain ordinary text while editing Input or a field.

To customize Home, select any operation in its category and press `f` to add or remove it. From Home, `Delete` removes the selected shortcut and `R` restores the built-in set. Changes are saved atomically under `tui.home` in the existing `config.toml`; Home only references known vutils operations and never executes arbitrary shell commands.

Commands run through the current `vutils` executable without a shell. The editor content is sent through stdin, so the same parser, validation, persistent-configuration layer, and exit behavior used by the regular CLI remain active; values visible in the form are explicit and therefore take precedence over configured defaults. Binary output is shown as a safe hexadecimal preview instead of being written directly to the terminal.

### LazyVim

Copy the following spec to `~/.config/nvim/lua/plugins/vutils.lua`:

```lua
return {
  {
    "volneineves/vutils",
    build = "cargo build --release --locked",
    cmd = { "Vutils" },
    keys = {
      {
        "<leader>uv",
        function()
          require("vutils").open()
        end,
        desc = "Open vutils TUI",
      },
    },
    opts = { keymap = false },
  },
}
```

The build step lets the plugin work without a separate system installation and its matching binary takes precedence over `PATH`. Remove `build` to use an installed `vutils`, or set `command` explicitly. Use `:Vutils` or `<leader>uv` to open the floating terminal. Press `q` inside the TUI to exit normally, or press `Esc` twice to close the window immediately.

The window and executable can be customized through `opts`:

```lua
opts = {
  command = "/custom/path/to/vutils",
  width = 0.9,
  height = 0.85,
  border = "rounded",
  winblend = 0,
  keymap = false,
}
```

A ready-to-copy version of the spec is available at [`extras/lazyvim/vutils.lua`](extras/lazyvim/vutils.lua).

## Input and output

Transformations accept a positional value or existing file path, an explicit `--input <path>`, or stdin. Existing regular files are detected automatically; use `--literal` when text that happens to match a file name must not be read from disk. Output is written to stdout by default.

```bash
printf '%s' '{"name":"Ana"}' | vutils json pretty
vutils json pretty ./payload.json
vutils json to-yaml payload.json --output payload.yaml
vutils --in-place json pretty payload.json
vutils text trim --literal README.md
vutils base64 decode 'AAEC' --output bytes.bin
```

`--input` remains useful when a missing or unreadable path must produce a filesystem error instead of being treated as literal input. `--in-place` writes a temporary file beside the detected or explicit source and replaces the original only after successful transformation. `--output` refuses to overwrite an existing file unless `--force` is present. `--copy` additionally copies UTF-8 output to the local clipboard.

## Persistent defaults

Use `vutils config` to avoid repeating the same flags. Explicit command-line flags always win over the config, and built-in defaults remain active for keys that are not configured.

```bash
vutils config path
vutils config list
vutils config set sql.dialect postgres
vutils config set uuid.version v4
vutils config set uuid.format simple
vutils config set crypto.algorithm aes-256-gcm
vutils config set tui.home json.pretty,uuid,sql.format,code.types
vutils config get sql.dialect
vutils config unset uuid.format
```

| Key | Accepted values | Built-in default |
| --- | --- | --- |
| `sql.dialect` | `generic`, `postgres`, `mysql`, `sqlite`, `mssql` | `generic` |
| `uuid.version` | `v1` through `v8` | `v7` |
| `uuid.format` | `hyphenated`, `simple`, `urn`, `braced` | `hyphenated` |
| `crypto.algorithm` | `xchacha20-poly1305`, `aes-256-gcm` | `xchacha20-poly1305` for `enc` |
| `crypto.password-env` | Environment variable name | not set |
| `crypto.password-file` | Local file path | not set |
| `tui.home` | Comma-separated operation IDs | `json.pretty,uuid,gen.password,enc,dec,config.list` |
| `vruno.collection` | Bruno collection directory | not set |
| `vruno.openapi` | Local OpenAPI 3.x JSON or YAML file | not set |

Relative Vruno paths are resolved from the directory containing `config.toml`. `vutils vruno configure` validates and stores absolute paths atomically.

### Vruno: Bruno OpenAPI sync

Vruno implements OpenAPI drift detection and Bruno collection synchronization directly in vutils. It follows the conservative merge model proposed in [Bruno PR #7706](https://github.com/usebruno/bruno/pull/7706), but does not require `bru` or Node.js. Existing request values, authentication, tests, scripts, assertions, variables, and documentation are preserved; OpenAPI controls the URL and the structure of parameters, headers, and request bodies.

Configure one local OpenAPI 3.x `.json`, `.yaml`, or `.yml` file and its target collection:

```bash
vutils vruno configure \
  --collection /path/to/bruno-collection \
  --openapi /path/to/openapi.yaml
vutils vruno show
```

Use the read-only checks before applying changes:

```bash
vutils vruno check                  # reports drift; drift returns a non-zero status
vutils vruno preview                # native dry-run; writes nothing
vutils vruno sync --yes             # creates and updates collection requests
vutils vruno check --format json --group-by path
```

`sync` requires `--yes`. Stale requests are reported but never deleted, so removing local collection data always remains an explicit manual decision. The native engine currently supports classic Bruno collections (`bruno.json` with `.bru` request files) and bundled OpenAPI documents with internal `$ref` pointers. It refuses `opencollection.yml` and external `$ref` files instead of risking a lossy partial conversion. The same Configure, Show setup, Check drift, Preview sync, and Sync collection operations are available under TUI tab `6`.

Passwords are never stored directly in `config.toml`. Configure a source instead:

```bash
export VUTILS_PASSWORD='use-a-strong-password'
vutils config set crypto.password-env VUTILS_PASSWORD
ENCRYPTED="$(vutils enc 'Texto secreto')"  # reads VUTILS_PASSWORD
vutils dec "$ENCRYPTED"                    # reads VUTILS_PASSWORD
```

Setting `crypto.password-env` clears `crypto.password-file` and vice versa. A relative password-file path is resolved from the config directory. `dec` still detects the algorithm recorded in each envelope; `crypto.algorithm` selects the default for new `enc` output, while an explicit `--alg` overrides it.

The default config locations are `$XDG_CONFIG_HOME/vutils/config.toml` (or `~/.config/vutils/config.toml`) on Linux and `~/Library/Application Support/vutils/config.toml` on macOS. Set `VUTILS_CONFIG` to use a different file, which is also useful for isolated project profiles.

## Complete command reference

Every command runs locally. The tables describe the purpose of every command; use `vutils <command> --help` or `vutils help <command>` to see all flags, accepted values, defaults, and input modes.

### Identifiers, fixtures, and validation

| Command | Purpose |
| --- | --- |
| `uuid` | Generate UUID v1 through v8 in hyphenated, simple, URN, or braced format. Defaults to v7. |
| `id ulid` | Generate lexicographically sortable ULIDs. |
| `id nanoid` | Generate compact random NanoIDs with configurable length. |
| `id objectid` | Generate MongoDB-style 24-character ObjectId fixtures. |
| `gen password` | Generate passwords with configurable length, symbol use, and ambiguous-character exclusion. |
| `gen token` | Generate random tokens, optionally using a custom alphabet. |
| `gen email` / `gen name` | Generate local email or name fixtures. |
| `gen lorem` | Generate a requested number of Lorem Ipsum words. |
| `br` | Generate a complete Brazilian fixture profile as JSON: CPF, CNPJ, CEP, mobile phone, and PIX. |
| `br cpf` / `br cnpj` | Generate Brazilian CPF/CNPJ values or validate one with `--validate`; invalid values return exit code 1. |
| `br cep` / `br phone` | Generate synthetic Brazilian CEP or mobile phone fixtures. These are not looked up. |
| `br pix` | Generate synthetic random, CPF, CNPJ, email, or phone PIX keys. |

### Codecs, encryption, and structured data

| Command | Purpose |
| --- | --- |
| `base64 encode` / `base64 decode` | Convert binary data to or from standard or URL-safe Base64, with optional padding. |
| `binary encode` / `binary decode` | Convert bytes to 8-bit `0`/`1` text and decode those bits back to the original bytes. Alias: `bin`. |
| `hex encode` / `hex decode` | Convert binary data to or from hexadecimal text. |
| `url encode` / `url decode` | Percent-encode or decode URL/form components. |
| `url inspect` | Parse a URL and report its components without opening it. |
| `html encode` / `html decode` | Escape or decode HTML entities. |
| `gzip compress` / `gzip decompress` | Compress or decompress local GZip data. |
| `enc` | Encrypt text or binary input into a versioned, authenticated `vutils:v1` envelope. |
| `dec` | Authenticate and decrypt a `vutils:v1` envelope back to its original bytes. |
| `json pretty` / `json minify` | Format or compact JSON. |
| `json validate` | Validate JSON syntax. |
| `json escape` / `json unescape` | Encode a value as a JSON string literal or recover the string contents. |
| `json sort-keys` | Recursively sort object keys. |
| `json flatten` / `json unflatten` | Convert nested objects to or from dotted paths. |
| `json path` | Read a value using the supported JSON path expression syntax. |
| `json diff` | Compare two JSON inputs, optionally emitting a patch. |
| `json to-yaml` / `json to-csv` / `json to-toml` | Convert JSON to YAML, CSV, or TOML. |
| `json schema-validate` | Validate JSON against a local JSON Schema file. |
| `yaml pretty` / `yaml validate` | Format or validate YAML. |
| `yaml to-json` | Convert one JSON-compatible YAML document to JSON. |
| `yaml split` / `yaml join` | Split a multi-document YAML stream or join local YAML files. |
| `csv validate` / `csv to-json` | Validate CSV or convert rows to JSON objects. |
| `toml pretty` / `toml validate` / `toml to-json` | Format, validate, or convert TOML. |
| `xml pretty` / `xml validate` | Format or validate XML. |
| `dotenv parse` / `dotenv validate` / `dotenv sort` | Parse, validate, or sort dotenv entries. |
| `dotenv diff` | Compare dotenv files, hiding values unless explicitly requested. |

Convert text or source code to bits and decode it back to stdout:

```bash
vutils binary encode 'A'                         # 01000001
vutils binary encode --spaced 'let x = 1;'
vutils binary decode '01000001'                  # writes A to stdout

BITS="$(vutils bin encode 'fn main() {}')"
vutils bin decode "$BITS"                        # writes fn main() {} to stdout
vutils bin decode "$BITS" --output restored.rs  # writes the original bytes to a file
```

`binary decode` ignores spaces, line breaks, tabs, and `_`, but rejects any other character and requires complete 8-bit bytes. Its stdout is raw decoded data, so text appears directly while arbitrary bytes can be redirected or saved with `--output`.

Files and arbitrary binary data are preserved byte for byte. Use `--output` when a decoder produces a binary file and `--input` when encoding that file again:

```bash
# Base64 text -> binary -> Base64 text
vutils base64 decode 'AAEC/w==' --output payload.bin
vutils base64 encode --input payload.bin

# Hex text -> binary -> hex text
vutils hex decode '000102ff' --output payload-from-hex.bin
vutils hex encode --input payload-from-hex.bin

# Binary compression and decompression
vutils gzip compress --input payload.bin --output payload.bin.gz
vutils gzip decompress --input payload.bin.gz --output payload-restored.bin

# Binary encryption and authenticated decryption
vutils enc --input payload.bin --passwd-env VUTILS_PASSWORD --output payload.vutils
vutils dec --input payload.vutils --passwd-env VUTILS_PASSWORD --output payload-decrypted.bin
cmp payload.bin payload-decrypted.bin
```

Without `--output`, decoded binary bytes are written directly to stdout and can be piped or redirected. Text encoders add a final newline only when emitted as text; binary decoders and `dec` do not modify the decoded bytes.

### Text, regex, and code generation

| Command | Purpose |
| --- | --- |
| `code types` | Infer Rust, Kotlin, C#, or TypeScript models from a JSON example. |
| `text case` | Convert text to camelCase, PascalCase, snake_case, kebab-case, CONSTANT_CASE, or Title Case. |
| `text slug` | Produce a normalized URL-friendly slug. |
| `text trim` | Trim surrounding whitespace. |
| `text sort-lines` | Sort lines, optionally descending and/or unique. |
| `text unique-lines` | Remove duplicate lines while retaining the first occurrence. |
| `text normalize-eol` | Normalize line endings to LF or CRLF. |
| `text diff` | Produce a human-readable text diff. |
| `text unicode` | Inspect Unicode code points and character information. |
| `text only-digits` | Remove every non-digit character. |
| `regex test` | Test a Rust-compatible regular expression and report matches. |
| `regex replace` | Replace all or only the first regular-expression match. |
| `string escape` / `string unescape` | Escape or unescape literals for JSON, Rust, Kotlin, Java, C#, JavaScript, TypeScript, Python, SQL, or POSIX shell. |
| `number convert` | Convert an integer between bases 2 through 36. |
| `bytes format` / `bytes parse` | Convert byte counts to human-readable SI/IEC sizes or parse them back. |

### Offline cURL and SQL formatting

| Command | Purpose |
| --- | --- |
| `curl format` | Normalize and safely quote a static cURL command. |
| `sql format` | Format SQL for the selected dialect, keyword case, and indentation. |

### Security and local inspection

| Command | Purpose |
| --- | --- |
| `hash sha256` / `hash sha512` | Calculate a one-way cryptographic digest. Hashes cannot be decrypted. |
| `hmac` | Calculate a SHA-256 or SHA-512 keyed message authentication code. |
| `password-hash argon2-hash` / `argon2-verify` | Hash or verify a password using Argon2. |
| `password-hash bcrypt-hash` / `bcrypt-verify` | Hash or verify a UTF-8 password using bcrypt. |
| `totp generate-secret` | Generate a local Base32 TOTP secret. |
| `totp code` / `totp verify` | Calculate or verify offline TOTP codes with configurable algorithm, period, digits, and window. |
| `jwt decode` | Decode JWT header/payload and inspect temporal claims without verifying the signature. |
| `checksum file` / `checksum directory` | Calculate deterministic SHA checksums for a file or directory tree. |
| `pem inspect` | List PEM blocks, decoded sizes, and whether a block appears sensitive. |
| `cert inspect` | Inspect a local PEM X.509 certificate, including names, validity, serial, and SANs. |
| `qr generate` | Render input as a terminal, SVG, or PNG QR code. |

### Time, paths, versions, and shell integration

| Command | Purpose |
| --- | --- |
| `time now` | Return local RFC 3339 time by default, UTC with `--utc`, or Unix time with `--unix`. |
| `time to-iso` / `time to-unix` | Convert Unix timestamps and RFC 3339 values. |
| `time duration` | Parse a human-readable duration into milliseconds. |
| `cron next` / `cron explain` | Parse a cron expression and list upcoming local or UTC occurrences. |
| `chmod encode` / `chmod decode` | Convert symbolic Unix permissions and octal modes. |
| `path normalize` | Lexically normalize a path without accessing the filesystem target. |
| `path relative` | Calculate a relative path between two paths. |
| `semver compare` / `semver sort` / `semver bump` | Compare, sort, or increment semantic versions. |
| `ip cidr` | Inspect an IP/CIDR network, prefix, mask, and address range. |
| `mime` | Look up a MIME type from a file extension using the built-in table. |
| `tui` | Open the category-based full-screen workbench for interactive transformations and generators. |
| `vruno configure/show/check/preview/sync` | Configure and run Bruno OpenAPI collection drift checks and synchronization. |
| `config path/list/get/set/unset` | Inspect or manage validated per-user defaults for UUID, SQL, encryption, the TUI Home, and Vruno. |
| `completion` | Generate Bash, Zsh, Fish, PowerShell, or Elvish completion scripts. |
| `man` | Generate the `vutils(1)` manual page. |

## UUIDs

```bash
vutils uuid                         # v7 by default
vutils uuid --version v4 --count 5
vutils uuid --version v5 --namespace url --name https://example.com
vutils uuid --version v2 --domain person --local-id 1000
vutils uuid --version v8 --custom-bytes 00112233445566778899aabbccddeeff
```

| Version | What it contains | Typical use and caveats |
| --- | --- | --- |
| UUID v1 | Gregorian timestamp, clock sequence, and 48-bit node ID. | Legacy time-based identifiers. They are roughly ordered but can expose generation time and a supplied hardware-like node ID. `vutils` uses a random multicast node by default. |
| UUID v2 | DCE Security local domain and local identifier mixed with v1 fields. | Legacy person/group/organization fixtures. It is rarely supported and is not recommended for new systems. |
| UUID v3 | Namespace plus name hashed with MD5. | Deterministic compatibility IDs: the same namespace/name always produces the same UUID. MD5 here provides identity mapping, not password/security protection. |
| UUID v4 | 122 random bits. | General random identifiers when ordering is not required. |
| UUID v5 | Namespace plus name hashed with SHA-1. | Preferred deterministic UUID over v3 for compatibility, but it is still an identifier, not a security primitive. |
| UUID v6 | Reordered v1 timestamp plus clock/node fields. | Time-sortable replacement for v1 while retaining its node/time characteristics. |
| UUID v7 | Unix epoch milliseconds plus random bits. | Default and recommended for most new database/application IDs: naturally sortable with no MAC address. |
| UUID v8 | Application-defined 128-bit layout with version/variant bits normalized. | Controlled interoperability or test fixtures. Semantics and uniqueness are the application's responsibility. |

For v3/v5, pass `--namespace dns|url|oid|x500|<uuid>` and `--name <value>`. For v1/v2/v6, `--node-id` accepts 12 hexadecimal digits; omitting it avoids embedding a real MAC address. UUID v2 is a best-effort fixture because `vutils` has no DCE registry that can guarantee globally assigned local IDs. With a fixed node ID, one v2 batch is limited to 64 values. UUID v8 requires exactly 16 bytes (32 hexadecimal digits) through `--custom-bytes`.

## Brazilian development fixtures

Country-specific data is grouped under its country code so other countries can be added without mixing national rules into generic generators. Running `br` without a subcommand produces one complete JSON profile:

```bash
vutils br
vutils br --help
vutils br cpf --count 5 --formatted
vutils br cpf --validate '529.982.247-25'
vutils br cnpj --validate '11.222.333/0001-81'
vutils br cep --formatted
vutils br phone --count 3
vutils br pix --kind phone
```

CPF and CNPJ validation checks syntax/check digits only; it does not query Receita Federal or establish that a document was issued. CEP, phone, PIX, and document generation is synthetic test data.

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

Aliases matching code spelling are accepted directly, including `camelCase`, `PascalCase`, `snake_case`, `kebabCase`, `CONSTANT_CASE`, and `TitleCase`. Descriptive forms such as `camel-case`, `camelcase`, `pascal-case`, `pascalcase`, `snake-case`, and `snakecase` also work.

## cURL formatting

The formatter accepts one static POSIX cURL command, normalizes flags and quoting, and prints the result. It never sends a request or invokes a shell.

```bash
vutils curl format "curl -XPOST -H 'Accept: application/json' https://example.com"
vutils curl format --input request.curl
vutils curl format --shell powershell --input request.curl
```

Operators, substitutions, redirections, unsupported flags, and non-HTTP URLs are rejected.

## SQL formatting

No database connection is made.

```bash
vutils sql format --dialect postgres 'select id,name from users where id=$1'
vutils sql format --dialect mysql --keyword-case lower --indent 4 --input query.sql
vutils config set sql.dialect postgres
vutils sql format 'select id,name from users where id=$1'
```

Supported dialects are `generic`, `postgres`, `mysql`, `sqlite`, and `mssql`. Formatting validates the SQL syntax locally before producing output.

## Password encryption and decryption

`enc` encrypts arbitrary text or binary input; `dec` authenticates the envelope before returning the original bytes. XChaCha20-Poly1305 is the default:

```bash
vutils enc "Texto secreto" --passwd 123
export VUTILS_PASSWORD='use-a-strong-password'
ENCRYPTED="$(vutils enc 'Texto secreto' --passwd-env VUTILS_PASSWORD)"
vutils dec "$ENCRYPTED" --passwd-env VUTILS_PASSWORD
vutils enc --input secret.bin --passwd-file password.txt --output secret.vutils
vutils dec --input secret.vutils --passwd-file password.txt --output secret.bin
```

Select or inspect algorithms with:

```bash
vutils enc --help
vutils enc "Texto secreto" --passwd 123 --alg xchacha20-poly1305
vutils dec "$ENCRYPTED" --passwd-env VUTILS_PASSWORD --alg aes-256-gcm
```

| Algorithm | Notes |
| --- | --- |
| `xchacha20-poly1305` | Default. Modern authenticated stream cipher with an extended random nonce and no dependency on AES hardware acceleration. |
| `aes-256-gcm` | Widely supported authenticated encryption with a 256-bit derived key and a random 96-bit nonce. |

The `vutils:v1` envelope records the algorithm, KDF, random salt, random nonce, and authenticated ciphertext in URL-safe Base64. The password-derived key uses the RFC 9106 memory-constrained profile for Argon2id v1.3 (`m=65536 KiB`, `t=3`, `p=4`, 128-bit salt, 256-bit key), fixed for envelope v1 so future library-default changes cannot break existing data. Decryption rejects a wrong password, altered data, unsupported versions, and an optional mismatched `--alg`. Successful `enc` and `dec` commands print `algorithm: <name>` to stderr while stdout remains the unmodified result channel for pipes and binary files.

SHA-256 and SHA-512 are deliberately not accepted by `--alg`: SHA is one-way hashing and cannot support `dec`. Use `vutils hash sha256` or `vutils hash sha512` when a digest is the intended result.

## Secrets

Encryption prefers `--passwd-file` or `--passwd-env`. HMAC, TOTP, and password hashing prefer stdin, `--secret-file`, or `--secret-env`. `--passwd` and `--secret` are convenient but may be visible in shell history and process listings.

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

Every push to `main` creates a uniquely versioned GitHub prerelease named `v<crate-version>-build.<run>.<attempt>`. Pushing a `v*` tag creates a stable release. Both paths build and attach Linux and macOS binaries, Debian and RPM packages, plus `SHA256SUMS`. Stable aliases `vutils-latest-amd64.deb` and `vutils-latest-x86_64.rpm` keep installation URLs independent of the version number.

## License

Licensed under either Apache License 2.0 or MIT, at your option.
