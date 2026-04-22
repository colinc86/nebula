from functools import wraps


def log(fn):
    @wraps(fn)
    def wrapper(*args, **kwargs):
        print(f"calling {fn.__name__}")
        return fn(*args, **kwargs)
    return wrapper


@log
def add(a, b):
    return a + b


class Box:
    @staticmethod
    def kind():
        return "box"

    @classmethod
    def make(cls):
        return cls()

    @property
    def size(self):
        return 10
