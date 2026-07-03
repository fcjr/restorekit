// Root XPC daemon shim for the restorekit privileged helper.
//
// Hosts a Mach XPC service and, for every connection, verifies the peer really
// is our signed app before running anything — otherwise any local process could
// ask root to trigger DFU. The peer check follows the approach in
// https://wojciechregula.blog/post/learn-xpc-exploitation-part-1-broken-cryptography/
// (also used by ghostery-midnight): audit token -> SecCode -> static validity
// against a code-signing requirement, plus hardened-runtime + entitlement checks.
//
// Validated "command" strings are handed to the Rust callback, which runs the
// actual VDM trigger. This file contains no privileged logic of its own.

#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <xpc/xpc.h>
#import <dlfcn.h>
#import <bsm/libbsm.h>

// The Rust handler: runs `command`, returns 0 on success or fills `err` (a
// caller-provided buffer) and returns non-zero on failure.
typedef int32_t (*rk_handler_t)(const char *command, char *err, size_t err_len);

// The one client we trust: our app, signed by our team, on the Apple anchor.
static NSString *const kRequirement =
    @"identifier \"com.leftshift.restorekit.app\" and anchor apple generic and "
    @"certificate leaf[subject.OU] = \"KNBPD99JQM\"";
static const char *const kServiceName = "com.leftshift.restorekit.helper";

// CS flags (osfmk/kern/cs_blobs.h): require the hardened runtime.
static const uint32_t CS_RUNTIME = 0x10000;
// Entitlements that would let an attacker inject into / debug an otherwise
// legitimately-signed app and impersonate it over XPC.
static NSArray *problematicEntitlements(void) {
    return @[
        @"com.apple.security.get-task-allow",
        @"com.apple.security.cs.disable-library-validation",
        @"com.apple.security.cs.allow-dyld-environment-variables",
    ];
}

// The private xpc_connection_get_audit_token, resolved at runtime, with sanity
// checks that it agrees with the public pid/asid accessors.
static BOOL copyAuditToken(xpc_connection_t conn, audit_token_t *out) {
    static void (*fn)(xpc_connection_t, audit_token_t *) = NULL;
    if (fn == NULL) {
        void *libxpc = dlopen("/usr/lib/system/libxpc.dylib", RTLD_LAZY | RTLD_LOCAL);
        if (!libxpc) return NO;
        fn = dlsym(libxpc, "xpc_connection_get_audit_token");
        if (!fn) return NO;
    }
    audit_token_t tok = {0};
    fn(conn, &tok);
    if (audit_token_to_pid(tok) != xpc_connection_get_pid(conn)) return NO;
    if (audit_token_to_asid(tok) != xpc_connection_get_asid(conn)) return NO;
    *out = tok;
    return YES;
}

// Returns nil if the peer is our trusted app, else an error string.
static NSString *validatePeer(xpc_connection_t conn) {
    audit_token_t token;
    if (!copyAuditToken(conn, &token)) return @"no audit token";

    NSData *tokenData = [NSData dataWithBytes:&token length:sizeof(token)];
    NSDictionary *attrs = @{(__bridge NSString *)kSecGuestAttributeAudit : tokenData};

    SecCodeRef code = NULL;
    if (SecCodeCopyGuestWithAttributes(NULL, (__bridge CFDictionaryRef)attrs, kSecCSDefaultFlags,
                                       &code) != errSecSuccess || !code) {
        return @"cannot locate peer code";
    }

    SecStaticCodeRef staticCode = NULL;
    OSStatus s = SecCodeCopyStaticCode(code, kSecCSDefaultFlags, &staticCode);
    if (s != errSecSuccess || !staticCode) {
        CFRelease(code);
        return @"cannot load peer static code";
    }

    NSString *result = nil;
    do {
        // Hardened runtime + no injection/debug entitlements.
        CFDictionaryRef infoRef = NULL;
        if (SecCodeCopySigningInformation(staticCode, kSecCSDynamicInformation, &infoRef) ==
                errSecSuccess &&
            infoRef) {
            NSDictionary *info = (__bridge_transfer NSDictionary *)infoRef;
            uint32_t flags = [info[(__bridge NSString *)kSecCodeInfoFlags] unsignedIntValue];
            if ((flags & CS_RUNTIME) != CS_RUNTIME) {
                result = @"peer is not hardened-runtime signed";
                break;
            }
            NSDictionary *ents = info[(__bridge NSString *)kSecCodeInfoEntitlementsDict];
            for (NSString *bad in problematicEntitlements()) {
                if ([ents[bad] boolValue]) {
                    result = [NSString stringWithFormat:@"peer has entitlement %@", bad];
                    break;
                }
            }
            if (result) break;
        } else {
            result = @"peer has no signing information";
            break;
        }

        // The signing-identity requirement (bundle id + team + Apple anchor),
        // checked against both dynamic and static code.
        SecRequirementRef req = NULL;
        if (SecRequirementCreateWithString((__bridge CFStringRef)kRequirement, kSecCSDefaultFlags,
                                           &req) != errSecSuccess) {
            result = @"bad requirement";
            break;
        }
        if (SecCodeCheckValidity(code, kSecCSDefaultFlags, req) != errSecSuccess) {
            result = @"peer failed dynamic requirement";
        } else if (SecStaticCodeCheckValidity(staticCode, kSecCSDefaultFlags, req) != errSecSuccess) {
            result = @"peer failed static requirement";
        }
        CFRelease(req);
    } while (0);

    CFRelease(staticCode);
    CFRelease(code);
    return result;
}

static void handleMessage(xpc_connection_t conn, xpc_object_t event, rk_handler_t handler) {
    xpc_object_t reply = xpc_dictionary_create_reply(event);
    if (!reply) return;

    NSString *deny = validatePeer(conn);
    if (deny) {
        xpc_dictionary_set_string(reply, "error", deny.UTF8String);
        xpc_connection_send_message(conn, reply);
        return;
    }

    const char *command = xpc_dictionary_get_string(event, "command");
    if (!command) {
        xpc_dictionary_set_string(reply, "error", "no command");
        xpc_connection_send_message(conn, reply);
        return;
    }

    char err[512] = {0};
    int32_t rc = handler(command, err, sizeof(err));
    if (rc == 0) {
        xpc_dictionary_set_bool(reply, "ok", true);
    } else {
        xpc_dictionary_set_string(reply, "error", err[0] ? err : "trigger failed");
    }
    xpc_connection_send_message(conn, reply);
}

// Run the listener forever. Called from the daemon's main().
void rk_daemon_run(rk_handler_t handler) {
    xpc_connection_t listener = xpc_connection_create_mach_service(
        kServiceName, dispatch_get_main_queue(), XPC_CONNECTION_MACH_SERVICE_LISTENER);

    xpc_connection_set_event_handler(listener, ^(xpc_object_t peer) {
        if (xpc_get_type(peer) != XPC_TYPE_CONNECTION) return;
        xpc_connection_t conn = (xpc_connection_t)peer;
        xpc_connection_set_event_handler(conn, ^(xpc_object_t event) {
            if (xpc_get_type(event) == XPC_TYPE_DICTIONARY) {
                handleMessage(conn, event, handler);
            }
        });
        xpc_connection_resume(conn);
    });

    xpc_connection_resume(listener);
    dispatch_main();
}
