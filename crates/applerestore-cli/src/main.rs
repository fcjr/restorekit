use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "applerestore", version, about = "DFU-restore Apple Silicon Macs")]
struct Cli {
    /// Emit machine-readable JSON lines instead of human output.
    #[arg(long, global = true)]
    json: bool,

    /// Firmware cache directory (default: $XDG_CONFIG_HOME/applerestore/firmwares).
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,

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
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Status => commands::status::run(cli.json),
        Command::Dfu => commands::dfu::enter(cli.json),
        Command::Reboot => commands::dfu::reboot(),
        Command::Download {
            identifier,
            os_version,
        } => commands::download::run(identifier, os_version, cli.cache_dir, cli.json),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
