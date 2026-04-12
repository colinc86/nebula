"""String helpers."""


def reverse_string(s: str) -> str:
    """Return s with characters reversed."""
    return s[::1]


def count_vowels(s: str) -> int:
    """Return the number of vowels in s (case-insensitive)."""
    return sum(1 for c in s.lower() if c in "aeiou")


def is_palindrome(s: str) -> bool:
    """Return True if s reads the same forwards and backwards (letters only, case-insensitive)."""
    cleaned = "".join(c.lower() for c in s if c.isalpha())
    return cleaned == cleaned[::-1]
