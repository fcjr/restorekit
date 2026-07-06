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
    /// List every connected Apple device, with its mode and ECID.
    List,
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
        /// Detect the model from a specific Mac by ECID (hex like 0xc60a812345678,
        /// or decimal) when several are in DFU mode. See `restorekit list`.
        #[arg(long, value_parser = parse_ecid)]
        ecid: Option<u64>,
    },
    /// Restore (erase) the target Mac: triggers DFU entry if needed, then
    /// downloads firmware and restores.
    Restore(RestoreArgs),
    /// Show or manage the firmware cache.
    Cache {
        /// Delete all cached firmware.
        #[arg(long)]
        clear: bool,
        /// Print only the cache directory path.
        #[arg(long)]
        path: bool,
    },
    /// Bind the WinUSB driver so restorekit can reach the cabled Mac (elevates).
    #[cfg(target_os = "windows")]
    SetupDriver {
        /// Internal: this copy was relaunched already elevated.
        #[arg(long, hide = true)]
        elevated: bool,
        /// Internal: file the elevated copy writes its outcome to.
        #[arg(long, hide = true)]
        result_file: Option<PathBuf>,
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
    /// Target a specific Mac by ECID (hex like 0xc60a812345678, or decimal)
    /// when several are in DFU mode. See `restorekit list`.
    #[arg(long, value_parser = parse_ecid)]
    ecid: Option<u64>,
    /// Skip the erase confirmation prompt.
    #[arg(long)]
    yes: bool,
}

/// Parse an ECID: `0x`-prefixed or bare hex, or decimal.
fn parse_ecid(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let parsed = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else if s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse()
    } else {
        u64::from_str_radix(s, 16)
    };
    parsed.map_err(|_| format!("invalid ECID '{s}': expected hex (0x…) or decimal"))
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
            ecid: self.ecid,
            yes: self.yes,
            cache_dir,
            json,
            verbose,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_ecid;

    #[test]
    fn parses_ecid_forms() {
        assert_eq!(parse_ecid("0xC60A812345678").unwrap(), 0xc60a812345678);
        assert_eq!(parse_ecid("0Xc60a812345678").unwrap(), 0xc60a812345678);
        assert_eq!(parse_ecid("12345").unwrap(), 12345);
        assert_eq!(parse_ecid("c60a812345678").unwrap(), 0xc60a812345678);
        assert!(parse_ecid("nope!").is_err());
        assert!(parse_ecid("").is_err());
    }
}

fn main() {
    // Internal: this copy was relaunched elevated to run the restore-mode driver
    // watcher (see restorekit::driver). Handle it before clap and exit.
    #[cfg(target_os = "windows")]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args
            .iter()
            .position(|a| a == restorekit::driver::RESTORE_WATCH_ARG)
        {
            if let Some(liveness) = args.get(i + 1) {
                restorekit::driver::run_restore_mode_watch_worker(std::path::Path::new(liveness));
            }
            return;
        }
    }

    let cli = Cli::parse();
    let result = match cli.command {
        Command::List => commands::list::run(cli.json),
        Command::Dfu => commands::dfu::enter(cli.json),
        Command::Reboot => commands::dfu::reboot(cli.json),
        Command::Download {
            identifier,
            os_version,
            ecid,
        } => commands::download::run(identifier, os_version, ecid, cli.cache_dir, cli.json),
        Command::Restore(args) => {
            commands::restore::run(args.into_opts(cli.cache_dir, cli.json, cli.verbose))
        }
        Command::Cache { clear, path } => commands::cache::run(cli.cache_dir, clear, path),
        #[cfg(target_os = "windows")]
        Command::SetupDriver {
            elevated,
            result_file,
        } => commands::setup_driver::run(cli.json, elevated, result_file),
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
