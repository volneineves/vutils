mod cli;
mod key_store;
mod tui;
mod update;
mod vruno;

use std::{env, fs, io::Write as _, path::PathBuf, process::ExitCode};

use clap::{CommandFactory as _, Parser as _};
use cli::*;
use semver::Version;
use vutils::{
    Result, VutilsError, codec, codegen,
    config::UserConfig,
    countries, data, generators, http, identifiers,
    io::{InputArgs, OutputArgs},
    security, sql, text, time,
};

struct Outcome {
    bytes: Vec<u8>,
    textual: bool,
    input: InputArgs,
    success: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.update {
        return match update::run() {
            Ok(message) => {
                println!("{message}");
                ExitCode::SUCCESS
            }
            Err(error) => fail(&error),
        };
    }
    if cli.author {
        println!("{}", env!("CARGO_PKG_AUTHORS"));
        return ExitCode::SUCCESS;
    }

    if matches!(cli.command.as_ref(), Some(Command::Tui)) {
        if cli.output.is_some() || cli.in_place || cli.force || cli.copy {
            return fail(&VutilsError::InvalidInput(
                "output flags cannot be used with the interactive TUI".into(),
            ));
        }
        return match tui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(&error),
        };
    }

    if matches!(cli.command.as_ref(), Some(Command::Vruno(_))) && cli.in_place {
        return fail(&VutilsError::InvalidInput(
            "--in-place cannot be used with Vruno commands".into(),
        ));
    }
    let vruno_writes = matches!(
        cli.command.as_ref(),
        Some(Command::Vruno(
            VrunoCommand::Configure(_) | VrunoCommand::Sync(_)
        ))
    );
    if vruno_writes && (cli.output.is_some() || cli.force || cli.copy) {
        return fail(&VutilsError::InvalidInput(
            "output flags cannot be used with Vruno configure or sync".into(),
        ));
    }

    let output = OutputArgs {
        output: cli.output,
        in_place: cli.in_place,
        force: cli.force,
        copy: cli.copy,
    };
    let Some(command) = cli.command else {
        Cli::command()
            .error(
                clap::error::ErrorKind::MissingSubcommand,
                "a subcommand is required unless --author is present",
            )
            .exit();
    };
    let result = match command {
        Command::Config(command) => dispatch_config(command),
        command => UserConfig::load().and_then(|config| dispatch(command, &config)),
    };
    match result {
        Ok(result) => match vutils::io::emit(&result.bytes, &result.input, &output, result.textual)
        {
            Ok(()) if result.success => ExitCode::SUCCESS,
            Ok(()) => ExitCode::FAILURE,
            Err(error) => fail(&error),
        },
        Err(error) => fail(&error),
    }
}

#[allow(clippy::too_many_lines)]
fn dispatch(command: Command, config: &UserConfig) -> Result<Outcome> {
    match command {
        Command::Tui => Err(VutilsError::Message(
            "internal error: TUI command reached regular dispatcher".into(),
        )),
        Command::Uuid(args) => {
            if let Some(value) = args.validate.as_deref() {
                return status_out(identifiers::validate_uuid(value));
            }
            let version = resolve_uuid_version(args.version, config)?;
            let format = resolve_uuid_format(args.format, config)?;
            validate_count(args.count)?;
            if matches!(version, identifiers::UuidVersion::V2)
                && args.node_id.is_some()
                && args.count > 64
            {
                return Err(VutilsError::InvalidInput(
                    "UUID v2 with a fixed node ID is limited to 64 values per batch".into(),
                ));
            }
            let options = identifiers::UuidOptions {
                version,
                namespace: args.namespace.as_deref(),
                name: args.name.as_deref(),
                node_id: args.node_id.as_deref(),
                custom_bytes: args.custom_bytes.as_deref(),
                dce_domain: args.domain.map(map_dce_domain),
                local_id: args.local_id,
                dce_sequence: None,
            };
            let values = (0..args.count)
                .map(|index| {
                    let mut item_options = options.clone();
                    if matches!(version, identifiers::UuidVersion::V2) && args.node_id.is_some() {
                        item_options.dce_sequence = Some(index as u8);
                    }
                    identifiers::generate_uuid(&item_options)
                        .map(|value| identifiers::format_uuid(&value, format))
                })
                .collect::<Result<Vec<_>>>()?;
            text_out(values.join("\n"), InputOptions::default())
        }
        Command::Id(command) => match command {
            IdCommand::Ulid { count } => text_out(
                repeat(count, || Ok(identifiers::generate_ulid()))?.join("\n"),
                InputOptions::default(),
            ),
            IdCommand::Nanoid { length, count } => text_out(
                repeat(count, || identifiers::generate_nanoid(length))?.join("\n"),
                InputOptions::default(),
            ),
            IdCommand::Objectid { count } => text_out(
                repeat(count, || Ok(identifiers::generate_object_id()))?.join("\n"),
                InputOptions::default(),
            ),
        },
        Command::Gen(command) => dispatch_generator(command),
        Command::Br(args) => dispatch_br(args),
        Command::Config(_) => Err(VutilsError::Message(
            "internal error: config command reached regular dispatcher".into(),
        )),
        Command::Vruno(command) => dispatch_vruno(command, config),
        Command::Base64(command) => match command {
            Base64Command::Encode {
                input,
                url_safe,
                no_padding,
            } => {
                let value = codec::base64_encode(&read_bytes(&input)?, url_safe, !no_padding);
                text_out(value, input)
            }
            Base64Command::Decode {
                input,
                url_safe,
                no_padding,
            } => {
                let value = codec::base64_decode(&read_text(&input)?, url_safe, !no_padding)?;
                binary_out(value, input)
            }
        },
        Command::Binary(command) => match command {
            BinaryCommand::Encode { input, spaced } => {
                text_out(codec::binary_encode(&read_bytes(&input)?, spaced), input)
            }
            BinaryCommand::Decode(input) => {
                binary_out(codec::binary_decode(&read_text(&input)?)?, input)
            }
        },
        Command::Hex(command) => match command {
            HexCommand::Encode { input, uppercase } => {
                text_out(codec::hex_encode(&read_bytes(&input)?, uppercase), input)
            }
            HexCommand::Decode(input) => binary_out(codec::hex_decode(&read_text(&input)?)?, input),
        },
        Command::Url(command) => match command {
            UrlCommand::Encode { input, form } => {
                text_out(codec::url_encode(&read_text(&input)?, form), input)
            }
            UrlCommand::Decode { input, form } => {
                text_out(codec::url_decode(&read_text(&input)?, form)?, input)
            }
            UrlCommand::Inspect(input) => text_out(http::inspect_url(&read_text(&input)?)?, input),
        },
        Command::Html(command) => match command {
            TextCodecCommand::Encode(input) => {
                text_out(codec::html_encode(&read_text(&input)?), input)
            }
            TextCodecCommand::Decode(input) => {
                text_out(codec::html_decode(&read_text(&input)?), input)
            }
        },
        Command::Gzip(command) => match command {
            GzipCommand::Compress { input, level } => {
                binary_out(codec::gzip_compress(&read_bytes(&input)?, level)?, input)
            }
            GzipCommand::Decompress(input) => {
                binary_out(codec::gzip_decompress(&read_bytes(&input)?)?, input)
            }
        },
        Command::Enc(args) => {
            let key = read_encryption_key(args.key, config)?;
            let algorithm = resolve_encryption_algorithm(args.algorithm, config)?;
            let encrypted = security::encrypt(&read_bytes(&args.input)?, key.as_ref(), algorithm)?;
            remember_encryption_key(key.as_ref());
            eprintln!("algorithm: {}", algorithm.name());
            text_out(encrypted, args.input)
        }
        Command::Dec(args) => {
            let key = read_encryption_key(args.key, config)?;
            let decrypted = security::decrypt(
                &read_text(&args.input)?,
                key.as_ref(),
                args.algorithm.map(map_encryption_algorithm),
            )?;
            remember_encryption_key(key.as_ref());
            eprintln!("algorithm: {}", decrypted.algorithm.name());
            binary_out(decrypted.plaintext, args.input)
        }
        Command::Json(command) => dispatch_json(command),
        Command::Yaml(command) => dispatch_yaml(command),
        Command::Csv(command) => match command {
            CsvCommand::Validate(input) => {
                data::csv_to_json(&read_text(&input)?)?;
                status_with_input(true, input)
            }
            CsvCommand::ToJson(input) => text_out(data::csv_to_json(&read_text(&input)?)?, input),
        },
        Command::Toml(command) => match command {
            TomlCommand::Pretty(input) => text_out(data::toml_pretty(&read_text(&input)?)?, input),
            TomlCommand::Validate(input) => {
                data::toml_pretty(&read_text(&input)?)?;
                status_with_input(true, input)
            }
            TomlCommand::ToJson(input) => text_out(data::toml_to_json(&read_text(&input)?)?, input),
        },
        Command::Xml(command) => match command {
            XmlCommand::Pretty(input) => text_out(data::xml_pretty(&read_text(&input)?)?, input),
            XmlCommand::Validate(input) => {
                data::xml_pretty(&read_text(&input)?)?;
                status_with_input(true, input)
            }
        },
        Command::Dotenv(command) => dispatch_dotenv(command),
        Command::Code(CodeCommand::Types { lang, name, input }) => text_out(
            codegen::generate_types(&read_text(&input)?, map_language(lang), &name)?,
            input,
        ),
        Command::Curl(command) => dispatch_curl(command),
        Command::Sql(command) => dispatch_sql(command, config),
        Command::Text(command) => dispatch_text(command),
        Command::Regex(command) => match command {
            RegexCommand::Test { pattern, input } => text_out(
                serde_json::to_string_pretty(&text::regex_test(&pattern, &read_text(&input)?)?)
                    .map_err(message)?,
                input,
            ),
            RegexCommand::Replace {
                pattern,
                replacement,
                first_only,
                input,
            } => text_out(
                text::regex_replace(&pattern, &replacement, &read_text(&input)?, first_only)?,
                input,
            ),
        },
        Command::StringValue(command) => match command {
            StringCommand::Escape { language, input } => text_out(
                text::escape_string(&read_text(&input)?, map_escape_language(language))?,
                input,
            ),
            StringCommand::Unescape { language, input } => text_out(
                text::unescape_string(&read_text(&input)?, map_escape_language(language))?,
                input,
            ),
        },
        Command::Number(NumberCommand::Convert { value, from, to }) => text_out(
            text::convert_number(&value, from, to)?,
            InputOptions::default(),
        ),
        Command::Bytes(command) => match command {
            BytesCommand::Format {
                value,
                iec,
                precision,
            } => text_out(
                text::format_bytes(value, iec, precision)?,
                InputOptions::default(),
            ),
            BytesCommand::Parse { value } => text_out(
                text::parse_bytes(&value)?.to_string(),
                InputOptions::default(),
            ),
        },
        Command::Hash(command) => match command {
            HashCommand::Sha256(input) => text_out(
                security::digest(&read_bytes(&input)?, security::DigestAlgorithm::Sha256),
                input,
            ),
            HashCommand::Sha512(input) => text_out(
                security::digest(&read_bytes(&input)?, security::DigestAlgorithm::Sha512),
                input,
            ),
        },
        Command::Hmac(args) => {
            if args.input.value.is_none()
                && args.input.input.is_none()
                && args.secret.secret.is_none()
                && args.secret.secret_file.is_none()
                && args.secret.secret_env.is_none()
            {
                return Err(VutilsError::InvalidInput(
                    "message input and HMAC secret cannot both use stdin; provide one explicitly"
                        .into(),
                ));
            }
            text_out(
                security::hmac(
                    &read_bytes(&args.input)?,
                    &read_secret(&args.secret)?,
                    map_hash(args.algorithm),
                )?,
                args.input,
            )
        }
        Command::PasswordHash(command) => dispatch_password_hash(command),
        Command::Totp(command) => dispatch_totp(command),
        Command::Jwt(JwtCommand::Decode(input)) => {
            eprintln!("warning: JWT signature was not verified");
            text_out(security::decode_jwt(&read_text(&input)?)?, input)
        }
        Command::Checksum(command) => match command {
            ChecksumCommand::File { path, algorithm } => text_out(
                security::checksum_file(&path, map_hash(algorithm))?,
                InputOptions::default(),
            ),
            ChecksumCommand::Directory {
                path,
                algorithm,
                follow_links,
            } => text_out(
                security::checksum_directory(&path, map_hash(algorithm), follow_links)?,
                InputOptions::default(),
            ),
        },
        Command::Pem(PemCommand::Inspect(input)) => {
            text_out(security::inspect_pem(&read_text(&input)?)?, input)
        }
        Command::Cert(CertCommand::Inspect(input)) => {
            text_out(security::inspect_certificate(&read_text(&input)?)?, input)
        }
        Command::Time(command) => dispatch_time(command),
        Command::Cron(command) => match command {
            CronCommand::Next {
                expression,
                count,
                utc,
            }
            | CronCommand::Explain {
                expression,
                count,
                utc,
            } => text_out(
                time::explain_cron(&expression, count, map_output_timezone(utc))?,
                InputOptions::default(),
            ),
        },
        Command::Chmod(command) => match command {
            ChmodCommand::Encode { value } => {
                text_out(time::chmod_encode(&value)?, InputOptions::default())
            }
            ChmodCommand::Decode { value } => {
                text_out(time::chmod_decode(&value)?, InputOptions::default())
            }
        },
        Command::Path(command) => match command {
            PathCommand::Normalize { value } => text_out(
                time::normalize_path(&value).to_string_lossy().into_owned(),
                InputOptions::default(),
            ),
            PathCommand::Relative { from, to } => text_out(
                time::relative_path(&from, &to)?
                    .to_string_lossy()
                    .into_owned(),
                InputOptions::default(),
            ),
        },
        Command::Semver(command) => dispatch_semver(command),
        Command::Ip(IpCommand::Cidr { value }) => {
            text_out(time::inspect_cidr(&value)?, InputOptions::default())
        }
        Command::Qr(QrCommand::Generate {
            format,
            size,
            input,
        }) => match format {
            QrFormatArg::Terminal => text_out(security::qr_terminal(&read_text(&input)?)?, input),
            QrFormatArg::Svg => text_out(security::qr_svg(&read_text(&input)?, size)?, input),
            QrFormatArg::Png => binary_out(security::qr_png(&read_text(&input)?, size)?, input),
        },
        Command::Completion { shell } => completion(shell),
        Command::Man => man_page(),
        Command::Mime { extension } => text_out(
            http::mime_lookup(&extension).into(),
            InputOptions::default(),
        ),
    }
}

fn dispatch_config(command: ConfigCommand) -> Result<Outcome> {
    match command {
        ConfigCommand::Path => text_out(
            vutils::config::config_path()?.display().to_string(),
            InputOptions::default(),
        ),
        ConfigCommand::List => {
            let config = UserConfig::load()?;
            let values = config
                .entries()
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("\n");
            text_out(values, InputOptions::default())
        }
        ConfigCommand::Get { key } => {
            let config = UserConfig::load()?;
            text_out(config.get(&key)?, InputOptions::default())
        }
        ConfigCommand::Set { key, value } => {
            let mut config = UserConfig::load()?;
            config.set(&key, &value)?;
            let effective = config.get(&key)?;
            config.save()?;
            text_out(effective, InputOptions::default())
        }
        ConfigCommand::Unset { key } => {
            let mut config = UserConfig::load()?;
            config.unset(&key)?;
            let effective = config.get(&key).unwrap_or_else(|_| "<unset>".into());
            config.save()?;
            text_out(effective, InputOptions::default())
        }
        ConfigCommand::ForgetKey => {
            let message = if key_store::forget().map_err(VutilsError::Message)? {
                "saved encryption key removed"
            } else {
                "no saved encryption key"
            };
            text_out(message.into(), InputOptions::default())
        }
    }
}

fn dispatch_vruno(command: VrunoCommand, config: &UserConfig) -> Result<Outcome> {
    match command {
        VrunoCommand::Configure(args) => {
            let collection = vruno::validate_collection(&args.collection)?;
            let openapi = vruno::validate_openapi(&args.openapi)?;
            let mut updated = config.clone();
            updated.set("vruno.collection", &collection.display().to_string())?;
            updated.set("vruno.openapi", &openapi.display().to_string())?;
            updated.save()?;
            text_out(
                format!(
                    "collection={}\nopenapi={}",
                    collection.display(),
                    openapi.display()
                ),
                InputOptions::default(),
            )
        }
        VrunoCommand::Show => text_out(vruno_setup(config), InputOptions::default()),
        VrunoCommand::Check(args) => {
            run_vruno(args.run, config, vruno::SyncMode::Check, args.output_format)
        }
        VrunoCommand::Preview(args) => run_vruno(
            args,
            config,
            vruno::SyncMode::Preview,
            VrunoOutputFormatArg::Text,
        ),
        VrunoCommand::Sync(args) => {
            if !args.yes {
                return Err(VutilsError::InvalidInput(
                    "Vruno sync writes collection files; run preview first, then pass --yes to confirm"
                        .into(),
                ));
            }
            run_vruno(
                args.run,
                config,
                vruno::SyncMode::Sync,
                VrunoOutputFormatArg::Text,
            )
        }
    }
}

fn vruno_setup(config: &UserConfig) -> String {
    format!(
        "engine=native\ncollection={}\nopenapi={}",
        config
            .vruno_collection()
            .map_or_else(|| "<unset>".into(), |path| path.display().to_string()),
        config
            .vruno_openapi()
            .map_or_else(|| "<unset>".into(), |path| path.display().to_string())
    )
}

fn run_vruno(
    args: VrunoRunArgs,
    config: &UserConfig,
    mode: vruno::SyncMode,
    output_format: VrunoOutputFormatArg,
) -> Result<Outcome> {
    let collection = args
        .collection
        .or_else(|| config.vruno_collection())
        .ok_or_else(|| {
            VutilsError::InvalidInput(
                "Vruno collection is not configured; run `vu vruno configure` first or pass --collection".into(),
            )
        })?;
    let openapi = args.openapi.or_else(|| config.vruno_openapi()).ok_or_else(|| {
        VutilsError::InvalidInput(
            "Vruno OpenAPI source is not configured; run `vu vruno configure` first or pass --openapi".into(),
        )
    })?;
    let request = vruno::SyncRequest {
        collection,
        openapi,
        mode,
        json: matches!(output_format, VrunoOutputFormatArg::Json),
        group_by: match args.group_by {
            VrunoGroupByArg::Tags => vruno::GroupBy::Tags,
            VrunoGroupByArg::Path => vruno::GroupBy::Path,
        },
    };
    let output = vruno::run(&request)?;
    Ok(Outcome {
        bytes: output.stdout,
        textual: true,
        input: InputArgs::default(),
        success: output.success,
    })
}

fn dispatch_generator(command: GenCommand) -> Result<Outcome> {
    let values = match command {
        GenCommand::Password {
            length,
            count,
            no_symbols,
            exclude_ambiguous,
        } => repeat(count, || {
            generators::password(length, !no_symbols, exclude_ambiguous)
        })?,
        GenCommand::Token {
            length,
            count,
            alphabet,
        } => repeat(count, || generators::token(length, alphabet.as_deref()))?,
        GenCommand::Email { domain, count } => repeat(count, || generators::email(&domain))?,
        GenCommand::Name { count } => repeat(count, || Ok(generators::name()))?,
        GenCommand::Lorem { words } => vec![generators::lorem(words)?],
    };
    text_out(values.join("\n"), InputOptions::default())
}

fn dispatch_br(args: BrArgs) -> Result<Outcome> {
    match args.command {
        None => text_out(
            serde_json::to_string_pretty(&countries::br::profile()?).map_err(message)?,
            InputOptions::default(),
        ),
        Some(BrCommand::Cpf(args)) => {
            dispatch_br_document(args, countries::br::cpf, countries::br::validate_cpf)
        }
        Some(BrCommand::Cnpj(args)) => {
            dispatch_br_document(args, countries::br::cnpj, countries::br::validate_cnpj)
        }
        Some(BrCommand::Cep(args)) => text_out(
            repeat(args.count, || Ok(countries::br::cep(args.formatted)))?.join("\n"),
            InputOptions::default(),
        ),
        Some(BrCommand::Phone(args)) => text_out(
            repeat(args.count, || Ok(countries::br::phone(args.formatted)))?.join("\n"),
            InputOptions::default(),
        ),
        Some(BrCommand::Pix { kind, count }) => text_out(
            repeat(count, || countries::br::pix(&kind))?.join("\n"),
            InputOptions::default(),
        ),
    }
}

fn dispatch_br_document(
    args: BrDocumentArgs,
    generate: fn(bool) -> String,
    validate: fn(&str) -> bool,
) -> Result<Outcome> {
    if let Some(value) = args.validate {
        status_out(validate(&value))
    } else {
        text_out(
            repeat(args.count, || Ok(generate(args.formatted)))?.join("\n"),
            InputOptions::default(),
        )
    }
}

fn dispatch_json(command: JsonCommand) -> Result<Outcome> {
    match command {
        JsonCommand::Pretty(input) => text_out(data::json_pretty(&read_text(&input)?)?, input),
        JsonCommand::Minify(input) => text_out(data::json_minify(&read_text(&input)?)?, input),
        JsonCommand::Validate(input) => {
            data::parse_json(&read_text(&input)?)?;
            status_with_input(true, input)
        }
        JsonCommand::Escape(input) => text_out(data::json_escape(&read_text(&input)?)?, input),
        JsonCommand::Unescape(input) => text_out(data::json_unescape(&read_text(&input)?)?, input),
        JsonCommand::SortKeys(input) => text_out(data::json_sort_keys(&read_text(&input)?)?, input),
        JsonCommand::Flatten(input) => text_out(data::json_flatten(&read_text(&input)?)?, input),
        JsonCommand::Unflatten(input) => {
            text_out(data::json_unflatten(&read_text(&input)?)?, input)
        }
        JsonCommand::Path { expression, input } => {
            text_out(data::json_query(&read_text(&input)?, &expression)?, input)
        }
        JsonCommand::Diff(args) => {
            let (left, right) = read_diff(&args)?;
            text_out(
                data::json_diff(&left, &right, args.patch)?,
                InputOptions::default(),
            )
        }
        JsonCommand::ToYaml(input) => text_out(data::json_to_yaml(&read_text(&input)?)?, input),
        JsonCommand::ToCsv {
            input,
            stringify_nested,
        } => text_out(
            data::json_to_csv(&read_text(&input)?, stringify_nested)?,
            input,
        ),
        JsonCommand::ToToml(input) => text_out(data::json_to_toml(&read_text(&input)?)?, input),
        JsonCommand::SchemaValidate { schema, input } => {
            let schema_text = fs::read_to_string(&schema).map_err(|source| VutilsError::Read {
                path: schema,
                source,
            })?;
            data::validate_json_schema(&read_text(&input)?, &schema_text)?;
            status_with_input(true, input)
        }
    }
}

fn dispatch_yaml(command: YamlCommand) -> Result<Outcome> {
    match command {
        YamlCommand::Pretty(input) => text_out(data::yaml_pretty(&read_text(&input)?)?, input),
        YamlCommand::Validate(input) => {
            data::yaml_pretty(&read_text(&input)?)?;
            status_with_input(true, input)
        }
        YamlCommand::ToJson(input) => text_out(data::yaml_to_json(&read_text(&input)?)?, input),
        YamlCommand::Split { input, output_dir } => {
            let documents = data::yaml_split(&read_text(&input)?)?;
            if let Some(directory) = output_dir {
                fs::create_dir_all(&directory).map_err(|source| VutilsError::Write {
                    path: directory.clone(),
                    source,
                })?;
                let paths: Vec<_> = (1..=documents.len())
                    .map(|index| directory.join(format!("document-{index:03}.yaml")))
                    .collect();
                if let Some(path) = paths.iter().find(|path| path.exists()) {
                    return Err(VutilsError::Write {
                        path: path.clone(),
                        source: std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "refusing to overwrite an existing YAML document",
                        ),
                    });
                }
                for (path, document) in paths.iter().zip(&documents) {
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .map_err(|source| VutilsError::Write {
                            path: path.clone(),
                            source,
                        })?;
                    file.write_all(document.as_bytes())
                        .map_err(|source| VutilsError::Write {
                            path: path.clone(),
                            source,
                        })?;
                }
                text_out(
                    format!("wrote {} documents", documents.len()),
                    InputOptions::default(),
                )
            } else {
                text_out(
                    serde_json::to_string_pretty(&documents).map_err(message)?,
                    input,
                )
            }
        }
        YamlCommand::Join { files } => {
            if files.is_empty() {
                return Err(VutilsError::InvalidInput(
                    "yaml join requires at least one file".into(),
                ));
            }
            let documents = files
                .into_iter()
                .map(|path| {
                    fs::read_to_string(&path).map_err(|source| VutilsError::Read { path, source })
                })
                .collect::<Result<Vec<_>>>()?;
            text_out(data::yaml_join(&documents)?, InputOptions::default())
        }
    }
}

fn dispatch_dotenv(command: DotenvCommand) -> Result<Outcome> {
    match command {
        DotenvCommand::Parse(input) => text_out(data::dotenv_to_json(&read_text(&input)?)?, input),
        DotenvCommand::Validate(input) => {
            data::dotenv_parse(&read_text(&input)?)?;
            status_with_input(true, input)
        }
        DotenvCommand::Sort(input) => text_out(data::dotenv_sort(&read_text(&input)?)?, input),
        DotenvCommand::Diff { diff, show_values } => {
            let (left, right) = read_diff(&diff)?;
            text_out(
                data::dotenv_diff(&left, &right, show_values)?,
                InputOptions::default(),
            )
        }
    }
}

fn dispatch_curl(command: CurlCommand) -> Result<Outcome> {
    match command {
        CurlCommand::Format { shell, input } => text_out(
            http::format_curl(&read_text(&input)?, map_shell(shell))?,
            input,
        ),
    }
}

fn dispatch_sql(command: SqlCommand, config: &UserConfig) -> Result<Outcome> {
    match command {
        SqlCommand::Format(args) => {
            let uppercase = match args.keyword_case {
                KeywordCaseArg::Upper => Some(true),
                KeywordCaseArg::Lower => Some(false),
                KeywordCaseArg::Preserve => None,
            };
            text_out(
                sql::format_sql(
                    &read_text(&args.common.input)?,
                    resolve_sql_dialect(args.common.dialect, config)?,
                    uppercase,
                    args.indent,
                    false,
                )?,
                args.common.input,
            )
        }
    }
}

fn dispatch_text(command: TextCommand) -> Result<Outcome> {
    match command {
        TextCommand::Case { style, input } => text_out(
            text::convert_case(&read_text(&input)?, map_case(style)),
            input,
        ),
        TextCommand::Slug(input) => text_out(text::slugify(&read_text(&input)?), input),
        TextCommand::Trim(input) => text_out(read_text(&input)?.trim().to_owned(), input),
        TextCommand::SortLines {
            unique,
            descending,
            input,
        } => text_out(
            text::sort_lines(&read_text(&input)?, unique, descending),
            input,
        ),
        TextCommand::UniqueLines(input) => text_out(text::unique_lines(&read_text(&input)?), input),
        TextCommand::NormalizeEol { crlf, input } => {
            text_out(text::normalize_eol(&read_text(&input)?, crlf), input)
        }
        TextCommand::Diff(args) => {
            let (left, right) = read_diff(&args)?;
            text_out(text::text_diff(&left, &right), InputOptions::default())
        }
        TextCommand::Unicode(input) => text_out(text::unicode_inspect(&read_text(&input)?)?, input),
        TextCommand::OnlyDigits(input) => text_out(text::only_digits(&read_text(&input)?), input),
    }
}

fn dispatch_password_hash(command: PasswordHashCommand) -> Result<Outcome> {
    match command {
        PasswordHashCommand::Argon2Hash { secret } => text_out(
            security::argon2_hash(&read_secret(&secret)?)?,
            InputOptions::default(),
        ),
        PasswordHashCommand::Argon2Verify { encoded, secret } => {
            status_out(security::argon2_verify(&read_secret(&secret)?, &encoded)?)
        }
        PasswordHashCommand::BcryptHash { cost, secret } => text_out(
            security::bcrypt_hash(&read_secret(&secret)?, cost)?,
            InputOptions::default(),
        ),
        PasswordHashCommand::BcryptVerify { encoded, secret } => {
            status_out(security::bcrypt_verify(&read_secret(&secret)?, &encoded)?)
        }
    }
}

fn dispatch_totp(command: TotpCommand) -> Result<Outcome> {
    match command {
        TotpCommand::GenerateSecret { bytes } => text_out(
            security::generate_totp_secret(bytes)?,
            InputOptions::default(),
        ),
        TotpCommand::Code {
            secret,
            algorithm,
            digits,
            period,
            timestamp,
        } => text_out(
            security::totp_code(
                &String::from_utf8(read_secret(&secret)?)
                    .map_err(|_| VutilsError::InvalidInput("TOTP secret must be UTF-8".into()))?,
                map_totp(algorithm),
                digits,
                period,
                timestamp,
            )?,
            InputOptions::default(),
        ),
        TotpCommand::Verify {
            code,
            secret,
            algorithm,
            digits,
            period,
            timestamp,
            window,
        } => status_out(security::verify_totp(
            &String::from_utf8(read_secret(&secret)?)
                .map_err(|_| VutilsError::InvalidInput("TOTP secret must be UTF-8".into()))?,
            &code,
            map_totp(algorithm),
            digits,
            period,
            timestamp,
            window,
        )?),
    }
}

fn dispatch_time(command: TimeCommand) -> Result<Outcome> {
    match command {
        TimeCommand::Now { unix, unit, utc } => text_out(
            if unix {
                time::now(map_time_unit(unit.unwrap_or(TimeUnitArg::Seconds))).to_string()
            } else {
                time::now_rfc3339(map_output_timezone(utc))
            },
            InputOptions::default(),
        ),
        TimeCommand::ToIso { value, unit, utc } => text_out(
            time::unix_to_rfc3339(value, map_time_unit(unit), map_output_timezone(utc))?,
            InputOptions::default(),
        ),
        TimeCommand::ToUnix { value, unit } => text_out(
            time::rfc3339_to_unix(&value, map_time_unit(unit))?.to_string(),
            InputOptions::default(),
        ),
        TimeCommand::Duration { value } => text_out(
            time::parse_duration(&value)?.to_string(),
            InputOptions::default(),
        ),
    }
}

fn dispatch_semver(command: SemverCommand) -> Result<Outcome> {
    match command {
        SemverCommand::Compare { left, right } => {
            let left = Version::parse(&left)
                .map_err(|error| VutilsError::InvalidInput(error.to_string()))?;
            let right = Version::parse(&right)
                .map_err(|error| VutilsError::InvalidInput(error.to_string()))?;
            text_out(
                match left.cmp(&right) {
                    std::cmp::Ordering::Less => "less",
                    std::cmp::Ordering::Equal => "equal",
                    std::cmp::Ordering::Greater => "greater",
                }
                .into(),
                InputOptions::default(),
            )
        }
        SemverCommand::Sort { versions } => {
            let mut parsed = versions
                .into_iter()
                .map(|value| {
                    Version::parse(&value).map_err(|error| {
                        VutilsError::InvalidInput(format!("invalid version `{value}`: {error}"))
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            parsed.sort();
            text_out(
                parsed
                    .into_iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                InputOptions::default(),
            )
        }
        SemverCommand::Bump { value, kind } => {
            text_out(time::semver_bump(&value, &kind)?, InputOptions::default())
        }
    }
}

fn completion(shell: CompletionShell) -> Result<Outcome> {
    let mut command = Cli::command();
    let mut output = Vec::new();
    match shell {
        CompletionShell::Bash => clap_complete::generate(
            clap_complete::shells::Bash,
            &mut command,
            "vutils",
            &mut output,
        ),
        CompletionShell::Zsh => clap_complete::generate(
            clap_complete::shells::Zsh,
            &mut command,
            "vutils",
            &mut output,
        ),
        CompletionShell::Fish => clap_complete::generate(
            clap_complete::shells::Fish,
            &mut command,
            "vutils",
            &mut output,
        ),
        CompletionShell::Powershell => clap_complete::generate(
            clap_complete::shells::PowerShell,
            &mut command,
            "vutils",
            &mut output,
        ),
        CompletionShell::Elvish => clap_complete::generate(
            clap_complete::shells::Elvish,
            &mut command,
            "vutils",
            &mut output,
        ),
    }
    binary_out(output, InputOptions::default())
}

fn man_page() -> Result<Outcome> {
    let mut output = Vec::new();
    clap_mangen::Man::new(Cli::command())
        .render(&mut output)
        .map_err(VutilsError::from)?;
    binary_out(output, InputOptions::default())
}

fn read_bytes(input: &InputOptions) -> Result<Vec<u8>> {
    vutils::io::read_input(&map_input(input))
}
fn read_text(input: &InputOptions) -> Result<String> {
    vutils::io::read_text(&map_input(input))
}
fn map_input(input: &InputOptions) -> InputArgs {
    if !input.literal
        && input.input.is_none()
        && let Some(path) = input.value.as_deref().map(PathBuf::from)
        && path.is_file()
    {
        return InputArgs {
            value: None,
            input: Some(path),
        };
    }

    InputArgs {
        value: input.value.clone(),
        input: input.input.clone(),
    }
}

fn read_secret(options: &SecretOptions) -> Result<Vec<u8>> {
    if let Some(value) = &options.secret {
        return Ok(value.as_bytes().to_vec());
    }
    if let Some(path) = &options.secret_file {
        let mut value = fs::read(path).map_err(|source| VutilsError::Read {
            path: path.clone(),
            source,
        })?;
        trim_line_ending(&mut value);
        return Ok(value);
    }
    if let Some(name) = &options.secret_env {
        return env::var(name).map(String::into_bytes).map_err(|_| {
            VutilsError::InvalidInput(format!(
                "environment variable `{name}` is not set or is not Unicode"
            ))
        });
    }
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(VutilsError::InvalidInput(
            "provide a secret using stdin, --secret-file, --secret-env, or --secret".into(),
        ));
    }
    let mut value = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut value)?;
    trim_line_ending(&mut value);
    Ok(value)
}

fn read_encryption_key(
    options: EncryptionKeyOptions,
    config: &UserConfig,
) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    if let Some(value) = options.key {
        return Ok(zeroize::Zeroizing::new(value.into_bytes()));
    }
    if let Some(path) = options.key_file {
        let mut value = fs::read(&path).map_err(|source| VutilsError::Read {
            path: path.clone(),
            source,
        })?;
        trim_line_ending(&mut value);
        return Ok(zeroize::Zeroizing::new(value));
    }
    if let Some(name) = options.key_env {
        return read_key_environment(&name);
    }
    if let Some(path) = config.password_file() {
        let mut value = fs::read(&path).map_err(|source| VutilsError::Read {
            path: path.clone(),
            source,
        })?;
        trim_line_ending(&mut value);
        return Ok(zeroize::Zeroizing::new(value));
    }
    if let Some(name) = config.password_env() {
        return read_key_environment(name);
    }
    match key_store::load() {
        Ok(Some(key)) => return Ok(key),
        Ok(None) => {}
        Err(error) => {
            return Err(VutilsError::InvalidInput(format!(
                "no explicit or configured encryption key was provided, and the saved key is unavailable: {error}; use --key, --key-file, or --key-env"
            )));
        }
    }
    Err(VutilsError::InvalidInput(
        "provide an encryption key using --key, --key-file, --key-env (legacy aliases: --passwd, --passwd-file, --passwd-env), configure crypto.password-env/crypto.password-file, or first save a key with a successful enc/dec command".into(),
    ))
}

fn remember_encryption_key(key: &[u8]) {
    if let Err(error) = key_store::remember(key) {
        eprintln!(
            "warning: {error}; the command result is still valid, but this key was not remembered"
        );
    }
}

fn read_key_environment(name: &str) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    env::var(name)
        .map(String::into_bytes)
        .map(zeroize::Zeroizing::new)
        .map_err(|_| {
            VutilsError::InvalidInput(format!(
                "environment variable `{name}` is not set or is not Unicode"
            ))
        })
}

fn trim_line_ending(value: &mut Vec<u8>) {
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
    {
        value.pop();
    }
}

fn read_diff(args: &DiffArgs) -> Result<(String, String)> {
    let read_side = |value: &Option<String>,
                     file: &Option<PathBuf>,
                     name: &str|
     -> Result<String> {
        match (value, file) {
            (Some(value), None) => Ok(value.clone()),
            (None, Some(path)) => fs::read_to_string(path).map_err(|source| VutilsError::Read {
                path: path.clone(),
                source,
            }),
            _ => Err(VutilsError::InvalidInput(format!(
                "provide exactly one of --{name} or --{name}-file"
            ))),
        }
    };
    Ok((
        read_side(&args.left, &args.left_file, "left")?,
        read_side(&args.right, &args.right_file, "right")?,
    ))
}

fn repeat<T>(count: u32, mut operation: impl FnMut() -> Result<T>) -> Result<Vec<T>> {
    validate_count(count)?;
    (0..count).map(|_| operation()).collect()
}

fn validate_count(count: u32) -> Result<()> {
    if !(1..=100_000).contains(&count) {
        return Err(VutilsError::InvalidInput(
            "count must be between 1 and 100000".into(),
        ));
    }
    Ok(())
}

fn text_out(value: String, input: InputOptions) -> Result<Outcome> {
    Ok(Outcome {
        bytes: value.into_bytes(),
        textual: true,
        input: map_input(&input),
        success: true,
    })
}
fn binary_out(value: Vec<u8>, input: InputOptions) -> Result<Outcome> {
    Ok(Outcome {
        bytes: value,
        textual: false,
        input: map_input(&input),
        success: true,
    })
}
fn status_out(valid: bool) -> Result<Outcome> {
    status_with_input(valid, InputOptions::default())
}
fn status_with_input(valid: bool, input: InputOptions) -> Result<Outcome> {
    Ok(Outcome {
        bytes: if valid {
            b"valid".to_vec()
        } else {
            b"invalid".to_vec()
        },
        textual: true,
        input: map_input(&input),
        success: valid,
    })
}
fn message(error: serde_json::Error) -> VutilsError {
    VutilsError::Message(error.to_string())
}
fn fail(error: &VutilsError) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}

fn resolve_uuid_version(
    value: Option<UuidVersionArg>,
    config: &UserConfig,
) -> Result<identifiers::UuidVersion> {
    if let Some(value) = value {
        return Ok(map_uuid_version(value));
    }
    match config.uuid_version() {
        "v1" => Ok(identifiers::UuidVersion::V1),
        "v2" => Ok(identifiers::UuidVersion::V2),
        "v3" => Ok(identifiers::UuidVersion::V3),
        "v4" => Ok(identifiers::UuidVersion::V4),
        "v5" => Ok(identifiers::UuidVersion::V5),
        "v6" => Ok(identifiers::UuidVersion::V6),
        "v7" => Ok(identifiers::UuidVersion::V7),
        "v8" => Ok(identifiers::UuidVersion::V8),
        value => Err(invalid_loaded_config("uuid.version", value)),
    }
}

fn map_uuid_version(value: UuidVersionArg) -> identifiers::UuidVersion {
    match value {
        UuidVersionArg::V1 => identifiers::UuidVersion::V1,
        UuidVersionArg::V2 => identifiers::UuidVersion::V2,
        UuidVersionArg::V3 => identifiers::UuidVersion::V3,
        UuidVersionArg::V4 => identifiers::UuidVersion::V4,
        UuidVersionArg::V5 => identifiers::UuidVersion::V5,
        UuidVersionArg::V6 => identifiers::UuidVersion::V6,
        UuidVersionArg::V7 => identifiers::UuidVersion::V7,
        UuidVersionArg::V8 => identifiers::UuidVersion::V8,
    }
}

fn resolve_uuid_format(
    value: Option<UuidFormatArg>,
    config: &UserConfig,
) -> Result<identifiers::UuidFormat> {
    if let Some(value) = value {
        return Ok(map_uuid_format(value));
    }
    match config.uuid_format() {
        "hyphenated" => Ok(identifiers::UuidFormat::Hyphenated),
        "simple" => Ok(identifiers::UuidFormat::Simple),
        "urn" => Ok(identifiers::UuidFormat::Urn),
        "braced" => Ok(identifiers::UuidFormat::Braced),
        value => Err(invalid_loaded_config("uuid.format", value)),
    }
}

fn map_uuid_format(value: UuidFormatArg) -> identifiers::UuidFormat {
    match value {
        UuidFormatArg::Hyphenated => identifiers::UuidFormat::Hyphenated,
        UuidFormatArg::Simple => identifiers::UuidFormat::Simple,
        UuidFormatArg::Urn => identifiers::UuidFormat::Urn,
        UuidFormatArg::Braced => identifiers::UuidFormat::Braced,
    }
}
fn map_dce_domain(value: DceDomainArg) -> identifiers::DceDomain {
    match value {
        DceDomainArg::Person => identifiers::DceDomain::Person,
        DceDomainArg::Group => identifiers::DceDomain::Group,
        DceDomainArg::Organization => identifiers::DceDomain::Organization,
    }
}
fn map_language(value: LanguageArg) -> codegen::TargetLanguage {
    match value {
        LanguageArg::Rust => codegen::TargetLanguage::Rust,
        LanguageArg::Kotlin => codegen::TargetLanguage::Kotlin,
        LanguageArg::Csharp => codegen::TargetLanguage::CSharp,
        LanguageArg::Typescript => codegen::TargetLanguage::TypeScript,
    }
}
fn map_shell(value: ShellArg) -> http::Shell {
    match value {
        ShellArg::Posix => http::Shell::Posix,
        ShellArg::Powershell => http::Shell::PowerShell,
    }
}
fn resolve_sql_dialect(
    value: Option<SqlDialectArg>,
    config: &UserConfig,
) -> Result<sql::SqlDialect> {
    if let Some(value) = value {
        return Ok(map_sql_dialect(value));
    }
    match config.sql_dialect() {
        "generic" => Ok(sql::SqlDialect::Generic),
        "postgres" => Ok(sql::SqlDialect::PostgreSql),
        "mysql" => Ok(sql::SqlDialect::MySql),
        "sqlite" => Ok(sql::SqlDialect::SQLite),
        "mssql" => Ok(sql::SqlDialect::SqlServer),
        value => Err(invalid_loaded_config("sql.dialect", value)),
    }
}

fn map_sql_dialect(value: SqlDialectArg) -> sql::SqlDialect {
    match value {
        SqlDialectArg::Generic => sql::SqlDialect::Generic,
        SqlDialectArg::Postgres => sql::SqlDialect::PostgreSql,
        SqlDialectArg::Mysql => sql::SqlDialect::MySql,
        SqlDialectArg::Sqlite => sql::SqlDialect::SQLite,
        SqlDialectArg::Mssql => sql::SqlDialect::SqlServer,
    }
}
fn map_case(value: CaseArg) -> text::CaseStyle {
    match value {
        CaseArg::Camel => text::CaseStyle::Camel,
        CaseArg::Pascal => text::CaseStyle::Pascal,
        CaseArg::Snake => text::CaseStyle::Snake,
        CaseArg::Kebab => text::CaseStyle::Kebab,
        CaseArg::Constant => text::CaseStyle::Constant,
        CaseArg::Title => text::CaseStyle::Title,
    }
}
fn map_escape_language(value: EscapeLanguageArg) -> text::EscapeLanguage {
    match value {
        EscapeLanguageArg::Json => text::EscapeLanguage::Json,
        EscapeLanguageArg::Rust => text::EscapeLanguage::Rust,
        EscapeLanguageArg::Kotlin => text::EscapeLanguage::Kotlin,
        EscapeLanguageArg::Java => text::EscapeLanguage::Java,
        EscapeLanguageArg::Csharp => text::EscapeLanguage::CSharp,
        EscapeLanguageArg::Javascript => text::EscapeLanguage::JavaScript,
        EscapeLanguageArg::Typescript => text::EscapeLanguage::TypeScript,
        EscapeLanguageArg::Python => text::EscapeLanguage::Python,
        EscapeLanguageArg::Sql => text::EscapeLanguage::Sql,
        EscapeLanguageArg::PosixShell => text::EscapeLanguage::PosixShell,
    }
}
fn map_hash(value: HashAlgorithmArg) -> security::DigestAlgorithm {
    match value {
        HashAlgorithmArg::Sha256 => security::DigestAlgorithm::Sha256,
        HashAlgorithmArg::Sha512 => security::DigestAlgorithm::Sha512,
    }
}

fn resolve_encryption_algorithm(
    value: Option<EncryptionAlgorithmArg>,
    config: &UserConfig,
) -> Result<security::EncryptionAlgorithm> {
    if let Some(value) = value {
        return Ok(map_encryption_algorithm(value));
    }
    match config.crypto_algorithm() {
        "aes-256-gcm" => Ok(security::EncryptionAlgorithm::Aes256Gcm),
        "xchacha20-poly1305" => Ok(security::EncryptionAlgorithm::XChaCha20Poly1305),
        value => Err(invalid_loaded_config("crypto.algorithm", value)),
    }
}

fn map_encryption_algorithm(value: EncryptionAlgorithmArg) -> security::EncryptionAlgorithm {
    match value {
        EncryptionAlgorithmArg::Aes256Gcm => security::EncryptionAlgorithm::Aes256Gcm,
        EncryptionAlgorithmArg::XChaCha20Poly1305 => {
            security::EncryptionAlgorithm::XChaCha20Poly1305
        }
    }
}

fn invalid_loaded_config(key: &str, value: &str) -> VutilsError {
    VutilsError::InvalidInput(format!(
        "invalid normalized config value `{value}` for `{key}`"
    ))
}
fn map_totp(value: TotpAlgorithmArg) -> security::TotpAlgorithm {
    match value {
        TotpAlgorithmArg::Sha1 => security::TotpAlgorithm::Sha1,
        TotpAlgorithmArg::Sha256 => security::TotpAlgorithm::Sha256,
        TotpAlgorithmArg::Sha512 => security::TotpAlgorithm::Sha512,
    }
}
fn map_time_unit(value: TimeUnitArg) -> time::TimeUnit {
    match value {
        TimeUnitArg::Seconds => time::TimeUnit::Seconds,
        TimeUnitArg::Milliseconds => time::TimeUnit::Milliseconds,
    }
}

fn map_output_timezone(utc: bool) -> time::OutputTimeZone {
    if utc {
        time::OutputTimeZone::Utc
    } else {
        time::OutputTimeZone::Local
    }
}
