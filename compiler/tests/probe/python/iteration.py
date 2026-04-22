def upto(n):
    total = 0
    for i in range(n):
        total += i
    return total


def indexed(xs):
    out = []
    for i, x in enumerate(xs):
        out.append((i, x))
    return out


def zipped(a, b):
    return [(x, y) for x, y in zip(a, b)]


def stepped(start, stop, step):
    return list(range(start, stop, step))


def reversed_list(xs):
    return list(reversed(xs))
