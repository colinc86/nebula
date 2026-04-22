def describe(x):
    match x:
        case 0:
            return "zero"
        case int() if x > 0:
            return "positive"
        case [a, b]:
            return f"pair {a},{b}"
        case {"kind": "point", "x": px, "y": py}:
            return f"point {px},{py}"
        case _:
            return "other"


def head_tail(xs):
    match xs:
        case []:
            return None
        case [h, *t]:
            return (h, t)


def unpack(d):
    a, b = d["a"], d["b"]
    return a + b
