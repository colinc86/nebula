from typing import Generic, TypeVar

T = TypeVar("T")
K = TypeVar("K")
V = TypeVar("V")


def first(xs: list[T]) -> T:
    return xs[0]


def swap(a: T, b: T) -> tuple[T, T]:
    return (b, a)


class Stack(Generic[T]):
    def __init__(self) -> None:
        self.items: list[T] = []

    def push(self, x: T) -> None:
        self.items.append(x)

    def pop(self) -> T:
        return self.items.pop()


class Cache(Generic[K, V]):
    def __init__(self) -> None:
        self.store: dict[K, V] = {}
