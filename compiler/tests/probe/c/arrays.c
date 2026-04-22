#include <stddef.h>

// int* with len
int sum_range(const int *xs, size_t n) {
    int total = 0;
    for (size_t i = 0; i < n; i++) total += xs[i];
    return total;
}

// subrange via pointer arithmetic
int sum_slice(const int *xs, size_t lo, size_t hi) {
    int total = 0;
    for (size_t i = lo; i < hi; i++) total += xs[i];
    return total;
}

// int[N] decay
int first(int xs[10]) {
    return xs[0];
}

// 2D array
int sum_2d(int m[3][3]) {
    int total = 0;
    for (int i = 0; i < 3; i++)
        for (int j = 0; j < 3; j++)
            total += m[i][j];
    return total;
}

// char buffer / string
size_t my_strlen(const char *s) {
    size_t n = 0;
    while (s[n]) n++;
    return n;
}
