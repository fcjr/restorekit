// App-side glue for the privileged helper daemon: register/observe it via
// SMAppService, open the approval UI, and send it commands over XPC.

#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#import <xpc/xpc.h>

static NSString *const kPlistName = @"com.leftshift.restorekit.helper.plist";
static const char *const kServiceName = "com.leftshift.restorekit.helper";

static void fill(char *buf, size_t len, const char *msg) {
    if (!buf || len == 0) return;
    strlcpy(buf, msg ? msg : "", len);
}

// SMAppServiceStatus: 0 notRegistered, 1 enabled, 2 requiresApproval, 3 notFound.
// Returns -1 if SMAppService isn't available (pre-macOS 13).
int32_t rk_helper_status(void) {
    if (@available(macOS 13.0, *)) {
        SMAppService *svc = [SMAppService daemonServiceWithPlistName:kPlistName];
        return (int32_t)svc.status;
    }
    return -1;
}

// Register the daemon. Returns 0 on success (or if already enabled), else fills
// `err` and returns non-zero. First registration leaves it in requiresApproval
// until the user enables it in System Settings.
int32_t rk_helper_register(char *err, size_t err_len) {
    if (@available(macOS 13.0, *)) {
        SMAppService *svc = [SMAppService daemonServiceWithPlistName:kPlistName];
        if (svc.status == SMAppServiceStatusEnabled) return 0;
        NSError *error = nil;
        if ([svc registerAndReturnError:&error]) return 0;
        fill(err, err_len, error.localizedDescription.UTF8String);
        return 1;
    }
    fill(err, err_len, "requires macOS 13 or later");
    return 1;
}

// Open System Settings > General > Login Items so the user can approve it.
void rk_open_login_items_settings(void) {
    if (@available(macOS 13.0, *)) {
        [SMAppService openSystemSettingsLoginItems];
    }
}

// Send a command to the daemon and wait for its reply.
//   0  = success
//   1  = the daemon ran but reported an error (err filled)
//   2  = the daemon is unreachable — not approved / not running (err filled)
int32_t rk_helper_send(const char *command, char *err, size_t err_len) {
    xpc_connection_t conn = xpc_connection_create_mach_service(
        kServiceName, NULL, XPC_CONNECTION_MACH_SERVICE_PRIVILEGED);
    xpc_connection_set_event_handler(conn, ^(xpc_object_t e) {
        (void)e;
    });
    xpc_connection_resume(conn);

    xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(msg, "command", command);

    xpc_object_t reply = xpc_connection_send_message_with_reply_sync(conn, msg);

    int32_t rc;
    if (xpc_get_type(reply) == XPC_TYPE_DICTIONARY) {
        if (xpc_dictionary_get_bool(reply, "ok")) {
            rc = 0;
        } else {
            fill(err, err_len, xpc_dictionary_get_string(reply, "error"));
            rc = 1;
        }
    } else {
        fill(err, err_len, "the helper isn't available — approve it in System Settings");
        rc = 2;
    }

    xpc_connection_cancel(conn);
    return rc;
}
