use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "applerestore", version, about = "DFU-restore Apple Silicon Macs")]
struct Cli {
    /// Emit machine-readable JSON lines instead of human output.
    #[arg(long, global = true)]
    json: bool,

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
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Status => commands::status::run(cli.json),
        Command::Dfu => commands::dfu::enter(cli.json),
        Command::Reboot => commands::dfu::reboot(),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
