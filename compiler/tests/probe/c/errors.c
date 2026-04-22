#include <errno.h>
#include <stdio.h>
#include <string.h>

// errno-style error
int read_count(const char *path, int *out) {
    FILE *f = fopen(path, "r");
    if (!f) return errno;
    int n;
    if (fscanf(f, "%d", &n) != 1) {
        fclose(f);
        return -1;
    }
    fclose(f);
    *out = n;
    return 0;
}

// error string
const char *my_strerror(int rc) {
    if (rc == 0) return "ok";
    if (rc == -1) return "parse error";
    return strerror(rc);
}

// assert-like
int must_positive(int n) {
    if (n < 0) {
        fprintf(stderr, "negative: %d\n", n);
        return -1;
    }
    return n;
}
