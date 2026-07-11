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
    /// Put the cabled target Mac into DFU mode — via a dongle (any host) or the
    /// host's own port (Apple Silicon macOS, root). See `--dongle` / `--ecid`.
    Dfu(TargetArgs),
    /// Reboot the cabled target Mac back into normal mode (dongle or host).
    Reboot(TargetArgs),
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
    /// Erase and restore the target Mac: triggers DFU entry if needed, then
    /// downloads firmware and restores. This wipes all data on the target.
    Restore(RestoreArgs),
    /// Revive the target Mac: reinstall firmware without erasing user data —
    /// use this to recover a Mac bricked by a failed update.
    Revive(ReviveArgs),
    /// Show or manage the firmware cache.
    Cache {
        /// Delete all cached firmware.
        #[arg(long)]
        clear: bool,
        /// Print only the cache directory path.
        #[arg(long)]
        path: bool,
    },
    /// Inspect RecoverKit dongles (list, live status). Use the top-level `dfu` /
    /// `reboot` with `--dongle` or `--ecid` to act on the cabled Mac.
    Dongle {
        #[command(subcommand)]
        action: DongleAction,
    },
    /// Show, export, or clear the capture/restore history.
    #[cfg(feature = "history")]
    History {
        #[command(subcommand)]
        action: HistoryAction,
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

/// Which target `dfu` / `reboot` should act on, and how to reach it. With no
/// flags: a connected dongle is used if one is present, otherwise the host's
/// own sole DFU-capable port.
#[derive(clap::Args)]
struct TargetArgs {
    /// Trigger via a specific dongle by its id (USB serial, e.g. DL-1A2B3C4D).
    /// See `restorekit dongle list`.
    #[arg(long, conflicts_with_all = ["ecid", "port"])]
    dongle: Option<String>,
    /// Target the Mac with this ECID (hex like 0xc60a812345678, or decimal).
    /// Auto-routes through the dongle it's cabled to, else the host DFU port.
    #[arg(long, value_parser = parse_ecid, conflicts_with = "port")]
    ecid: Option<u64>,
    /// Target a specific host DFU-capable port by its RID (host trigger only).
    #[arg(long)]
    port: Option<i32>,
}

#[derive(Subcommand)]
enum DongleAction {
    /// List connected dongles and what each has cabled to it.
    List,
    /// Show a dongle's live status (target attached, PD state, orientation).
    Status(DongleSelect),
    /// Reboot a dongle into its USB bootloader to update its firmware.
    Bootsel(DongleSelect),
    /// Update a dongle's firmware over USB (a raw image staged to its spare
    /// flash slot; no bootloader mode, no drive appears). With no --file, the
    /// latest published firmware release is fetched and installed if newer.
    Update {
        /// A raw firmware image (.bin) to install, instead of the latest
        /// published release.
        #[arg(long, short)]
        file: Option<std::path::PathBuf>,
        #[command(flatten)]
        select: DongleSelect,
    },
}

/// Which dongle to act on. With no selector, the sole connected dongle is used.
#[derive(clap::Args)]
struct DongleSelect {
    /// The dongle's id (USB serial, e.g. DL-1A2B3C4D). See
    /// `restorekit dongle list`.
    id: Option<String>,
    /// Same as the positional id, for scripting symmetry with `dfu --dongle`.
    #[arg(long, conflicts_with_all = ["ecid", "id"])]
    dongle: Option<String>,
    /// Target the dongle the Mac with this ECID is cabled to (hex like
    /// 0xc60a812345678, or decimal), resolved by USB topology.
    #[arg(long, value_parser = parse_ecid, conflicts_with = "id")]
    ecid: Option<u64>,
}

impl DongleSelect {
    fn into_target(self) -> restorekit::DongleTarget {
        match (self.id.or(self.dongle), self.ecid) {
            (Some(id), _) => restorekit::DongleTarget::Id(id),
            (_, Some(e)) => restorekit::DongleTarget::Ecid(e),
            _ => restorekit::DongleTarget::Auto,
        }
    }
}

#[cfg(feature = "history")]
#[derive(Subcommand)]
enum HistoryAction {
    /// List logged devices, newest first.
    List,
    /// Export the whole history to a CSV file.
    Export {
        /// Destination CSV path.
        path: PathBuf,
    },
    /// Delete all logged history.
    Clear,
}

/// Firmware selection and target arguments shared by `restore` and `revive`.
#[derive(clap::Args)]
struct FirmwareArgs {
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
    /// Trigger DFU entry via a specific dongle by its id (USB serial). Lets you
    /// restore from any host OS. See `restorekit dongle list`.
    #[arg(long)]
    dongle: Option<String>,
}

#[derive(clap::Args)]
struct RestoreArgs {
    #[command(flatten)]
    firmware: FirmwareArgs,
    /// Skip the erase confirmation prompt.
    #[arg(long)]
    yes: bool,
}

#[derive(clap::Args)]
struct ReviveArgs {
    #[command(flatten)]
    firmware: FirmwareArgs,
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

impl FirmwareArgs {
    fn into_opts(
        self,
        revive: bool,
        yes: bool,
        cache_dir: Option<PathBuf>,
        json: bool,
        verbose: bool,
    ) -> commands::restore::Opts {
        commands::restore::Opts {
            revive,
            ipsw: self.ipsw,
            os_version: self.os_version,
            identifier: self.identifier,
            ecid: self.ecid,
            dongle: self.dongle,
            yes,
            cache_dir,
            json,
            verbose,
        }
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
        Command::Dfu(t) => commands::dfu::enter(cli.json, t.dongle, t.ecid, t.port),
        Command::Reboot(t) => commands::dfu::reboot(cli.json, t.dongle, t.ecid, t.port),
        Command::Download {
            identifier,
            os_version,
            ecid,
        } => commands::download::run(identifier, os_version, ecid, cli.cache_dir, cli.json),
        Command::Restore(args) => commands::restore::run(args.firmware.into_opts(
            false,
            args.yes,
            cli.cache_dir,
            cli.json,
            cli.verbose,
        )),
        Command::Revive(args) => commands::restore::run(args.firmware.into_opts(
            true,
            false,
            cli.cache_dir,
            cli.json,
            cli.verbose,
        )),
        Command::Cache { clear, path } => commands::cache::run(cli.cache_dir, clear, path),
        Command::Dongle { action } => match action {
            DongleAction::List => commands::dongle::list(cli.json),
            DongleAction::Status(s) => commands::dongle::status(cli.json, s.into_target()),
            DongleAction::Bootsel(s) => commands::dongle::bootsel(cli.json, s.into_target()),
            DongleAction::Update { file, select } => {
                commands::dongle::update(cli.json, select.into_target(), file.as_deref())
            }
        },
        #[cfg(feature = "history")]
        Command::History { action } => match action {
            HistoryAction::List => commands::history::list(cli.json),
            HistoryAction::Export { path } => commands::history::export(path),
            HistoryAction::Clear => commands::history::clear(),
        },
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
