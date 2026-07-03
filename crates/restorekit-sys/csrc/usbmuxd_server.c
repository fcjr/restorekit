/*
 * usbmuxd_server.c — in-process usbmuxd server shim for restorekit.
 *
 * Replaces usbmuxd's main.c with library-callable functions so the server
 * event loop can run on a background thread inside the restorekit binary.
 *
 * Copyright (C) 2024 restorekit contributors
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#define _DEFAULT_SOURCE
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include "utils.h"   /* struct fdlist — must come before client.h */
#include "client.h"
#include "device.h"
#include "log.h"
#include "usb.h"

/* ── globals that main.c normally provides ────────────────────────── */

int should_exit;
int should_discover;
int use_logfile  = 0;
int no_preflight = 1;   /* always skip preflight — no libimobiledevice dep */

/* ── preflight stubs (device.c references these) ─────────────────── */

void preflight_worker_device_add(struct device_info *info)
{
    /* Without HAVE_LIBIMOBILEDEVICE the real preflight.c would just call
       device_set_visible().  We do the same. */
    device_set_visible(info->id);
    client_device_add(info);
}

void preflight_device_remove_cb(void *data)
{
    (void)data;
}

/* ── private state ───────────────────────────────────────────────── */

static int listenfd = -1;
static char socket_path_buf[256];

/* ── helpers ─────────────────────────────────────────────────────── */

static int create_unix_socket(const char *path)
{
    struct sockaddr_un addr;
    int fd;

    if (strlen(path) >= sizeof(addr.sun_path)) {
        usbmuxd_log(LL_FATAL, "socket path too long: %s", path);
        return -1;
    }

    /* Remove stale socket file. */
    unlink(path);

    fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        usbmuxd_log(LL_FATAL, "socket() failed: %s", strerror(errno));
        return -1;
    }

    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, path, sizeof(addr.sun_path) - 1);

    if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) != 0) {
        usbmuxd_log(LL_FATAL, "bind(%s) failed: %s", path, strerror(errno));
        close(fd);
        return -1;
    }
    chmod(path, 0666);

    /* non-blocking */
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags >= 0)
        fcntl(fd, F_SETFL, flags | O_NONBLOCK);

    if (listen(fd, 64) != 0) {
        usbmuxd_log(LL_FATAL, "listen() failed: %s", strerror(errno));
        close(fd);
        return -1;
    }

    return fd;
}

/* ── public API called from Rust ─────────────────────────────────── */

int restorekit_usbmuxd_start(const char *sock_path)
{
    int res;

    snprintf(socket_path_buf, sizeof(socket_path_buf), "%s", sock_path);

    should_exit    = 0;
    should_discover = 0;

    /* Quiet usbmuxd's own logging — it goes to stderr which is fine. */
    log_level = LL_WARNING;

    client_init();
    device_init();

    usbmuxd_log(LL_NOTICE, "restorekit: initializing embedded usbmuxd");

    res = usb_init();
    if (res < 0) {
        usbmuxd_log(LL_FATAL, "usb_init failed (%d)", res);
        return -1;
    }
    usbmuxd_log(LL_NOTICE, "restorekit: USB init found %d device(s)", res);

    listenfd = create_unix_socket(socket_path_buf);
    if (listenfd < 0)
        return -1;

    usbmuxd_log(LL_NOTICE, "restorekit: embedded usbmuxd listening on %s", socket_path_buf);
    return 0;
}

void restorekit_usbmuxd_run(void)
{
    struct fdlist pollfds;
    fdlist_create(&pollfds);

    sigset_t empty_sigset;
    sigemptyset(&empty_sigset);

    while (!should_exit) {
        int to  = usb_get_timeout();
        int dto = device_get_timeout();
        if (dto < to)
            to = dto;

        fdlist_reset(&pollfds);
        fdlist_add(&pollfds, FD_LISTEN, listenfd, POLLIN);
        usb_get_fds(&pollfds);
        client_get_fds(&pollfds);

        struct timespec tspec;
        tspec.tv_sec  = to / 1000;
        tspec.tv_nsec = (to % 1000) * 1000000;

        int cnt = ppoll(pollfds.fds, pollfds.count, &tspec, &empty_sigset);

        if (cnt == -1) {
            if (errno == EINTR) {
                if (should_exit)
                    break;
                if (should_discover) {
                    should_discover = 0;
                    usb_discover();
                }
            }
        } else if (cnt == 0) {
            if (usb_process() < 0) {
                usbmuxd_log(LL_FATAL, "usb_process() failed");
                break;
            }
            device_check_timeouts();
        } else {
            int done_usb = 0;
            for (int i = 0; i < pollfds.count; i++) {
                if (pollfds.fds[i].revents) {
                    if (!done_usb && pollfds.owners[i] == FD_USB) {
                        if (usb_process() < 0) {
                            usbmuxd_log(LL_FATAL, "usb_process() failed");
                            fdlist_free(&pollfds);
                            return;
                        }
                        done_usb = 1;
                    }
                    if (pollfds.owners[i] == FD_LISTEN) {
                        if (client_accept(listenfd) < 0) {
                            usbmuxd_log(LL_FATAL, "client_accept() failed");
                            fdlist_free(&pollfds);
                            return;
                        }
                    }
                    if (pollfds.owners[i] == FD_CLIENT) {
                        client_process(pollfds.fds[i].fd, pollfds.fds[i].revents);
                    }
                }
            }
        }
    }

    fdlist_free(&pollfds);
}

void restorekit_usbmuxd_stop(void)
{
    should_exit = 1;
}

void restorekit_usbmuxd_cleanup(void)
{
    /*
     * We intentionally skip device_kill_connections() and usb_shutdown() here.
     *
     * device_kill_connections() tries to send TCP RSTs to the device, but after
     * a successful restore the device has already rebooted and the USB handle is
     * stale — attempting to submit transfers on it causes a crash.
     *
     * usb_shutdown() calls libusb_exit(NULL) which tears down the *shared*
     * default libusb context.  idevicerestore (via libirecovery) also uses the
     * default context, and any lingering state there becomes a use-after-free.
     *
     * Skipping these is safe: the device is gone, and the process either exits
     * shortly or will re-initialise everything for the next restore.
     */
    client_shutdown();

    if (listenfd >= 0) {
        close(listenfd);
        listenfd = -1;
    }
    if (socket_path_buf[0]) {
        unlink(socket_path_buf);
        socket_path_buf[0] = '\0';
    }
}
