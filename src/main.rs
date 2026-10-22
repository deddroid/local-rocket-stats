// No console window on Windows; command-line use attaches to the parent terminal.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use local_rocket_stats::runtime::{self, Options, Runtime};
use local_rocket_stats::settings::{self, Settings};
use local_rocket_stats::{logging, logln, platform, setup, simulator};

#[derive(Parser)]
#[command(
    name = "local-rocket-stats",
    version,
    about = "Rocket League stats and MMR tracker that reads the game's local data"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// Run without the tray icon (stop with Ctrl+C)
    #[arg(long)]
    no_tray: bool,
    /// Don't open the dashboard in the browser at start
    #[arg(long)]
    no_browser: bool,
    /// Serve the dashboard on the local network too (read-only), e.g. for a phone
    #[arg(long)]
    lan: bool,
    /// Dashboard port
    #[arg(long)]
    port: Option<u16>,
    /// Stats API TCP port of the game
    #[arg(long)]
    game_port: Option<u16>,
    /// Database file
    #[arg(long)]
    db: Option<PathBuf>,
    /// Don't read MMR from the game log
    #[arg(long)]
    no_log: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Enable the game's Stats API (PacketSendRate in TAStatsAPI.ini)
    Setup {
        /// .ini file to edit (default: found automatically)
        #[arg(long)]
        path: Option<PathBuf>,
        /// UpdateState packets per second (1-120)
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=120))]
        rate: u32,
    },
    /// Pretend to be the game, to try the dashboard without it
    Simulate {
        #[arg(long, default_value_t = 49123)]
        game_port: u16,
        #[arg(long, default_value_t = 3)]
        matches: usize,
        /// Speed multiplier
        #[arg(long, default_value_t = 20.0)]
        speed: f64,
    },
}

fn main() -> ExitCode {
    platform::attach_console();
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Setup { path, rate }) => match setup::run(path.as_deref(), rate) {
            Ok(files) => {
                for f in files {
                    println!("OK  {}  (PacketSendRate={rate})", f.display());
                }
                println!("Restart Rocket League for the change to take effect.");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some(Cmd::Simulate {
            game_port,
            matches,
            speed,
        }) => match simulator::run(game_port, matches, speed) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        None => run_app(cli),
    }
}

fn run_app(cli: Cli) -> ExitCode {
    logging::init();
    let settings = Settings::load();
    let port = cli.port.unwrap_or(settings.port);
    let dashboard = runtime::dashboard_url(port, "/");
    let lang = settings.lang();

    // Launching it again just opens the dashboard of the running instance.
    let Some(_lock) = platform::single_instance() else {
        logln!(
            "Local Rocket Stats {}",
            local_rocket_stats::i18n::t(lang, local_rocket_stats::i18n::T::AlreadyRunning)
        );
        runtime::open(&dashboard);
        return ExitCode::SUCCESS;
    };

    let opts = Options {
        db: cli.db.unwrap_or_else(settings::default_db),
        port,
        game_port: cli.game_port.unwrap_or(settings.game_port),
        lan: cli.lan || settings.lan,
        read_logs: !cli.no_log,
    };
    let rt = match Runtime::start(opts, settings) {
        Ok(rt) => rt,
        Err(e) => {
            logln!("Could not start: {e}");
            return ExitCode::FAILURE;
        }
    };
    if !cli.no_browser {
        runtime::open(&dashboard);
    }
    #[cfg(any(target_os = "linux", windows))]
    if !cli.no_tray {
        match local_rocket_stats::tray::run(rt) {
            Ok(()) => return ExitCode::SUCCESS,
            // No tray on this desktop: keep running without it.
            Err(e) => {
                logln!("No system tray available ({e}): running without it, stop with Ctrl+C")
            }
        }
    }
    loop {
        std::thread::park();
    }
}
