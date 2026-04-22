#include <stdio.h>

// OO-like interface via vtable
typedef struct Shape Shape;
typedef struct {
    double (*area)(const Shape *);
    const char *(*name)(const Shape *);
} ShapeVTable;

struct Shape {
    const ShapeVTable *vt;
};

typedef struct {
    Shape base;
    double r;
} Circle;

static double circle_area(const Shape *s) {
    const Circle *c = (const Circle *)s;
    return 3.14159 * c->r * c->r;
}

static const char *circle_name(const Shape *s) { (void)s; return "circle"; }

static const ShapeVTable circle_vt = { circle_area, circle_name };

Circle make_circle(double r) {
    Circle c;
    c.base.vt = &circle_vt;
    c.r = r;
    return c;
}
