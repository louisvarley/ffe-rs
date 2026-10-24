/* Forced include for the Windows (MSVC) build of headless.c: the POSIX facilities some
   scenarios use are shimmed or unavailable. The scenarios that need a forked child
   (the terrain and planet fault catchers) report "unsupported" on Windows; the rest
   build and run as on Linux. */
#ifndef FFE_WIN_COMPAT_H
#define FFE_WIN_COMPAT_H
#include <setjmp.h>
#include <stdio.h>
#include <stddef.h>
#define sigjmp_buf jmp_buf
#define sigsetjmp(env, mask) setjmp(env)
#define siglongjmp(env, value) longjmp(env, value)
#define __attribute__(x)
#define ADDR_NO_RANDOMIZE 0x0040000
static int personality(unsigned long mode) { (void)mode; return -1; }
static int execv(const char *path, char **argv) { (void)path; (void)argv; return -1; }
typedef int pid_t;
static FILE *open_memstream(char **buffer, size_t *size) { (void)buffer; (void)size; return 0; }
static int fork(void) { return -1; }
static int waitpid(int child, int *status, int options) { (void)child; (void)status; (void)options; return -1; }
/* The sysgen scenario arms an interval timer as a watchdog against a hang; without it a
   hang would not be interrupted, which has not been seen on these inputs. */
#define SIGALRM 14
#define ITIMER_REAL 0
struct itimerval_part { long tv_sec; long tv_usec; };
struct itimerval { struct itimerval_part it_value; struct itimerval_part it_interval; };
static int setitimer(int which, const struct itimerval *value, struct itimerval *old) { (void)which; (void)value; (void)old; return 0; }
#define WIFEXITED(s) 0
#define WEXITSTATUS(s) 0
#define WIFSIGNALED(s) 0
#define WTERMSIG(s) 0
#endif
