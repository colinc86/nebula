#include <stddef.h>

// function pointer type
typedef int (*binop)(int, int);

int add(int a, int b) { return a + b; }
int sub(int a, int b) { return a - b; }

int apply(binop op, int a, int b) {
    return op(a, b);
}

// higher-order: map over array
void map_int(int *xs, size_t n, int (*f)(int)) {
    for (size_t i = 0; i < n; i++) xs[i] = f(xs[i]);
}

int double_it(int x) { return x * 2; }

// dispatch via table
typedef struct {
    const char *name;
    binop op;
} Entry;

int dispatch(Entry *tbl, size_t n, const char *name, int a, int b) {
    for (size_t i = 0; i < n; i++) {
        if (tbl[i].name[0] == name[0]) return tbl[i].op(a, b);
    }
    return 0;
}
