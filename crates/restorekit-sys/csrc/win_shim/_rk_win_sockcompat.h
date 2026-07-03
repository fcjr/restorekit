/*
 * POSIX→Winsock compatibility shim for building the vendored usbmuxd daemon on
 * Windows (MinGW). Pulled in via the fake <sys/socket.h> etc. headers in this
 * directory, which sit first on the include path so only the POSIX headers that
 * don't exist on Windows resolve here.
 */
#ifndef RK_WIN_SOCKCOMPAT_H
#define RK_WIN_SOCKCOMPAT_H

#ifdef _WIN32

#include <winsock2.h>
#include <ws2tcpip.h>
#include <stdint.h>
#include <io.h>
#include <process.h>
#include <time.h>

/* usbmuxd's log.c uses localtime_r on the 32-bit tv_sec of Winsock's timeval;
 * bridge the size gap to time_t and use MSVCRT's TLS-backed localtime. Paired
 * with -DHAVE_LOCALTIME_R for the usbmuxd build on Windows. */
static inline struct tm *rk_localtime_r(const long *t, struct tm *out)
{
	time_t tt = (time_t)*t;
	struct tm *r = localtime(&tt);
	if (r && out) {
		*out = *r;
		return out;
	}
	return (struct tm *)0;
}
#define localtime_r(t, o) rk_localtime_r((const long *)(t), (o))

/* usbmuxd uses fclose() for FILE* and close() only on socket fds, so remapping
 * close to closesocket is safe here. */
#define close(fd) closesocket((SOCKET)(fd))
#define poll(fds, n, to) WSAPoll((fds), (n), (to))

#ifndef SHUT_RD
#define SHUT_RD SD_RECEIVE
#endif
#ifndef SHUT_WR
#define SHUT_WR SD_SEND
#endif
#ifndef SHUT_RDWR
#define SHUT_RDWR SD_BOTH
#endif

/* Socket error codes: map the POSIX spellings usbmuxd checks to the WSA ones.
 * On Windows socket calls set the error via WSAGetLastError(), but usbmuxd reads
 * errno; the CRT keeps a compatible errno for some, so also cover EWOULDBLOCK. */
#ifndef EWOULDBLOCK
#define EWOULDBLOCK WSAEWOULDBLOCK
#endif

/* usbmuxd occasionally sleeps in microseconds. */
static inline int rk_usleep(unsigned int usec)
{
	Sleep((usec + 999) / 1000);
	return 0;
}
#define usleep(us) rk_usleep((unsigned int)(us))

/* usbmuxd sets sockets non-blocking with fcntl(F_SETFL, O_NONBLOCK); emulate
 * that with ioctlsocket(). The only fcntl use is toggling non-blocking. */
#ifndef F_GETFL
#define F_GETFL 3
#endif
#ifndef F_SETFL
#define F_SETFL 4
#endif
#ifndef O_NONBLOCK
#define O_NONBLOCK 0x800
#endif
static inline int rk_fcntl(int fd, int cmd, int arg)
{
	if (cmd == F_SETFL) {
		u_long nb = (arg & O_NONBLOCK) ? 1 : 0;
		ioctlsocket((SOCKET)fd, FIONBIO, &nb);
	}
	return 0;
}
#define fcntl(fd, cmd, arg) rk_fcntl((int)(fd), (cmd), (int)(arg))

/* Winsock's [gs]etsockopt take a char* option value; usbmuxd passes int* etc. */
#define setsockopt(s, l, o, v, n) setsockopt((s), (l), (o), (const char *)(v), (n))
#define getsockopt(s, l, o, v, n) getsockopt((s), (l), (o), (char *)(v), (n))

#endif /* _WIN32 */

#endif /* RK_WIN_SOCKCOMPAT_H */
