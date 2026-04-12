"""List helpers."""

from typing import Iterable


def flatten(nested: Iterable) -> list:
    """Flatten one level of nesting from an iterable of iterables."""
    out = []
    for sub in nested:
        for x in sub:
            out.append(x)
    return out


def chunk(xs: list, size: int) -> list:
    """Split xs into consecutive sublists of length size; final chunk may be shorter.
    Returns an empty list if size <= 0 or xs is empty."""
    # TODO: implement
    raise NotImplementedError

def unique(xs: list) -> list:
    """Return distinct elements in first-occurrence order."""
    seen = set()
    out = []
    for x in xs:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out
