//! Run the privileged DFU helper as root via macOS Authorization Services.
//!
//! To get **Touch ID** rather than a password prompt, we first acquire the
//! `system.privilege.admin` right with `AuthorizationCopyRights` — that's the
//! modern "unlock to make changes" dialog, which offers Touch ID on capable
//! Macs. Because the authorization then already holds that right,
//! `AuthorizationExecuteWithPrivileges` runs the helper as root without showing
//! its own (legacy, password-only) prompt.
//!
//! `AuthorizationExecuteWithPrivileges` is deprecated but works for an unsigned
//! app; the fully-modern path (a signed `SMAppService` XPC helper) is a follow-up
//! that requires code signing. The helper prints a final `RESULT: ok` /
//! `RESULT: error <msg>` line to stdout, which we read over the authorization
//! pipe (Authorization Services doesn't surface the tool's exit code).

use std::ffi::{c_void, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

type AuthorizationRef = *mut c_void;
type OsStatus = i32;

const ERR_AUTHORIZATION_CANCELED: OsStatus = -60006;

// AuthorizationFlags
const FLAG_INTERACTION_ALLOWED: u32 = 1 << 0;
const FLAG_EXTEND_RIGHTS: u32 = 1 << 1;
const FLAG_PREAUTHORIZE: u32 = 1 << 4;

#[repr(C)]
struct AuthorizationItem {
    name: *const c_char,
    value_length: usize,
    value: *mut c_void,
    flags: u32,
}

#[repr(C)]
struct AuthorizationRights {
    count: u32,
    items: *mut AuthorizationItem,
}

#[link(name = "Security", kind = "framework")]
extern "C" {
    fn AuthorizationCreate(
        rights: *const AuthorizationRights,
        environment: *const AuthorizationRights,
        flags: u32,
        authorization: *mut AuthorizationRef,
    ) -> OsStatus;
    fn AuthorizationCopyRights(
        authorization: AuthorizationRef,
        rights: *const AuthorizationRights,
        environment: *const AuthorizationRights,
        flags: u32,
        authorized_rights: *mut *mut AuthorizationRights,
    ) -> OsStatus;
    fn AuthorizationExecuteWithPrivileges(
        authorization: AuthorizationRef,
        path_to_tool: *const c_char,
        options: u32,
        arguments: *const *mut c_char,
        communications_pipe: *mut *mut libc::FILE,
    ) -> OsStatus;
    fn AuthorizationFree(authorization: AuthorizationRef, flags: u32) -> OsStatus;
}

/// Run `restorekit-dfu-helper <subcommand>` as root, prompting via the modern
/// admin authentication dialog (Touch ID where available).
pub fn run_helper(subcommand: &str) -> Result<(), String> {
    let helper = helper_path()
        .ok_or_else(|| "DFU helper not found (the app bundle is incomplete)".to_string())?;

    let path_c = CString::new(helper.to_string_lossy().as_bytes())
        .map_err(|_| "helper path contains a NUL byte".to_string())?;
    let arg_c = CString::new(subcommand).map_err(|_| "bad subcommand".to_string())?;
    let mut args: [*mut c_char; 2] = [arg_c.as_ptr() as *mut c_char, ptr::null_mut()];

    // The admin right AEWP requires; pre-acquiring it lets the Touch-ID dialog
    // (not AEWP's password dialog) do the authentication.
    let right_name = CString::new("system.privilege.admin").unwrap();

    unsafe {
        let mut auth: AuthorizationRef = ptr::null_mut();
        if AuthorizationCreate(ptr::null(), ptr::null(), 0, &mut auth) != 0 {
            return Err("could not start authorization".to_string());
        }

        let mut item = AuthorizationItem {
            name: right_name.as_ptr(),
            value_length: 0,
            value: ptr::null_mut(),
            flags: 0,
        };
        let rights = AuthorizationRights {
            count: 1,
            items: &mut item,
        };
        let status = AuthorizationCopyRights(
            auth,
            &rights,
            ptr::null(),
            FLAG_INTERACTION_ALLOWED | FLAG_EXTEND_RIGHTS | FLAG_PREAUTHORIZE,
            ptr::null_mut(),
        );
        if status != 0 {
            AuthorizationFree(auth, 0);
            return Err(if status == ERR_AUTHORIZATION_CANCELED {
                "Authorization was cancelled.".to_string()
            } else {
                format!("the DFU trigger could not be authorized ({status})")
            });
        }

        // The authorization already holds system.privilege.admin, so this won't
        // show its own prompt.
        let mut pipe: *mut libc::FILE = ptr::null_mut();
        #[allow(deprecated)]
        let status = AuthorizationExecuteWithPrivileges(
            auth,
            path_c.as_ptr(),
            0,
            args.as_mut_ptr(),
            &mut pipe,
        );
        if status != 0 {
            AuthorizationFree(auth, 0);
            return Err(format!("the DFU helper could not run as root ({status})"));
        }

        let output = read_pipe(pipe);
        AuthorizationFree(auth, 0);
        interpret(&output)
    }
}

/// Read a C `FILE*` to EOF, then close it.
unsafe fn read_pipe(pipe: *mut libc::FILE) -> String {
    let mut out = Vec::new();
    if !pipe.is_null() {
        let mut buf = [0u8; 512];
        loop {
            let n = libc::fread(buf.as_mut_ptr() as *mut c_void, 1, buf.len(), pipe);
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        libc::fclose(pipe);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse the helper's `RESULT:` sentinel line.
fn interpret(output: &str) -> Result<(), String> {
    for line in output.lines().rev() {
        if let Some(rest) = line.trim().strip_prefix("RESULT:") {
            let rest = rest.trim();
            return if rest == "ok" {
                Ok(())
            } else {
                Err(rest.strip_prefix("error").unwrap_or(rest).trim().to_string())
            };
        }
    }
    // No sentinel — the helper likely couldn't start.
    Err("the DFU trigger did not report a result".to_string())
}

/// Locate the bundled helper across dev and bundled layouts.
fn helper_path() -> Option<PathBuf> {
    let arch = std::env::consts::ARCH; // "aarch64"
    let triple_name = format!("restorekit-dfu-helper-{arch}-apple-darwin");
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("restorekit-dfu-helper"));
            candidates.push(dir.join(&triple_name));
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("binaries").join(&triple_name));
    for profile in ["debug", "release"] {
        candidates.push(
            manifest
                .join("../../../target")
                .join(profile)
                .join("restorekit-dfu-helper"),
        );
    }

    candidates.into_iter().find(|p| p.exists())
}
