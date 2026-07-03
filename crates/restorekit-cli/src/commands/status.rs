use restorekit::{dfu, Result};

pub fn run(json: bool) -> Result<()> {
    let devices = dfu::list()?;

    if json {
        println!("{}", serde_json::to_string(&devices).unwrap());
        return Ok(());
    }

    if devices.is_empty() {
        println!("No Macs in DFU mode found.");
        if !dfu::host_can_trigger_dfu() {
            println!("\n{}", dfu::manual_dfu_instructions());
        } else {
            println!("Cable a target Mac to the DFU port and run `restorekit dfu`.");
        }
        return Ok(());
    }

    println!(
        "Found {} Mac{} in DFU mode:\n",
        devices.len(),
        if devices.len() == 1 { "" } else { "s" }
    );
    for d in &devices {
        println!("  {}", d.display_name());
        if let Some(id) = d.identifier() {
            println!("    identifier: {id}");
        }
        println!("    chip: CPID:{:04x}  board: BDID:{:02x}", d.cpid, d.bdid);
        println!("    ECID: {}", d.ecid_hex());
        if let Some(srtg) = &d.srtg {
            println!("    iBoot: {srtg}");
        }
        println!();
    }

    Ok(())
}
