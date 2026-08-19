# Changelog

All notable changes to this project are documented here. The format follows Keep a Changelog and the project uses Semantic Versioning.

## [Unreleased]

## [0.7.0] - 2026-08-19

### Added

- UUID validation through `vutils uuid --validate <uuid>` and the Validators tab, with support for hyphenated, simple, URN, and braced representations.
- `vu` as a complete short executable alias, included in release archives, DEB packages, and RPM packages.
- Vruno OpenAPI loading from explicit HTTP(S) URLs, with bounded downloads, plus `file://`, collection-directory, and `bruno.json` location support for local Bruno collections.
- Automatic reuse of the last successfully used encryption key through the native operating-system credential store, with `config forget-key` for explicit removal.

### Changed

- Linux release artifacts and CI are built on Ubuntu 22.04, with an automated gate that prevents binaries from requiring a glibc version newer than 2.35.
- Encrypt and Decrypt use `--key`, `--key-file`, and `--key-env` as their primary options while retaining the former `--passwd*` names as visible backward-compatible aliases.
- TUI arrow keys now stay within Input and Output, with `Tab`/`Shift-Tab` providing predictable panel navigation and `h`/`l` retaining Vim-style compatibility.

## [0.6.0] - 2026-08-16

### Added

- Dedicated Validators tab in the TUI for JSON, JSON Schema, YAML, CSV, TOML, XML, and dotenv validation, separated from parser workflows and available through shortcut `4`.

### Changed

- Replaced Configuration with SQL formatting in the default TUI Home shortcuts.

## [0.5.1] - 2026-08-16

### Changed

- Clarified throughout the TUI, CLI help, and documentation that `crypto.password-env` stores only an environment-variable name; the variable value is a secret passphrase or text used to automate `enc` and `dec`.

## [0.5.0] - 2026-08-16

### Added

- Full-screen `vutils tui` backend workbench with category tabs, contextual typed forms, editable stdin, asynchronous execution, exact command previews, and safe text/binary output previews.
- Guided UUID v1-v8 selection and password controls for length, quantity, special characters, and ambiguous-character exclusion.
- LazyVim adapter with a floating terminal, `:Vutils`, configurable keymap/window options, and an optional plugin-local release build.
- Customizable TUI Home with persisted favorite operations, common backend defaults including `enc`/`dec`, configuration access, and safe add/remove/reset shortcuts.
- Vim-style TUI navigation and visible Ex command mode supporting `:q`, `:qa`, and `:qall` variants.
- Dedicated Configuration tab in the last top-level position, now available through shortcut `7` while retaining configuration access from Home.
- Random category for local generators, with an inline reminder that UUID v3 and v5 are deterministic rather than random.
- Native Vruno OpenAPI-to-Bruno synchronization, with persisted collection/OpenAPI paths, conservative local-data-preserving merges, safe drift checks, dry-run previews, and confirmed writes without a `bru` dependency.
- Typed Configuration-tab editors for every supported setting, including reset/unset flows and persisted-value prefill.
- Dimmed input examples that behave as placeholders instead of submitted content.
- Typed TUI coverage for every operational CLI leaf command, with an automated catalog-coverage regression test.
- Guided Encrypt/Decrypt password sources with configured-source prefill, masked direct secrets, redacted command previews, and child-process argument protection.
- Safe `q` quit confirmation with a default **No** selection, keyboard navigation, and explicit cancel/confirm shortcuts.

### Changed

- Replaced the legacy JSON/Data/Text/Backend tab model with the fixed Home, Random, Formatters, Parsers, Codecs, Security, Vruno, and Configuration workflow taxonomy.
- Scoped TUI output to the selected operation so tab changes and background completions cannot leak unrelated results into the active workspace.

## [0.4.1] - 2026-08-09

### Added

- Author metadata output and expanded configuration help.

### Changed

- Existing regular files can be supplied directly as positional input, while `--literal` keeps ambiguous values as text.
- Installation examples use temporary downloads and simpler cleanup.
- Validation remains warning-free on macOS.

## [0.4.0] - 2026-08-09

### Added

- Persistent validated defaults for frequently repeated command options.
- Binary bit-string encoding and decoding utilities.

## [0.3.0] - 2026-08-09

### Added

- Password-based `enc` and `dec` commands with XChaCha20-Poly1305 by default and optional AES-256-GCM.
- Versioned `vutils:v1` envelopes using Argon2id, random salts/nonces, authenticated ciphertext, and URL-safe Base64.
- Complete README command reference, UUID v1-v8 guidance, and copyable `curl` installation commands for the latest stable release.
- Country-oriented `br` commands for complete profiles plus CPF, CNPJ, CEP, phone, and PIX generation; CPF/CNPJ validation now lives beside generation.

### Changed

- XChaCha20-Poly1305 is the default encryption algorithm; successful encryption/decryption reports the selected algorithm on stderr without corrupting stdout pipelines.
- Brazilian fixture logic moved into an extensible `countries::br` module instead of the generic generator namespace.

## [0.2.0] - 2026-08-09

### Added

- Discoverable aliases for camelCase, PascalCase, snake_case, kebab-case, CONSTANT_CASE, and Title Case conversion.
- Explicit `--utc` output for timestamp and cron formatting.
- Automated GitHub releases for Linux, Windows, Intel/Apple Silicon macOS, DEB, and RPM artifacts with SHA-256 checksums.
- Installation instructions that use prebuilt artifacts and do not require Rust or Cargo.

### Changed

- `time now`, `time to-iso`, and cron occurrences now use the machine's local timezone by default.
- Unix output from `time now` is now requested explicitly with `--unix`; Unix timestamps remain timezone-independent.
- Direct dependencies were upgraded to their newest Rust 1.88-compatible releases and the MSRV is now Rust 1.88.

## [0.1.0] - 2026-08-09

### Added

- Offline CLI architecture with argument, stdin, file, atomic in-place, output, and clipboard support.
- UUID v1-v8, ULID, NanoID, ObjectId, local test-data generators, and Brazilian document validation.
- JSON, YAML, CSV, TOML, XML, dotenv, codecs, text, regex, time, and filesystem-independent calculators.
- Rust, Kotlin, C#, and TypeScript type generation from JSON examples.
- Offline HTTP/cURL parsing and rendering plus SQL formatting, inspection, and parameterized statement generation.
- Hash, HMAC, Argon2, bcrypt, TOTP, JWT inspection, checksums, PEM/X.509 inspection, and QR generation.
