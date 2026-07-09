use restorekit::{device, Result};

/// Print the host's USB-C port topology, so users can see which `--port` RIDs
/// exist. No-op on hosts whose topology can't be read (non-macOS, etc.).
fn print_ports() {
    let ports = restorekit::dfu::ports();
    if ports.is_empty() {
        return;
    }
    println!("Host USB-C ports:\n");
    for p in &ports {
        let loc = p.location.as_deref().unwrap_or("(unlabeled)");
        let dfu = if p.dfu { " — DFU-capable" } else { "" };
        println!("  [rid {}] {loc}{dfu}", p.rid);
    }
    println!();
}

/// Show connected RecoverKit dongles and the Mac (if any) cabled to each.
fn print_dongles() {
    let dongles = match restorekit::dongle::list() {
        Ok(d) if !d.is_empty() => d,
        _ => return,
    };
    println!("RecoverKit dongles:\n");
    for d in &dongles {
        match d.attached_device() {
            Ok(Some(dev)) => println!(
                "  {} ({}) — {} [{} mode]",
                d.serial,
                d.product,
                dev.display_name(),
                dev.mode
            ),
            _ => println!("  {} ({}) — no target visible on this host", d.serial, d.product),
        }
    }
    println!();
}

pub fn run(json: bool) -> Result<()> {
    let mut devices = device::list()?;
    // Fill in booted Macs' ECIDs on macOS hosts (best-effort, no-op elsewhere).
    device::identify(&mut devices);

    if json {
        use serde_json::json;
        // Each device carries how it reaches this host: direct (host DFU works),
        // via a dongle (only dongle DFU works), or behind a plain hub (neither).
        let devs: Vec<serde_json::Value> = devices
            .iter()
            .map(|d| {
                let conn = restorekit::dongle::connection_for(d);
                let host_dfu = conn.host_reachable() && d.port.as_ref().is_some_and(|p| p.dfu);
                let mut v = serde_json::to_value(d).unwrap_or_else(|_| json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("connection".into(), json!(conn.kind()));
                    obj.insert("via_dongle".into(), json!(conn.dongle()));
                    obj.insert("host_dfu_capable".into(), json!(host_dfu));
                }
                v
            })
            .collect();
        let host_ports: Vec<serde_json::Value> = restorekit::dfu::ports()
            .into_iter()
            .map(|p| json!({ "rid": p.rid, "location": p.location, "dfu": p.dfu }))
            .collect();
        let dongles: Vec<serde_json::Value> = restorekit::dongle::list()
            .unwrap_or_default()
            .into_iter()
            .map(|d| {
                // `targets` is an array: current dongles drive one target, but
                // multi-port models (RecoverKit Pro) will drive several.
                json!({
                    "serial": d.serial,
                    "product": d.product,
                    "targets": d.attached_device().ok().flatten().into_iter().collect::<Vec<_>>(),
                })
            })
            .collect();
        println!(
            "{}",
            json!({ "devices": devs, "host_ports": host_ports, "dongles": dongles })
        );
        return Ok(());
    }

    if devices.is_empty() {
        println!("No Apple devices found on USB.");
        if !restorekit::host_can_trigger_dfu() {
            println!("\n{}", restorekit::manual_dfu_instructions());
        } else {
            println!("Cable a target Mac to the DFU port and run `restorekit dfu`.");
        }
        print_ports();
        print_dongles();
        return Ok(());
    }

    println!(
        "Found {} Apple device{}:\n",
        devices.len(),
        if devices.len() == 1 { "" } else { "s" }
    );
    let dfu_port = restorekit::dfu::dfu_port_label();
    for d in &devices {
        println!("  {} [{} mode]", d.display_name(), d.mode);
        if let Some(id) = d.identifier() {
            println!("    identifier: {id}");
        }
        if let Some(i) = &d.identity {
            println!("    chip: CPID:{:04x}  board: BDID:{:02x}", i.cpid, i.bdid);
        }
        if let Some(ecid) = d.ecid {
            println!("    ECID: 0x{ecid:x}");
        }
        if let Some(srtg) = d.srtg() {
            println!("    iBoot: {srtg}");
        }
        if d.identity.is_none() && !d.serial.is_empty() {
            println!("    serial: {}", d.serial);
        }
        match restorekit::dongle::connection_for(d) {
            restorekit::Connection::Dongle(id) => {
                let here = d.port.as_ref().and_then(|p| p.location.as_deref());
                match here {
                    Some(loc) => println!("    connection: via dongle {id} on {loc}"),
                    None => println!("    connection: via dongle {id}"),
                }
                println!(
                    "    → host DFU can't reach it through the dongle; `restorekit dfu` triggers it over the dongle"
                );
            }
            restorekit::Connection::Hub => {
                let here = d.port.as_ref().and_then(|p| p.location.as_deref());
                match here {
                    Some(loc) => println!("    connection: behind a USB hub on {loc}"),
                    None => println!("    connection: behind a USB hub"),
                }
                println!("    → no DFU path from this host; cable it straight to the DFU port");
            }
            restorekit::Connection::Direct => {
                if let Some(port) = &d.port {
                    let here = port.location.as_deref().unwrap_or("this port");
                    if port.dfu {
                        println!("    port: {here} [rid {}] (the DFU port)", port.rid);
                    } else {
                        match dfu_port.as_deref() {
                            Some(name) => println!(
                                "    port: {here} [rid {}] — move the cable to {name} to restore",
                                port.rid
                            ),
                            None => {
                                println!("    port: {here} [rid {}] — not the DFU port", port.rid)
                            }
                        }
                    }
                }
            }
        }
        println!();
    }

    print_ports();
    print_dongles();

    if !devices.iter().any(|d| d.restorable()) {
        println!("None are in DFU mode; only a Mac in DFU mode can be restored.");
        if restorekit::host_can_trigger_dfu() {
            println!("Run `restorekit dfu` to put the cabled target into DFU mode.");
        }
    }

    Ok(())
}
