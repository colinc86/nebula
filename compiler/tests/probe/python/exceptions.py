class BadInput(Exception):
    pass


def parse_int(s):
    try:
        return int(s)
    except ValueError as e:
        raise BadInput(f"not an int: {s}") from e


def divide(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        return float("inf")
    except (TypeError, ArithmeticError) as e:
        print(f"numeric error: {e}")
        return None
    finally:
        pass


def check_positive(x):
    if x < 0:
        raise ValueError(f"negative: {x}")
    return x
