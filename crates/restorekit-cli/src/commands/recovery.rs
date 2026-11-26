use std::io::{IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use restorekit::device::{self, UsbMode};
use restorekit::progress::Event;
use restorekit::recovery::{self, RunningDevice, Supervisor};
use restorekit::{Device, Error, Result};
use zeroize::Zeroizing;

use super::dfu::emit_stage;
use super::render;

/// The PKCS #12 archive's password, kept out of the command line (and so out
/// of `ps` and shell history).
const PASSWORD_VAR: &str = "RESTOREKIT_SUPERVISOR_PASSWORD";

/// How long to wait for the device's user to answer the Trust prompt.
const TRUST_WAIT: Duration = Duration::from_secs(120);

/// `restorekit recovery` — ask a running iPhone or iPad to reboot into
/// recovery mode, then wait for it to show up there.
pub fn enter(json: bool, ecid: Option<u64>, supervisor: Option<&Path>) -> Result<()> {
    let supervisor = supervisor.map(load_supervisor).transpose()?;
    let ecid = match ecid {
        Some(ecid) => ecid,
        None => select(recovery::devices()?, json)?,
    };
    let device = to_recovery(json, ecid, supervisor.as_ref())?;
    if json {
        render::emit_json(&Event::DeviceDetected { device });
    } else {
        println!("\n{} is in recovery mode.", device.display_name());
        println!("  ECID: {ecid:#x}");
    }
    Ok(())
}

/// Take the running iPhone or iPad with this ECID to recovery mode and return
/// it once it shows up there. While it asks its user to trust this computer,
/// keep asking until they answer, for up to [`TRUST_WAIT`].
pub(crate) fn to_recovery(
    json: bool,
    ecid: u64,
    supervisor: Option<&Supervisor>,
) -> Result<Device> {
    let stage = |stage: String| emit_stage(json, Event::RecoveryStage { stage });
    stage(format!("asking {ecid:#x} to reboot into recovery mode"));
    let deadline = Instant::now() + TRUST_WAIT;
    let mut told = false;
    loop {
        match recovery::enter(ecid, supervisor) {
            Ok(()) => break,
            Err(Error::TrustPending) if Instant::now() < deadline => {
                if !told {
                    stage("tap Trust on the device and enter its passcode".into());
                    told = true;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return Err(e),
        }
    }
    stage("waiting for it to show up in recovery mode".into());
    device::wait_where(Duration::from_secs(90), |d| {
        d.ecid == Some(ecid) && d.mode == UsbMode::Recovery
    })
}

/// A PEM file with the certificate and unencrypted key, else a PKCS #12
/// archive with its password in [`PASSWORD_VAR`] (empty when unset).
pub(crate) fn load_supervisor(path: &Path) -> Result<Supervisor> {
    let bytes = Zeroizing::new(std::fs::read(path)?);
    let password = Zeroizing::new(std::env::var(PASSWORD_VAR).unwrap_or_default());
    Supervisor::load(&bytes, &password)
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
