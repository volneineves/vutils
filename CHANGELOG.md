# Changelog

All notable changes to this project are documented here. The format follows Keep a Changelog and the project uses Semantic Versioning.

## [Unreleased]

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
