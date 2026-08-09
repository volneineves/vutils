mod cli;

use std::{env, fs, io::Write as _, path::PathBuf, process::ExitCode};

use clap::{CommandFactory as _, Parser as _};
use cli::*;
use semver::Version;
use vutils::{
    Result, VutilsError, codec, codegen, data, generators, http, identifiers,
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
    let output = OutputArgs {
        output: cli.output,
        in_place: cli.in_place,
        force: cli.force,
        copy: cli.copy,
    };
    match dispatch(cli.command) {
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
fn dispatch(command: Command) -> Result<Outcome> {
    match command {
        Command::Uuid(args) => {
            validate_count(args.count)?;
            if matches!(args.version, UuidVersionArg::V2)
                && args.node_id.is_some()
                && args.count > 64
            {
                return Err(VutilsError::InvalidInput(
                    "UUID v2 with a fixed node ID is limited to 64 values per batch".into(),
                ));
            }
            let options = identifiers::UuidOptions {
                version: map_uuid_version(args.version),
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
                    if matches!(args.version, UuidVersionArg::V2) && args.node_id.is_some() {
                        item_options.dce_sequence = Some(index as u8);
                    }
                    identifiers::generate_uuid(&item_options)
                        .map(|value| identifiers::format_uuid(&value, map_uuid_format(args.format)))
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
        Command::Validate(command) => {
            let valid = match command {
                ValidateCommand::Cpf { value } => generators::validate_cpf(&value),
                ValidateCommand::Cnpj { value } => generators::validate_cnpj(&value),
            };
            status_out(valid)
        }
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
        Command::Http(command) => dispatch_http(command),
        Command::Curl(command) => dispatch_curl(command),
        Command::Sql(command) => dispatch_sql(command),
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
            CronCommand::Next { expression, count }
            | CronCommand::Explain { expression, count } => text_out(
                time::explain_cron(&expression, count)?,
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
        GenCommand::Cpf { formatted, count } => repeat(count, || Ok(generators::cpf(formatted)))?,
        GenCommand::Cnpj { formatted, count } => repeat(count, || Ok(generators::cnpj(formatted)))?,
        GenCommand::Cep { formatted, count } => repeat(count, || Ok(generators::cep(formatted)))?,
        GenCommand::Phone { formatted, count } => {
            repeat(count, || Ok(generators::phone(formatted)))?
        }
        GenCommand::Email { domain, count } => repeat(count, || generators::email(&domain))?,
        GenCommand::Name { count } => repeat(count, || Ok(generators::name()))?,
        GenCommand::Pix { kind, count } => repeat(count, || generators::pix(&kind))?,
        GenCommand::Lorem { words } => vec![generators::lorem(words)?],
    };
    text_out(values.join("\n"), InputOptions::default())
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

fn dispatch_http(command: HttpCommand) -> Result<Outcome> {
    match command {
        HttpCommand::Build(args) => {
            let mut request = http::HttpRequestSpec::new(&args.method, &args.url)?;
            request.headers = args
                .headers
                .iter()
                .map(|header| {
                    header
                        .split_once(':')
                        .map(|(name, value)| (name.trim().into(), value.trim().into()))
                        .ok_or_else(|| {
                            VutilsError::InvalidInput(format!(
                                "header `{header}` must contain a colon"
                            ))
                        })
                })
                .collect::<Result<_>>()?;
            let body_count = usize::from(args.json.is_some())
                + usize::from(args.data.is_some())
                + usize::from(args.body_file.is_some());
            if body_count > 1 {
                return Err(VutilsError::InvalidInput(
                    "use only one of --json, --data, or --body-file".into(),
                ));
            }
            request.body = if let Some(json) = args.json {
                Some(http::HttpBody::Json(data::parse_json(&json)?))
            } else if let Some(value) = args.data {
                Some(http::HttpBody::Text(value))
            } else {
                args.body_file.map(|path| http::HttpBody::File {
                    path: path.to_string_lossy().into_owned(),
                    binary: true,
                })
            };
            request.follow_redirects = args.follow;
            request.compressed = args.compressed;
            text_out(
                http::render(
                    &request,
                    map_http_renderer(args.render),
                    map_shell(args.shell),
                )?,
                InputOptions::default(),
            )
        }
        HttpCommand::Render {
            renderer,
            shell,
            input,
        } => {
            let request: http::HttpRequestSpec = serde_json::from_str(&read_text(&input)?)
                .map_err(|error| {
                    VutilsError::InvalidInput(format!("invalid HTTP request spec: {error}"))
                })?;
            text_out(
                http::render(&request, map_http_renderer(renderer), map_shell(shell))?,
                input,
            )
        }
        HttpCommand::FromHar {
            entry,
            renderer,
            shell,
            input,
        } => {
            let request = http::request_from_har(&read_text(&input)?, entry)?;
            text_out(
                http::render(&request, map_http_renderer(renderer), map_shell(shell))?,
                input,
            )
        }
        HttpCommand::Status { code } => text_out(
            format!("{code} {}", http::http_status(code)?),
            InputOptions::default(),
        ),
    }
}

fn dispatch_curl(command: CurlCommand) -> Result<Outcome> {
    match command {
        CurlCommand::Parse(input) => text_out(
            serde_json::to_string_pretty(&http::parse_curl(&read_text(&input)?)?)
                .map_err(message)?,
            input,
        ),
        CurlCommand::Format { shell, input } => text_out(
            http::format_curl(&read_text(&input)?, map_shell(shell))?,
            input,
        ),
        CurlCommand::Explain {
            show_secrets,
            input,
        } => text_out(
            http::explain_curl(&read_text(&input)?, show_secrets)?,
            input,
        ),
        CurlCommand::Convert { to, shell, input } => text_out(
            http::render(
                &http::parse_curl(&read_text(&input)?)?,
                map_http_renderer(to),
                map_shell(shell),
            )?,
            input,
        ),
    }
}

fn dispatch_sql(command: SqlCommand) -> Result<Outcome> {
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
                    map_sql_dialect(args.common.dialect),
                    uppercase,
                    args.indent,
                    false,
                )?,
                args.common.input,
            )
        }
        SqlCommand::Minify {
            common,
            strip_comments,
        } => text_out(
            sql::minify_sql(
                &read_text(&common.input)?,
                map_sql_dialect(common.dialect),
                strip_comments,
            )?,
            common.input,
        ),
        SqlCommand::Validate(common) => {
            sql::validate_sql(&read_text(&common.input)?, map_sql_dialect(common.dialect))?;
            status_with_input(true, common.input)
        }
        SqlCommand::Inspect(common) => text_out(
            sql::inspect_sql(&read_text(&common.input)?, map_sql_dialect(common.dialect))?,
            common.input,
        ),
        SqlCommand::Insert {
            table,
            common,
            literal,
            csv,
        } => {
            let raw = read_text(&common.input)?;
            let json_input = if csv { data::csv_to_json(&raw)? } else { raw };
            let generated = sql::generate_insert(
                &table,
                &json_input,
                map_sql_dialect(common.dialect),
                literal,
            )?;
            if literal {
                text_out(generated.sql, common.input)
            } else {
                text_out(
                    serde_json::to_string_pretty(&generated).map_err(message)?,
                    common.input,
                )
            }
        }
        SqlCommand::Update {
            table,
            data,
            where_data,
            dialect,
            literal,
        } => {
            let generated = sql::generate_update(
                &table,
                &data,
                &where_data,
                map_sql_dialect(dialect),
                literal,
            )?;
            if literal {
                text_out(generated.sql, InputOptions::default())
            } else {
                text_out(
                    serde_json::to_string_pretty(&generated).map_err(message)?,
                    InputOptions::default(),
                )
            }
        }
        SqlCommand::Placeholders { target, common } => text_out(
            sql::convert_placeholders(
                &read_text(&common.input)?,
                map_sql_dialect(common.dialect),
                &target,
            )?,
            common.input,
        ),
        SqlCommand::QuoteIdentifier { value, dialect } => text_out(
            sql::quote_identifier(&value, map_sql_dialect(dialect))?,
            InputOptions::default(),
        ),
        SqlCommand::QuoteLiteral { value, dialect } => text_out(
            sql::quote_literal(&serde_json::Value::String(value), map_sql_dialect(dialect))?,
            InputOptions::default(),
        ),
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
        TimeCommand::Now { unit } => text_out(
            time::now(map_time_unit(unit)).to_string(),
            InputOptions::default(),
        ),
        TimeCommand::ToIso { value, unit } => text_out(
            time::unix_to_rfc3339(value, map_time_unit(unit))?,
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
fn map_http_renderer(value: HttpRendererArg) -> http::HttpRenderer {
    match value {
        HttpRendererArg::Curl => http::HttpRenderer::Curl,
        HttpRendererArg::Httpie => http::HttpRenderer::Httpie,
        HttpRendererArg::Fetch => http::HttpRenderer::Fetch,
        HttpRendererArg::Axios => http::HttpRenderer::Axios,
        HttpRendererArg::Json => http::HttpRenderer::Json,
    }
}
fn map_shell(value: ShellArg) -> http::Shell {
    match value {
        ShellArg::Posix => http::Shell::Posix,
        ShellArg::Powershell => http::Shell::PowerShell,
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
