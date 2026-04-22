#include <stddef.h>

int upto(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) total += i;
    return total;
}

int while_loop(int n) {
    int i = 0;
    int total = 0;
    while (i < n) {
        total += i;
        i++;
    }
    return total;
}

int do_while_loop(int n) {
    int i = 0;
    int total = 0;
    do {
        total += i;
        i++;
    } while (i < n);
    return total;
}

// nested with break/continue
int find_pair(const int *xs, size_t n, int target) {
    for (size_t i = 0; i < n; i++) {
        for (size_t j = i + 1; j < n; j++) {
            if (xs[i] + xs[j] == target) return (int)(i * 100 + j);
        }
    }
    return -1;
}
