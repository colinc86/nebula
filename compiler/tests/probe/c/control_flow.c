#include <stdio.h>

// do-while
int countdown(int n) {
    int steps = 0;
    do {
        n--;
        steps++;
    } while (n > 0);
    return steps;
}

// ternary
int max(int a, int b) {
    return (a > b) ? a : b;
}

// multi-dim array
int sum_matrix(int m[3][3]) {
    int total = 0;
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 3; j++) {
            total += m[i][j];
        }
    }
    return total;
}

// variadic-like via array (simple)
int sum_n(int count, const int *nums) {
    int s = 0;
    for (int i = 0; i < count; i++) s += nums[i];
    return s;
}

// bitwise ops
unsigned int pack_rgb(unsigned int r, unsigned int g, unsigned int b) {
    return (r & 0xff) << 16 | (g & 0xff) << 8 | (b & 0xff);
}
