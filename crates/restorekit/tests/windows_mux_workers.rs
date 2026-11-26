#![cfg(windows)]

use restorekit::usbmuxd::{mux_port, DriverBinding, MuxPort, UsbmuxdGuard};
use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, TcpStream},
    process::{Child, Command, Stdio},
};

#[test]
#[ignore = "requires a free Windows usbmuxd port"]
fn overlapping_workers_share_the_listener() {
    if std::env::var_os("RESTOREKIT_TEST_MUX_WORKER").is_some() {
        return worker();
    }
    let address = (Ipv4Addr::LOCALHOST, 27015);
    assert!(TcpStream::connect(address).is_err(), "mux port is in use");
    let marker = std::env::var_os("RESTOREKIT_SHARED_USBMUXD");

    let (mut first, first_guard) = spawn_worker();
    let (mut second, second_guard) = spawn_worker();
    assert_eq!(std::env::var_os("RESTOREKIT_SHARED_USBMUXD"), marker);

    // The second worker finishes first. The first makes a fresh mux request
    // after the second's guard has gone.
    drop(second.stdin.take());
    assert!(second.wait().unwrap().success());
    drop(second_guard);
    assert!(matches!(mux_port(), MuxPort::Other { pid, .. } if pid == std::process::id()));

    drop(first.stdin.take());
    assert!(first.wait().unwrap().success());
    drop(first_guard);
    assert_eq!(mux_port(), MuxPort::Free);
}

fn spawn_worker() -> (Child, UsbmuxdGuard) {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "overlapping_workers_share_the_listener",
        "--ignored",
        "--nocapture",
    ])
    .env("RESTOREKIT_TEST_MUX_WORKER", "1")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped());
    let guard = UsbmuxdGuard::for_worker(&mut cmd, DriverBinding::External, &mut |_| {})
        .unwrap()
        .expect("expected the embedded mux");
    let mut child = cmd.spawn().unwrap();
    let mut lines = BufReader::new(child.stdout.as_mut().unwrap()).lines();
    assert!(lines.any(|line| line.unwrap().ends_with("mux ready")));
    (child, guard)
}

fn worker() {
    assert_eq!(
        std::env::var("RESTOREKIT_SHARED_USBMUXD").as_deref(),
        Ok("1")
    );
    let query = || {
        restorekit::restore::preflight().unwrap();
        let mut list = std::ptr::null_mut();
        let mut count = 0;
        unsafe {
            assert_eq!(
                restorekit_sys::idevice_get_device_list(&mut list, &mut count),
                restorekit_sys::IDEVICE_E_SUCCESS
            );
            restorekit_sys::idevice_device_list_free(list);
        }
    };
    query();
    println!("mux ready");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_line(&mut String::new()).unwrap();
    query();
}
