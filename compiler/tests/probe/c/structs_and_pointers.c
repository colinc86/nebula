#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// struct with typedef
typedef struct {
    int id;
    char name[32];
    double balance;
} Account;

// function pointer type
typedef int (*cmp_fn)(int, int);

// pointer arithmetic / array iteration
int sum_array(const int *xs, size_t n) {
    int total = 0;
    for (size_t i = 0; i < n; i++) {
        total += xs[i];
    }
    return total;
}

// malloc / free
Account *make_account(int id, const char *name) {
    Account *a = (Account *)malloc(sizeof(Account));
    if (a == NULL) return NULL;
    a->id = id;
    strncpy(a->name, name, 31);
    a->name[31] = '\0';
    a->balance = 0.0;
    return a;
}

// enum
typedef enum {
    LEVEL_DEBUG = 0,
    LEVEL_INFO = 1,
    LEVEL_WARN = 2,
    LEVEL_ERROR = 3
} LogLevel;

// switch statement
const char *level_name(LogLevel l) {
    switch (l) {
        case LEVEL_DEBUG: return "debug";
        case LEVEL_INFO: return "info";
        case LEVEL_WARN: return "warn";
        case LEVEL_ERROR: return "error";
        default: return "unknown";
    }
}

// static local + goto cleanup pattern
int open_and_process(const char *path) {
    FILE *f = fopen(path, "r");
    int rc = 0;
    if (f == NULL) { rc = -1; goto done; }
    char buf[128];
    if (fgets(buf, sizeof(buf), f) == NULL) { rc = -2; goto cleanup; }
    rc = (int)strlen(buf);
cleanup:
    fclose(f);
done:
    return rc;
}
