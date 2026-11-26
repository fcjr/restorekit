use std::io::{IsTerminal, Write};
use std::path::Path;
use std::time::Duration;

use restorekit::device::{self, UsbMode};
use restorekit::progress::Event;
use restorekit::recovery::{self, RunningDevice, Supervisor};
use restorekit::{Error, Result};
use zeroize::Zeroizing;

use super::render;

/// The PKCS #12 archive's password, kept out of the command line (and so out
/// of `ps` and shell history).
const PASSWORD_VAR: &str = "RESTOREKIT_SUPERVISOR_PASSWORD";

/// `restorekit recovery` — ask a running iPhone or iPad to reboot into
/// recovery mode, then wait for it to show up there.
pub fn enter(json: bool, ecid: Option<u64>, supervisor: Option<&Path>) -> Result<()> {
    let supervisor = supervisor.map(load_supervisor).transpose()?;
    let ecid = match ecid {
        Some(ecid) => ecid,
        None => select(recovery::devices()?, json)?,
    };
    if !json {
        println!("Asking {ecid:#x} to reboot into recovery mode...");
    }
    recovery::enter(ecid, supervisor.as_ref())?;

    let device = device::wait_where(Duration::from_secs(90), |d| {
        d.ecid == Some(ecid) && d.mode == UsbMode::Recovery
    })?;
    if json {
        render::emit_json(&Event::DeviceDetected { device });
    } else {
        println!("\n{} is in recovery mode.", device.display_name());
        println!("  ECID: {ecid:#x}");
    }
    Ok(())
}

/// A PEM file with the certificate and unencrypted key, else a PKCS #12
/// archive with its password in [`PASSWORD_VAR`] (empty when unset).
fn load_supervisor(path: &Path) -> Result<Supervisor> {
    let bytes = Zeroizing::new(std::fs::read(path)?);
    if bytes.windows(11).any(|w| w == b"-----BEGIN ") {
        return Supervisor::from_pem(&bytes);
    }
    let password = Zeroizing::new(std::env::var(PASSWORD_VAR).unwrap_or_default());
    Supervisor::from_pkcs12(&bytes, &password)
}

/// The sole running device's ECID, else one picked interactively.
fn select(mut devices: Vec<RunningDevice>, json: bool) -> Result<u64> {
    match devices.len() {
        0 => Err(Error::NoRunningDevice),
        1 => Ok(devices.pop().unwrap().ecid),
        n => {
            if json || !std::io::stdin().is_terminal() {
                return Err(Error::MultipleRunningDevices(n));
            }
            println!("Found {n} running iPhones and iPads:\n");
            for (i, d) in devices.iter().enumerate() {
                println!("  [{}] {} (ECID {:#x})", i + 1, d.display_name(), d.ecid);
            }
            loop {
                print!("\nSelect a device [1-{n}]: ");
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.is_empty() {
                    return Err(Error::MultipleRunningDevices(n));
                }
                match input.trim().parse::<usize>() {
                    Ok(i) if (1..=n).contains(&i) => return Ok(devices[i - 1].ecid),
                    _ => println!("  Enter a number between 1 and {n}."),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_supervisor_loads_from_pem() {
        let dir = std::env::temp_dir().join(format!("rk-supervisor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pem = dir.join("identity.pem");
        let supervisor = Supervisor::generate("Example School").unwrap();
        std::fs::write(&pem, supervisor.to_pem().unwrap()).unwrap();
        let loaded = load_supervisor(&pem).unwrap();
        assert_eq!(
            loaded.certificate().unwrap(),
            supervisor.certificate().unwrap()
        );

        let junk = dir.join("junk");
        std::fs::write(&junk, b"not an identity").unwrap();
        assert!(matches!(
            load_supervisor(&junk),
            Err(Error::SupervisionIdentity(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
