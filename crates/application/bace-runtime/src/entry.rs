use bace_config::ServerConfig;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "bace-server",
    version,
    about = "BetterACE host console and Rust foundations; stock-client serving is not ready"
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
    /// Readiness gate: refuses service until stock-client world support exists.
    Serve {
        #[arg(long)]
        config: PathBuf,
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

fn execute(args: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    match args.command.unwrap_or(Command::Host { config: None }) {
        Command::Host { config } => {
            let config = config
                .map(|path| ServerConfig::load(&path))
                .transpose()?
                .unwrap_or_default();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let result = runtime.block_on(crate::supervisor::run_host(config.host));
            runtime.shutdown_timeout(std::time::Duration::from_secs(2));
            result?;
        }
        Command::HostChild {
            state_directory,
            generation,
            drain_timeout,
        } => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(crate::supervisor_child::run_child(
                &state_directory,
                generation,
                std::time::Duration::from_secs(drain_timeout),
                crate::supervisor_child::FoundationBackend,
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
            ServerConfig::load(&config)?;
            return Err("Not ready: stock-client authentication/world entry, AC motion/BSP collision, and complete world import remain unimplemented. No game socket was opened. See docs/implementation-status.md.".into());
        }
    }
    Ok(())
}
