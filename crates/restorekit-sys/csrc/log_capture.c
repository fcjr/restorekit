/* Bridges idevicerestore's logger to a Rust sink.
 *
 * idevicerestore logs through logger()/print_func, whose callback takes a
 * va_list. Rust can't consume a va_list on stable, so this trampoline formats
 * the message with vsnprintf and hands a finished string to Rust. It also
 * silences idevicerestore's own stdout/stderr writes so our progress UI owns
 * the terminal and the captured lines can be surfaced in error messages. */

#include <stdarg.h>
#include <stdio.h>

#include "log.h"

/* idevicerestore's print threshold (defined in log.c, not exported in log.h). */
extern enum loglevel print_level;

/* Implemented in Rust (restorekit-sys). */
void restorekit_log_capture(int level, const char *msg);

static void restorekit_log_trampoline(enum loglevel level, const char *fmt, va_list ap)
{
	char buf[4096];
	vsnprintf(buf, sizeof(buf), fmt, ap);
	restorekit_log_capture((int)level, buf);
}

void restorekit_install_log_capture(void)
{
	print_level = LL_DEBUG;                          /* let the sink see every passed level */
	logger_set_logfile("NONE");                      /* stop idevicerestore writing to the terminal */
	logger_set_print_func(restorekit_log_trampoline);
}
