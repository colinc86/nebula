def greet(name, age):
    return f"hi {name}, you are {age}"


def fmt_num(x):
    return f"{x:.2f}"


def repr_point(p):
    return f"({p.x}, {p.y})"


def debug(name, value):
    return f"{name=} {value=}"


def expr_inline(a, b):
    return f"sum={a + b} prod={a * b}"
