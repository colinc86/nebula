// multi-return via out params
void divmod(int a, int b, int *q, int *r) {
    *q = a / b;
    *r = a % b;
}

// pair via struct return
typedef struct {
    int q;
    int r;
} DivResult;

DivResult divmod_struct(int a, int b) {
    DivResult out = { a / b, a % b };
    return out;
}

// error code + value out
int try_parse(const char *s, int *out) {
    if (!s || !*s) return -1;
    *out = 0;
    for (; *s; s++) {
        if (*s < '0' || *s > '9') return -1;
        *out = *out * 10 + (*s - '0');
    }
    return 0;
}
