/* <syslog.h> stub: usbmuxd's log.c can target syslog, absent on Windows. */
#ifndef RK_SYSLOG_H
#define RK_SYSLOG_H

#include <stdarg.h>

#define LOG_EMERG 0
#define LOG_ALERT 1
#define LOG_CRIT 2
#define LOG_ERR 3
#define LOG_WARNING 4
#define LOG_NOTICE 5
#define LOG_INFO 6
#define LOG_DEBUG 7

#define LOG_PID 0x01
#define LOG_CONS 0x02
#define LOG_USER (1 << 3)
#define LOG_DAEMON (3 << 3)

static inline void openlog(const char *ident, int option, int facility)
{
	(void)ident;
	(void)option;
	(void)facility;
}
static inline void closelog(void) {}
static inline void syslog(int priority, const char *format, ...)
{
	(void)priority;
	(void)format;
}
static inline void vsyslog(int priority, const char *format, va_list ap)
{
	(void)priority;
	(void)format;
	(void)ap;
}
static inline int setlogmask(int mask)
{
	(void)mask;
	return 0;
}

#endif /* RK_SYSLOG_H */
