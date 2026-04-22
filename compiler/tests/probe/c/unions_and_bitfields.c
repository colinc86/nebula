#include <stdint.h>

// tagged union
typedef enum { T_INT, T_STR } Kind;

typedef struct {
    Kind kind;
    union {
        int i;
        const char *s;
    } val;
} Value;

int get_int(const Value *v) {
    if (v->kind == T_INT) return v->val.i;
    return -1;
}

// bitfields
typedef struct {
    uint32_t r : 8;
    uint32_t g : 8;
    uint32_t b : 8;
    uint32_t a : 8;
} Rgba;

Rgba make_red(void) {
    Rgba c = {255, 0, 0, 255};
    return c;
}
