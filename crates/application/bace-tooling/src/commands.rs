use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_content::WeenieTemplate;
use bace_db_postgres::PgStore;
use bace_persistence::ContentCandidate;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "bace-cli",
    version,
    about = "BetterACE (BetterACEmulator) content and storage maintenance"
)]
pub struct Arguments {
    #[arg(long, default_value = "BACE_DATABASE_URL", global = true)]
    database_url_env: String,
    /// Private installation used only for disposable legacy SQL staging.
    #[arg(long, global = true)]
    mariadb_basedir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile native nested loot/rare profiles into one immutable supplement.
    LootBuild {
        #[arg(long)]
        table: Vec<PathBuf>,
        #[arg(long)]
        rare_profile: Vec<PathBuf>,
        #[arg(long)]
        output_directory: PathBuf,
    },
    /// Journal native profile edits; mapped publication validates and activates them.
    LootPublish {
        #[arg(long)]
        table: Vec<PathBuf>,
        #[arg(long)]
        rare_profile: Vec<PathBuf>,
    },
    /// Sample authored loot using a public synthetic seed, never player randomness.
    LootSample {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 10000)]
        events: u32,
    },
    /// Provision a private RNG key and bind its fingerprint in PostgreSQL.
    RngInit {
        #[arg(long)]
        key_file: PathBuf,
        #[arg(long, default_value_t = 1)]
        version: u32,
    },
    /// Compile a complete legacy world SQL dump into one aggregate .bace pack.
    WorldBuild {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output_directory: PathBuf,
    },
    /// Validate and accept the initial world pack in PostgreSQL.
    WorldActivate {
        #[arg(long)]
        manifest: PathBuf,
        /// Replace derived indexes only; logical source records must match.
        #[arg(long)]
        reindex: bool,
    },
    /// Initialize/start a private local PostgreSQL and apply migrations (Unix).
    LocalDatabase {
        #[arg(long, default_value = ".local/postgres")]
        directory: PathBuf,
    },
    /// Provision host-console credentials once; read a password from piped stdin.
    HostInit {
        #[arg(long, default_value = "state/host")]
        state_directory: PathBuf,
    },
    /// Convert a weenie SQL batch into TOML files and a provenance manifest.
    ImportSql {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output_directory: PathBuf,
    },
    /// Create an ordinary player account; password comes from an environment variable.
    AccountCreate {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "BACE_INITIAL_PASSWORD")]
        password_env: String,
    },
    /// Convert one weenie losslessly between a supported legacy/native format.
    Convert {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum)]
        from: InputFormat,
        #[arg(long, value_enum)]
        to: OutputFormat,
    },
    /// Queue a candidate revision. A running content worker validates/activates it.
    Publish {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, value_enum)]
        format: InputFormat,
    },
    /// Apply the native PostgreSQL schema migrations.
    Migrate,
    /// Show persisted generation and exact content counts without loading payloads.
    ContentStatus,
    /// Validate DAT header/BTree/sector bounds and optionally extract a record.
    DatInspect {
        path: PathBuf,
        #[arg(long)]
        fingerprint: bool,
        #[arg(long)]
        record: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InputFormat {
    Toml,
    AceJson,
    Binary,
    AceSql,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Toml,
    Binary,
}

pub async fn execute(args: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let mariadb_basedir = args
        .mariadb_basedir
        .or_else(|| std::env::var_os("BACE_MARIADB_BASEDIR").map(PathBuf::from));
    match args.command {
        Command::LootBuild {
            table,
            rare_profile,
            output_directory,
        } => crate::loot_tools::build(&table, &rare_profile, &output_directory)?,
        Command::LootPublish {
            table,
            rare_profile,
        } => {
            let store = connect(&args.database_url_env).await?;
            let revision = crate::loot_tools::publish(&table, &rare_profile, &store).await?;
            store.close().await;
            println!(
                "Queued native profile revision {revision}; pending mapped validation, not yet active."
            );
        }
        Command::LootSample { input, events } => crate::loot_tools::sample(&input, events)?,
        Command::RngInit { key_file, version } => {
            let key = bace_runtime::random_keys::initialize_random_key(&key_file, version)?;
            let store = connect(&args.database_url_env).await?;
            store.bind_random_key(version, key.fingerprint).await?;
            store.close().await;
            println!(
                "Private random key version {version} is bound to this database. Keep the key with protected backups; no rates were enabled."
            );
        }
        Command::WorldBuild {
            input,
            output_directory,
        } => {
            crate::world_tools::build(&input, &output_directory, mariadb_basedir.as_deref())?;
        }
        Command::WorldActivate { manifest, reindex } => {
            crate::world_tools::activate(&manifest, &args.database_url_env, reindex).await?;
        }
        Command::LocalDatabase { directory } => {
            let url = bace_db_postgres::initialize_local_database(&directory).await?;
            println!(
                "Local PostgreSQL is ready. Set {}={url}",
                args.database_url_env
            );
        }
        Command::HostInit { state_directory } => {
            provision_host(&state_directory)?;
        }
        Command::ImportSql {
            input,
            output_directory,
        } => {
            let staged = crate::sql_import::stage(&input, mariadb_basedir.as_deref())?;
            crate::sql_import::export(staged, &output_directory)?;
        }
        Command::AccountCreate { name, password_env } => {
            let name = AccountName::parse(&name)?;
            let password = std::env::var(&password_env)
                .map_err(|_| format!("{password_env} must contain the initial password"))?;
            let hash = tokio::task::spawn_blocking(move || {
                PasswordService::new(1)?.hash(password.as_bytes())
            })
            .await??;
            let store = connect(&args.database_url_env).await?;
            let outcome = store
                .create(NewAccount {
                    name,
                    password_hash: hash,
                })
                .await?;
            store.close().await;
            match outcome {
                CreateAccountOutcome::Created(account) => println!(
                    "Created ordinary player account {} (ID {}).",
                    account.name.as_str(),
                    account.id.0
                ),
                CreateAccountOutcome::AlreadyExists => {
                    return Err("account already exists; no password was changed".into());
                }
            }
        }
        Command::Convert {
            input,
            output,
            from,
            to,
        } => {
            let template = read_template(&input, from, mariadb_basedir.as_deref())?;
            let bytes = match to {
                OutputFormat::Toml => bace_content_tools::export(&template)?.into_bytes(),
                OutputFormat::Binary => bace_content_tools::compile_template(&template)?,
            };
            std::fs::write(&output, &bytes)?;
            println!(
                "Converted WCID {}: {} bytes -> {}",
                template.weenie_id,
                bytes.len(),
                output.display()
            );
        }
        Command::Publish { input, format } => {
            let template = read_template(&input, format, mariadb_basedir.as_deref())?;
            let bytes = bace_content_tools::compile_template(&template)?;
            let candidate = ContentCandidate {
                wcid: template.weenie_id,
                class_name: template.class_name,
                weenie_type: i32::try_from(template.weenie_type)?,
                bytes,
            };
            let store = connect(&args.database_url_env).await?;
            let revision = store.insert_candidates(&[candidate]).await?;
            store.close().await;
            println!(
                "Queued candidate revision {revision}; activation requires the content worker. No instance was spawned."
            );
        }
        Command::Migrate => {
            let store = connect(&args.database_url_env).await?;
            store.migrate().await?;
            store.close().await;
            println!("Native PostgreSQL migrations applied.");
        }
        Command::ContentStatus => {
            let store = connect(&args.database_url_env).await?;
            let status = store.content_status().await?;
            let mapped = store.active_generation().await?;
            store.close().await;
            println!(
                "Content journal revision: {}; SQL candidate heads: {}; pending batches: {}; rejected batches: {}",
                status.accepted_revision,
                status.active_templates,
                status.pending_publications,
                status.rejected_publications
            );
            if let Some(mapped) = mapped {
                let manifest = bace_storage_codec::PackManifest::decode(
                    &mapped.manifest_bytes,
                    bace_storage_codec::PackLimits::default(),
                )?;
                println!(
                    "Disk world: pack generation {}; {} base records; {} active .bace files. PostgreSQL stores its manifest metadata.",
                    manifest.generation,
                    manifest.base.record_count,
                    manifest.deltas.len() + 1,
                );
            } else {
                println!("No disk world manifest has been accepted.");
            }
        }
        Command::DatInspect {
            path,
            fingerprint,
            record,
            output,
        } => {
            let mut dat = bace_dat::DatArchive::open(&path)?;
            println!(
                "DAT dataset {} subset {}; {} records; sector size {}",
                dat.header().dataset,
                dat.header().subset,
                dat.records().len(),
                dat.header().block_size
            );
            if fingerprint {
                println!("SHA256 {}", bace_dat::fingerprint(&path)?);
            }
            if let Some(record) = record {
                let id = u32::from_str_radix(record.trim_start_matches("0x"), 16)?;
                let bytes = dat.read(id)?;
                println!("Record {id:08X}: {} bytes", bytes.len());
                if let Some(output) = output {
                    std::fs::write(output, bytes)?;
                }
            } else if output.is_some() {
                return Err("--output requires --record".into());
            }
        }
    }
    Ok(())
}

fn provision_host(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{IsTerminal, Read};
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err("host-init requires a password on piped stdin; use a hidden password prompt and pipe its result".into());
    }
    let mut password = Vec::new();
    stdin.lock().take(1027).read_to_end(&mut password)?;
    if password.ends_with(b"\n") {
        password.pop();
        if password.ends_with(b"\r") {
            password.pop();
        }
    }
    if password.is_empty()
        || password.len() > 1024
        || password.contains(&b'\n')
        || password.contains(&b'\r')
    {
        return Err("host password must be one nonempty line, at most 1024 bytes".into());
    }
    let result = bace_admin::provision_operator(&directory.join("operator.toml"), &password);
    password.fill(0);
    result?;
    println!(
        "BetterACE host credentials created in {}.",
        directory.display()
    );
    Ok(())
}

fn read_template(
    path: &Path,
    format: InputFormat,
    mariadb_basedir: Option<&Path>,
) -> Result<WeenieTemplate, Box<dyn std::error::Error>> {
    // Limit authoring input before reading it into memory. Full SQL dump import
    // uses a separate staged streaming workflow, not this single-weenie command.
    if std::fs::metadata(path)?.len() > 32 * 1024 * 1024 {
        return Err("single-template input exceeds 32 MiB".into());
    }
    match format {
        InputFormat::Toml => Ok(bace_content_tools::parse(&std::fs::read_to_string(path)?)?),
        InputFormat::AceJson => Ok(bace_import::import_weenie_json(&std::fs::read_to_string(
            path,
        )?)?),
        InputFormat::Binary => Ok(bace_content_tools::decode(&std::fs::read(path)?)?),
        InputFormat::AceSql => {
            let mut world = crate::sql_import::stage(path, mariadb_basedir)?;
            if world.weenies.len() != 1 {
                return Err("single-template command requires exactly one weenie; use import-sql for a batch".into());
            }
            world
                .weenies
                .pop()
                .ok_or_else(|| "no weenie produced".into())
        }
    }
}

async fn connect(variable: &str) -> Result<PgStore, Box<dyn std::error::Error>> {
    let url = std::env::var(variable)
        .map_err(|_| format!("{variable} must contain the PostgreSQL URL"))?;
    Ok(PgStore::connect(&url, 2).await?)
}
