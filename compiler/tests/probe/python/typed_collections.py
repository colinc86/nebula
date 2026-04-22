from typing import Dict, List, Optional, Tuple


def sum_ints(xs: list[int]) -> int:
    return sum(xs)


def make_pair(a: int, b: str) -> Tuple[int, str]:
    return (a, b)


def lookup(d: Dict[str, int], key: str) -> Optional[int]:
    return d.get(key)


def merge(a: List[int], b: List[int]) -> List[int]:
    return a + b


def words_by_len(words: list[str]) -> dict[int, list[str]]:
    out: dict[int, list[str]] = {}
    for w in words:
        out.setdefault(len(w), []).append(w)
    return out
