def make_adder(n):
    return lambda x: x + n


def double_all(xs):
    return list(map(lambda x: x * 2, xs))


def keep_positive(xs):
    return list(filter(lambda x: x > 0, xs))


def compose(f, g):
    return lambda x: f(g(x))


def counter(start):
    n = start

    def inc():
        nonlocal n
        n += 1
        return n
    return inc
