/* <sys/time.h> shim: struct timeval (via winsock) + gettimeofday. */
#ifndef RK_SYS_TIME_H
#define RK_SYS_TIME_H

#include "_rk_win_sockcompat.h" /* winsock2.h defines struct timeval */
#include <time.h>

struct timezone {
	int tz_minuteswest;
	int tz_dsttime;
};

static inline int gettimeofday(struct timeval *tv, void *tz)
{
	(void)tz;
	if (tv) {
		FILETIME ft;
		ULARGE_INTEGER u;
		GetSystemTimeAsFileTime(&ft);
		u.LowPart = ft.dwLowDateTime;
		u.HighPart = ft.dwHighDateTime;
		/* 100 ns ticks since 1601 → microseconds since the Unix epoch. */
		unsigned long long usec = (u.QuadPart - 116444736000000000ULL) / 10ULL;
		tv->tv_sec = (long)(usec / 1000000ULL);
		tv->tv_usec = (long)(usec % 1000000ULL);
	}
	return 0;
}

#endif /* RK_SYS_TIME_H */
