def squares(n):
    return [x * x for x in range(n)]


def evens(n):
    return [x for x in range(n) if x % 2 == 0]


def grid():
    return [(i, j) for i in range(3) for j in range(3)]


def word_counts(text):
    return {w: len(w) for w in text.split()}


def unique(xs):
    return {x for x in xs if x is not None}


# generator expression
def sum_positive(xs):
    return sum(x for x in xs if x > 0)
