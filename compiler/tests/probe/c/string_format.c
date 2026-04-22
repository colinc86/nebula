#include <stdio.h>
#include <stdarg.h>

int format_point(char *buf, size_t n, int x, int y) {
    return snprintf(buf, n, "(%d,%d)", x, y);
}

int greet(char *buf, size_t n, const char *name, int age) {
    return snprintf(buf, n, "hi %s, you are %d", name, age);
}

int fmt_float(char *buf, size_t n, double x) {
    return snprintf(buf, n, "%.2f", x);
}

// variadic
int my_logf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int r = vprintf(fmt, ap);
    va_end(ap);
    return r;
}

// hex format
int fmt_hex(char *buf, size_t n, unsigned int v) {
    return snprintf(buf, n, "0x%08x", v);
}
