use bace_config::{ConfigError, ServerConfig};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(test)]
mod tests;

const DEFAULT_CONFIG: &str = "server.toml";

#[derive(Parser)]
#[command(
    name = "bace-server",
    version,
    about = "BetterACE host console and authoritative game services"
)]
struct Arguments {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the local authenticated host dashboard (also the default command).
    Host {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    #[command(name = "__host-child", hide = true)]
    HostChild {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        state_directory: PathBuf,
        #[arg(long)]
        generation: u64,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=300))]
        drain_timeout: u64,
    },
    /// Validate configuration without binding sockets or opening a database.
    Check {
        #[arg(long)]
        config: PathBuf,
    },
    /// Exercise the synthetic fixed-step kernel; this is not an AC game server.
    Exercise {
        #[arg(long, default_value_t = 300)]
        ticks: u32,
        #[arg(long, default_value_t = 100)]
        players: u32,
        #[arg(long, default_value_t = 1000)]
        bodies: u32,
    },
    /// Run durable content validation and tick-boundary catalog delivery only.
    ContentWorker {
        #[arg(long)]
        config: PathBuf,
    },
    /// Run verified game services and retain ownership through durable shutdown.
    Serve {
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

pub fn run() -> ExitCode {
    let args = match Arguments::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(code as u8);
        }
    };
    match execute(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn load_run_config(
    explicit: Option<&Path>,
    default_path: &Path,
) -> Result<(ServerConfig, bool), ConfigError> {
    match explicit {
        Some(path) => ServerConfig::load(path).map(|config| (config, false)),
        None => ServerConfig::load_or_create_default(default_path),
    }
}

fn report_created_default(created: bool) {
    if created {
        eprintln!(
            "Created {DEFAULT_CONFIG}. Configure DATs, accepted packs, the RNG key and database access before game readiness."
        );
    }
}

fn execute(args: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    match args.command.unwrap_or(Command::Host { config: None }) {
        Command::Host { config } => {
            let (config, created) = load_run_config(config.as_deref(), Path::new(DEFAULT_CONFIG))?;
            report_created_default(created);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let result = runtime.block_on(crate::supervisor::run_host(config));
            runtime.shutdown_timeout(std::time::Duration::from_secs(2));
            result?;
        }
        Command::HostChild {
            config,
            state_directory,
            generation,
            drain_timeout,
        } => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let config = config
                .map(|path| ServerConfig::load(&path))
                .transpose()?
                .unwrap_or_default();
            let backend = crate::game_host::GameBackend::start(config)?;
            runtime.block_on(crate::supervisor_child::run_child(
                &state_directory,
                generation,
                std::time::Duration::from_secs(drain_timeout),
                backend,
            ))?;
        }
        Command::Check { config } => {
            ServerConfig::load(&config)?;
            println!("Configuration valid. This does not establish server readiness.");
        }
        Command::Exercise {
            ticks,
            players,
            bodies,
        } => crate::exercise::run(ticks, players, bodies)?,
        Command::ContentWorker { config } => {
            let config = ServerConfig::load(&config)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let result = runtime.block_on(crate::worker::run(config));
            runtime.shutdown_timeout(std::time::Duration::from_secs(5));
            result?;
        }
        Command::Serve { config } => {
            let (config, created) = load_run_config(config.as_deref(), Path::new(DEFAULT_CONFIG))?;
            report_created_default(created);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(crate::game_host::serve(config))?;
        }
    }
    Ok(())
}
