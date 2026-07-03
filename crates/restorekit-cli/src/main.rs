use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "restorekit", version, about = "DFU-restore Apple Silicon Macs")]
struct Cli {
    /// Emit machine-readable JSON lines instead of human output.
    #[arg(long, global = true)]
    json: bool,

    /// Firmware cache directory (default: $XDG_CONFIG_HOME/restorekit/firmwares).
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,

    /// Verbose output (streams idevicerestore's detailed restore log).
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List Macs currently in DFU mode.
    Status,
    /// Reboot the cabled target Mac into DFU mode (Apple Silicon macOS host, root).
    Dfu,
    /// Reboot the cabled target Mac back into normal mode.
    Reboot,
    /// Resolve and download firmware for the detected (or specified) Mac.
    Download {
        /// Model identifier (e.g. MacBookPro17,1). Defaults to the DFU device.
        #[arg(long)]
        identifier: Option<String>,
        /// Pin a macOS version (e.g. 26.5.2). Defaults to the latest signed build.
        #[arg(long)]
        os_version: Option<String>,
    },
    /// Restore (erase) the Mac in DFU mode.
    Restore(RestoreArgs),
    /// One-shot: trigger DFU, wait, download, and restore.
    Run(RestoreArgs),
    /// Show or manage the firmware cache.
    Cache {
        /// Delete all cached firmware.
        #[arg(long)]
        clear: bool,
        /// Print only the cache directory path.
        #[arg(long)]
        path: bool,
    },
}

#[derive(clap::Args)]
struct RestoreArgs {
    /// Update-style restore that keeps user data instead of erasing.
    #[arg(long)]
    revive: bool,
    /// Restore from a local IPSW instead of downloading.
    #[arg(long)]
    ipsw: Option<PathBuf>,
    /// Pin a macOS version (e.g. 26.5.2).
    #[arg(long)]
    os_version: Option<String>,
    /// Override the detected model identifier.
    #[arg(long)]
    identifier: Option<String>,
    /// Skip the erase confirmation prompt.
    #[arg(long)]
    yes: bool,
}

impl RestoreArgs {
    fn into_opts(
        self,
        cache_dir: Option<PathBuf>,
        json: bool,
        verbose: bool,
    ) -> commands::restore::Opts {
        commands::restore::Opts {
            revive: self.revive,
            ipsw: self.ipsw,
            os_version: self.os_version,
            identifier: self.identifier,
            yes: self.yes,
            cache_dir,
            json,
            verbose,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Status => commands::status::run(cli.json),
        Command::Dfu => commands::dfu::enter(cli.json),
        Command::Reboot => commands::dfu::reboot(cli.json),
        Command::Download {
            identifier,
            os_version,
        } => commands::download::run(identifier, os_version, cli.cache_dir, cli.json),
        Command::Restore(args) => {
            commands::restore::run(args.into_opts(cli.cache_dir, cli.json, cli.verbose))
        }
        Command::Run(args) => {
            commands::restore::run_oneshot(args.into_opts(cli.cache_dir, cli.json, cli.verbose))
        }
        Command::Cache { clear, path } => commands::cache::run(cli.cache_dir, clear, path),
    };

    if let Err(e) = result {
        if cli.json {
            println!(
                "{}",
                serde_json::json!({ "event": "error", "message": e.to_string() })
            );
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(1);
    }
}
