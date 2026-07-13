use std::io::{IsTerminal, Write};
use std::time::Duration;

use restorekit::progress::Event;
use restorekit::{device, Device, DfuOutcome, DfuVia, Error, Result, Target};

use super::render;

/// Emit a DFU event: NDJSON in `--json` mode, human stage text otherwise.
pub(crate) fn emit_stage(json: bool, event: Event) {
    if json {
        render::emit_json(&event);
    } else if let Event::DfuTriggerStage { stage } = event {
        println!("  {stage}");
    }
}

/// Macs currently in DFU mode — the devices restorekit can act on.
fn dfu_devices() -> Result<Vec<Device>> {
    Ok(device::list()?
        .into_iter()
        .filter(|d| d.restorable())
        .collect())
}

/// Pick the target device: by ECID when given, otherwise the sole DFU device,
/// otherwise an interactive picker (errors in `--json` / non-TTY mode).
pub(crate) fn select_device(ecid: Option<u64>, json: bool) -> Result<Device> {
    match ecid {
        Some(e) => device::find(Target::Ecid(e)),
        None => select_from(dfu_devices()?, json),
    }
}

fn select_from(mut devices: Vec<Device>, json: bool) -> Result<Device> {
    match devices.len() {
        0 => Err(Error::NoDeviceFound),
        1 => Ok(devices.pop().unwrap()),
        n => {
            if json || !std::io::stdin().is_terminal() {
                return Err(Error::MultipleDevices(n));
            }
            println!("Found {n} Macs in DFU mode:\n");
            for (i, d) in devices.iter().enumerate() {
                println!(
                    "  [{}] {} (ECID {})",
                    i + 1,
                    d.display_name(),
                    d.ecid_hex().unwrap_or_default()
                );
            }
            loop {
                print!("\nSelect a device [1-{n}]: ");
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.is_empty() {
                    // EOF: stdin closed mid-prompt.
                    return Err(Error::MultipleDevices(n));
                }
                match input.trim().parse::<usize>() {
                    Ok(i) if (1..=n).contains(&i) => return Ok(devices.swap_remove(i - 1)),
                    _ => println!("  Enter a number between 1 and {n}."),
                }
            }
        }
    }
}

/// Build the trigger route selector from the CLI flags.
fn via_from(dongle: Option<String>, ecid: Option<u64>, port: Option<i32>) -> DfuVia {
    if let Some(id) = dongle {
        DfuVia::Dongle(id)
    } else if let Some(e) = ecid {
        DfuVia::Ecid(e)
    } else if let Some(p) = port {
        DfuVia::Host(Some(p))
    } else {
        DfuVia::Auto
    }
}

/// `restorekit dfu` — trigger DFU on the cabled target, routing through a
/// dongle or the host, then wait for (and report) the Mac entering DFU.
pub fn enter(
    json: bool,
    dongle: Option<String>,
    ecid: Option<u64>,
    port: Option<i32>,
) -> Result<()> {
    let via = via_from(dongle, ecid, port);
    match restorekit::trigger_dfu(via, Duration::from_secs(30), &mut |e| emit_stage(json, e)) {
        Ok(DfuOutcome::Entered(device)) => {
            if json {
                emit_stage(true, Event::DeviceDetected { device });
            } else {
                println!("\nTarget is now in DFU mode: {}", device.display_name());
                println!("  ECID: {}", device.ecid_hex().unwrap_or_default());
            }
            Ok(())
        }
        Ok(DfuOutcome::Sent) => {
            if json {
                emit_stage(true, Event::Done);
            } else {
                println!(
                    "DFU trigger sent via dongle. No DFU device appeared on this host — if the \
                     target's USB data isn't cabled here, confirm on the Mac's screen."
                );
            }
            Ok(())
        }
        Err(Error::UnsupportedHost(_)) => {
            if !json {
                eprintln!("{}", restorekit::manual_dfu_instructions());
            }
            Err(Error::UnsupportedHost(
                "cannot trigger DFU on this host".into(),
            ))
        }
        Err(e) => Err(e),
    }
}

/// `restorekit reboot` — reboot the cabled target back to normal (dongle/host).
pub fn reboot(
    json: bool,
    dongle: Option<String>,
    ecid: Option<u64>,
    port: Option<i32>,
) -> Result<()> {
    let via = via_from(dongle, ecid, port);
    if !json {
        println!("Rebooting the target...");
    }
    match restorekit::dfu::reboot(via, &mut |e| emit_stage(json, e)) {
        Ok(()) => {
            if json {
                emit_stage(true, Event::Done);
            } else {
                println!("Done. The target should be booting normally.");
            }
            Ok(())
        }
        Err(Error::UnsupportedHost(_)) => {
            if !json {
                eprintln!("{}", restorekit::manual_dfu_instructions());
            }
            Err(Error::UnsupportedHost(
                "cannot control the target from this host".into(),
            ))
        }
        Err(e) => Err(e),
    }
}

/// `restorekit serial` — put the cabled target Mac into serial-console mode and
/// stream its debug UART. Routes through a dongle if one is present (its firmware
/// bridges the SBU UART to a CDC port and keeps it live across reboots), else the
/// host's own port controller (two-Apple-Silicon-Macs case). On the host path we
/// **re-apply serial mode every time the target drops off the port**, so it
/// survives a restore's DFU→recovery→restore reboots (a plain macvdmtool `serial`
/// would go silent after the first reset).
///
/// Unix-only: the console stream is driven through `libc` termios. Non-Unix
/// hosts get the stub below.
#[cfg(unix)]
pub fn serial(
    json: bool,
    dongle: Option<String>,
    ecid: Option<u64>,
    port: Option<i32>,
) -> Result<()> {
    let enter = |quiet: bool| {
        restorekit::dfu::serial(via_from(dongle.clone(), ecid, port), &mut |e| {
            if !quiet {
                emit_stage(json, e)
            }
        })
    };

    let console = match enter(false) {
        Ok(c) => c,
        Err(Error::UnsupportedHost(_)) => {
            if !json {
                eprintln!(
                    "Serial needs a dongle, or an Apple Silicon macOS host cabled to the target's \
                     DFU port with a SuperSpeed USB-C cable (USB-2/charge cables lack the SBU lines)."
                );
            }
            return Err(Error::UnsupportedHost("cannot enter serial mode".into()));
        }
        Err(e) => return Err(e),
    };

    // Host path re-arms serial mode over VDM on each reconnect; a dongle keeps
    // serial live in its own firmware, so we just reconnect its CDC port.
    let (path, host_rearm) = match &console {
        restorekit::SerialConsole::Host => ("/dev/cu.debug-console".to_string(), true),
        restorekit::SerialConsole::Dongle(d) => {
            let ttys = super::dongle::serial_ttys(d);
            let ts = ttys.get(1).cloned().ok_or_else(|| {
                Error::Dongle(format!(
                    "no target-serial tty for dongle {} — is its USB data on this host?",
                    d.serial
                ))
            })?;
            (ts, false)
        }
    };

    if !json {
        println!("\nStreaming {path} (115200 8N1). Ctrl-C to stop.");
        println!(
            "{}\n",
            if host_rearm {
                "Re-arms serial across target reboots — leave it running through the restore."
            } else {
                "The dongle keeps serial live across target reboots — leave it running."
            }
        );
    }

    let cpath = std::ffi::CString::new(path).unwrap();
    let mut buf = [0u8; 4096];
    let mut out = std::io::stdout();
    loop {
        // (Re)open — bounded wait for the device to (re)appear after a reboot.
        let mut fd = -1;
        for _ in 0..15 {
            fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_NOCTTY) };
            if fd >= 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if fd < 0 {
            if host_rearm {
                let _ = enter(true); // target mid-reboot: re-arm and retry.
            }
            continue;
        }
        unsafe {
            let mut tio: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut tio) == 0 {
                libc::cfmakeraw(&mut tio);
                libc::cfsetspeed(&mut tio, libc::B115200);
                libc::tcsetattr(fd, libc::TCSANOW, &tio);
            }
        }
        loop {
            let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n > 0 {
                let _ = out.write_all(&buf[..n as usize]);
                let _ = out.flush();
            } else {
                break; // EOF/error: target went away.
            }
        }
        unsafe { libc::close(fd) };
        if host_rearm {
            let _ = enter(true); // re-arm serial mode before reopening.
        }
    }
}

/// Serial console on non-Unix hosts: not yet wired. The streaming uses `libc`
/// termios and the dongle CDC discovery scans `/dev`, so Windows needs a
/// `serialport`-based path (COM-port enumeration + read) before this can work.
#[cfg(not(unix))]
pub fn serial(
    _json: bool,
    _dongle: Option<String>,
    _ecid: Option<u64>,
    _port: Option<i32>,
) -> Result<()> {
    Err(Error::UnsupportedHost(
        "the serial console isn't available on this host yet (Unix only)".into(),
    ))
}

/// `restorekit probe-ports` — report which host USB-C ports can accept DFU/VDM
/// commands. Enters and leaves each port controller's DBMa debug mode; sends no
/// DFU action, so it's non-destructive (though it can briefly blip a live
/// peripheral on a port).
pub fn probe_ports(json: bool) -> Result<()> {
    let probes = restorekit::dfu::probe_ports(&mut |e| {
        if !json {
            emit_stage(json, e)
        }
    })?;
    if json {
        for p in &probes {
            println!(
                "{}",
                serde_json::json!({
                    "event": "port_probe",
                    "rid": p.rid,
                    "location": p.location,
                    "connected": p.connected,
                    "dfu_capable": matches!(&p.dbma, Ok(s) if s == "DBMa"),
                    "status": p.dbma.as_ref().ok(),
                    "error": p.dbma.as_ref().err(),
                })
            );
        }
        return Ok(());
    }
    println!("\nHost USB-C ports (DFU/VDM capability):\n");
    for p in &probes {
        let loc = p.location.as_deref().unwrap_or("?");
        let conn = if p.connected {
            "device attached"
        } else {
            "empty"
        };
        let verdict = match &p.dbma {
            Ok(s) if s == "DBMa" => "DFU-capable".to_string(),
            Ok(s) => format!("reached debug (status {s})"),
            Err(e) => format!("not capable — {e}"),
        };
        println!("  RID {:<2}  {:<12}  {:<16}  {verdict}", p.rid, loc, conn);
    }
    println!();
    Ok(())
}

/// Ensure a Mac is in DFU mode: return it if already there (with the interactive
/// picker when several are present and no ECID pins one), otherwise trigger
/// entry via a dongle or the host and wait. Shared by `restore`.
pub(crate) fn ensure_present(
    json: bool,
    timeout: Duration,
    dongle: Option<String>,
    ecid: Option<u64>,
) -> Result<Device> {
    // An explicit dongle always routes through it.
    if let Some(id) = dongle {
        return finish_trigger(json, DfuVia::Dongle(id), timeout, ecid);
    }
    // Already in DFU? Use it without re-triggering.
    if let Some(e) = ecid {
        let mut devices = device::list()?;
        device::identify(&mut devices);
        match devices.into_iter().find(|d| d.ecid == Some(e)) {
            Some(dev) if dev.in_dfu() => return Ok(dev),
            Some(dev) if !json => {
                println!(
                    "{} is in {} mode; putting it into DFU...",
                    dev.display_name(),
                    dev.mode
                );
            }
            _ => {}
        }
    } else {
        let present = dfu_devices()?;
        if !present.is_empty() {
            return select_from(present, json);
        }
        // Name a target visible in another mode so the user knows the trigger
        // has something to act on.
        if !json {
            if let Some(d) = device::list()?.iter().find(|d| {
                matches!(
                    d.mode,
                    restorekit::UsbMode::Booted | restorekit::UsbMode::Recovery
                )
            }) {
                println!("Detected {} ({} mode).", d.display_name(), d.mode);
            }
        }
    }

    let via = ecid.map(DfuVia::Ecid).unwrap_or(DfuVia::Auto);
    finish_trigger(json, via, timeout, ecid)
}

/// Run the library trigger and map its outcome for `restore`: the confirmed
/// device, a clear error when a dongle triggered a Mac whose USB data isn't
/// cabled here, or a wait for a manual DFU entry when nothing can trigger.
fn finish_trigger(json: bool, via: DfuVia, timeout: Duration, ecid: Option<u64>) -> Result<Device> {
    match restorekit::trigger_dfu(via, timeout, &mut |e| emit_stage(json, e)) {
        Ok(DfuOutcome::Entered(dev)) => Ok(dev),
        Ok(DfuOutcome::Sent) => Err(Error::Dongle(
            "triggered via dongle, but the target's USB data isn't cabled to this host; \
             connect its USB-C data here to restore"
                .into(),
        )),
        Err(Error::UnsupportedHost(_)) => {
            // No dongle and this host can't trigger: wait for a manual DFU entry.
            if !json {
                eprintln!("{}\n", restorekit::manual_dfu_instructions());
                println!("Waiting for the Mac to enter DFU mode...");
            }
            restorekit::dfu::wait_manual(ecid, timeout)
        }
        Err(e) => Err(e),
    }
}
