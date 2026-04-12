#!/usr/bin/env python3
"""Generate task definitions for distillation.

Each task is a self-contained directory under tasks/ containing:
    meta.json   — task metadata: type, prompt, verify command, expected results
    seed/       — initial state of the working tree (copied per trial)

The distillation runner copies seed/ into a fresh trial dir, runs the agent
loop, then runs the verify command. If it exits 0 (or matches the expected
text pattern for explain tasks), the trace is kept.

Strategy: template mutation. We hand-curate a small library of well-tested
function packages, then generate task variants by:
    - implement: strip function bodies, ask the model to implement
    - refactor: pick a renameable identifier, ask the model to rename
    - bugfix: introduce a known bug pattern, ask the model to fix
    - explain: ask the model to describe what the package does
    - cross_lang: ask the model to port a Python helper to Go (or vice versa)

This is deterministic and cheap (no StarCoderData download). It exercises the
same MCP tool patterns as production scenarios. For the production training
run we can mix in StarCoderData-harvested tasks via prepare_multi_target.py.
"""

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

# ── Function Package library ────────────────────────────────────────────────
# Each PACKAGE is a self-contained Go module (or Python package) with one or
# more functions and a test file. We mutate these into different task types.

GO_PACKAGES = {
    "strings_basic": {
        "go.mod": "module taskpkg\n\ngo 1.21\n",
        "strings_util.go": '''package taskpkg

import "unicode"

// Reverse returns s with its runes in reverse order.
func Reverse(s string) string {
\trunes := []rune(s)
\tfor i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
\t\trunes[i], runes[j] = runes[j], runes[i]
\t}
\treturn string(runes)
}

// CountVowels returns the number of vowels (a, e, i, o, u, case-insensitive) in s.
func CountVowels(s string) int {
\tn := 0
\tfor _, r := range s {
\t\tswitch unicode.ToLower(r) {
\t\tcase 'a', 'e', 'i', 'o', 'u':
\t\t\tn++
\t\t}
\t}
\treturn n
}

// IsAnagram reports whether a and b have the same letters (case-insensitive),
// ignoring spaces.
func IsAnagram(a, b string) bool {
\tcount := func(s string) map[rune]int {
\t\tm := map[rune]int{}
\t\tfor _, r := range s {
\t\t\tif r == ' ' {
\t\t\t\tcontinue
\t\t\t}
\t\t\tm[unicode.ToLower(r)]++
\t\t}
\t\treturn m
\t}
\tca, cb := count(a), count(b)
\tif len(ca) != len(cb) {
\t\treturn false
\t}
\tfor k, v := range ca {
\t\tif cb[k] != v {
\t\t\treturn false
\t\t}
\t}
\treturn true
}
''',
        "strings_util_test.go": '''package taskpkg

import "testing"

func TestReverse(t *testing.T) {
\tcases := []struct{ in, want string }{{"", ""}, {"a", "a"}, {"abc", "cba"}, {"hello", "olleh"}}
\tfor _, c := range cases {
\t\tif got := Reverse(c.in); got != c.want {
\t\t\tt.Errorf("Reverse(%q) = %q, want %q", c.in, got, c.want)
\t\t}
\t}
}

func TestCountVowels(t *testing.T) {
\tcases := []struct {
\t\tin   string
\t\twant int
\t}{{"", 0}, {"hello", 2}, {"AEIOU", 5}, {"xyz", 0}, {"Programming", 3}}
\tfor _, c := range cases {
\t\tif got := CountVowels(c.in); got != c.want {
\t\t\tt.Errorf("CountVowels(%q) = %d, want %d", c.in, got, c.want)
\t\t}
\t}
}

func TestIsAnagram(t *testing.T) {
\tcases := []struct {
\t\ta, b string
\t\twant bool
\t}{
\t\t{"listen", "silent", true},
\t\t{"hello", "world", false},
\t\t{"Astronomer", "Moon starer", true},
\t\t{"a", "ab", false},
\t}
\tfor _, c := range cases {
\t\tif got := IsAnagram(c.a, c.b); got != c.want {
\t\t\tt.Errorf("IsAnagram(%q,%q) = %v want %v", c.a, c.b, got, c.want)
\t\t}
\t}
}
''',
        "renames": [
            ("Reverse", "FlipString"),
            ("CountVowels", "TallyVowels"),
            ("IsAnagram", "AreAnagrams"),
        ],
        "implementable": [
            ("Reverse", "Reverse(s string) string", "return s with runes in reverse order"),
            ("CountVowels", "CountVowels(s string) int", "count vowels (a/e/i/o/u, case-insensitive)"),
            ("IsAnagram", "IsAnagram(a, b string) bool", "true if a and b have the same letters, ignoring case and spaces"),
        ],
        "bugfix_lines": [
            ('runes[i], runes[j] = runes[j], runes[i]', 'runes[i], runes[j] = runes[i], runes[j]', "swap is a no-op (Reverse no longer reverses)"),
            ('case \'a\', \'e\', \'i\', \'o\', \'u\':', 'case \'a\', \'e\', \'i\', \'o\':', "u is missing from the vowel list"),
            ('if len(ca) != len(cb) {', 'if len(ca) == len(cb) {', "comparison flipped (returns false for matching anagrams)"),
        ],
    },
    "slice_basic": {
        "go.mod": "module taskpkg\n\ngo 1.21\n",
        "slice_util.go": '''package taskpkg

// Sum returns the sum of all elements in xs.
func Sum(xs []int) int {
\ts := 0
\tfor _, v := range xs {
\t\ts += v
\t}
\treturn s
}

// Max returns the largest element of xs and true. For an empty slice it
// returns 0 and false.
func Max(xs []int) (int, bool) {
\tif len(xs) == 0 {
\t\treturn 0, false
\t}
\tm := xs[0]
\tfor _, v := range xs[1:] {
\t\tif v > m {
\t\t\tm = v
\t\t}
\t}
\treturn m, true
}

// Filter returns a new slice containing only elements of xs for which keep returns true.
func Filter(xs []int, keep func(int) bool) []int {
\tvar out []int
\tfor _, v := range xs {
\t\tif keep(v) {
\t\t\tout = append(out, v)
\t\t}
\t}
\treturn out
}
''',
        "slice_util_test.go": '''package taskpkg

import (
\t"reflect"
\t"testing"
)

func TestSum(t *testing.T) {
\tcases := []struct {
\t\tin   []int
\t\twant int
\t}{{[]int{}, 0}, {[]int{1, 2, 3}, 6}, {[]int{-5, 5}, 0}, {[]int{10}, 10}}
\tfor _, c := range cases {
\t\tif got := Sum(c.in); got != c.want {
\t\t\tt.Errorf("Sum(%v) = %d, want %d", c.in, got, c.want)
\t\t}
\t}
}

func TestMax(t *testing.T) {
\tif _, ok := Max([]int{}); ok {
\t\tt.Error("expected ok=false for empty")
\t}
\tcases := []struct {
\t\tin   []int
\t\twant int
\t}{{[]int{1}, 1}, {[]int{3, 1, 2}, 3}, {[]int{-1, -2}, -1}}
\tfor _, c := range cases {
\t\tif got, ok := Max(c.in); !ok || got != c.want {
\t\t\tt.Errorf("Max(%v) = %d,%v want %d,true", c.in, got, ok, c.want)
\t\t}
\t}
}

func TestFilter(t *testing.T) {
\teven := func(n int) bool { return n%2 == 0 }
\tgot := Filter([]int{1, 2, 3, 4}, even)
\twant := []int{2, 4}
\tif !reflect.DeepEqual(got, want) {
\t\tt.Errorf("Filter even = %v, want %v", got, want)
\t}
\tif got := Filter(nil, even); len(got) != 0 {
\t\tt.Errorf("Filter(nil) = %v, want empty", got)
\t}
}
''',
        "renames": [
            ("Sum", "TotalOf"),
            ("Max", "Largest"),
            ("Filter", "Select"),
        ],
        "implementable": [
            ("Sum", "Sum(xs []int) int", "return sum of all elements"),
            ("Max", "Max(xs []int) (int, bool)", "return largest element and true; (0,false) for empty slice"),
            ("Filter", "Filter(xs []int, keep func(int) bool) []int", "return new slice with elements where keep returns true"),
        ],
        "bugfix_lines": [
            ('s += v', 's *= v', "Sum multiplies instead of adds"),
            ('if v > m {', 'if v < m {', "Max comparison flipped (becomes Min)"),
            ('if keep(v) {', 'if !keep(v) {', "Filter inverted"),
        ],
    },
    "math_basic": {
        "go.mod": "module taskpkg\n\ngo 1.21\n",
        "math_util.go": '''package taskpkg

// Gcd returns the greatest common divisor of a and b using Euclid's algorithm.
// Both inputs are taken as absolute values.
func Gcd(a, b int) int {
\tif a < 0 {
\t\ta = -a
\t}
\tif b < 0 {
\t\tb = -b
\t}
\tfor b != 0 {
\t\ta, b = b, a%b
\t}
\treturn a
}

// IsPrime reports whether n is prime. n < 2 is not prime.
func IsPrime(n int) bool {
\tif n < 2 {
\t\treturn false
\t}
\tif n%2 == 0 {
\t\treturn n == 2
\t}
\tfor i := 3; i*i <= n; i += 2 {
\t\tif n%i == 0 {
\t\t\treturn false
\t\t}
\t}
\treturn true
}

// Fib returns the nth Fibonacci number with Fib(0)=0, Fib(1)=1.
// For n < 0 it returns 0.
func Fib(n int) int {
\tif n < 0 {
\t\treturn 0
\t}
\ta, b := 0, 1
\tfor i := 0; i < n; i++ {
\t\ta, b = b, a+b
\t}
\treturn a
}
''',
        "math_util_test.go": '''package taskpkg

import "testing"

func TestGcd(t *testing.T) {
\tcases := []struct{ a, b, want int }{{12, 8, 4}, {17, 5, 1}, {0, 5, 5}, {-12, 8, 4}, {100, 75, 25}}
\tfor _, c := range cases {
\t\tif got := Gcd(c.a, c.b); got != c.want {
\t\t\tt.Errorf("Gcd(%d,%d) = %d want %d", c.a, c.b, got, c.want)
\t\t}
\t}
}

func TestIsPrime(t *testing.T) {
\tcases := []struct {
\t\tn    int
\t\twant bool
\t}{{0, false}, {1, false}, {2, true}, {3, true}, {4, false}, {17, true}, {100, false}, {97, true}}
\tfor _, c := range cases {
\t\tif got := IsPrime(c.n); got != c.want {
\t\t\tt.Errorf("IsPrime(%d) = %v want %v", c.n, got, c.want)
\t\t}
\t}
}

func TestFib(t *testing.T) {
\tcases := []struct{ n, want int }{{0, 0}, {1, 1}, {2, 1}, {3, 2}, {4, 3}, {5, 5}, {10, 55}, {-3, 0}}
\tfor _, c := range cases {
\t\tif got := Fib(c.n); got != c.want {
\t\t\tt.Errorf("Fib(%d) = %d want %d", c.n, got, c.want)
\t\t}
\t}
}
''',
        "renames": [
            ("Gcd", "GreatestCommonDivisor"),
            ("IsPrime", "Prime"),
            ("Fib", "Fibonacci"),
        ],
        "implementable": [
            ("Gcd", "Gcd(a, b int) int", "GCD via Euclid's algorithm; treat both inputs as absolute values"),
            ("IsPrime", "IsPrime(n int) bool", "true if n is prime; n < 2 is not prime"),
            ("Fib", "Fib(n int) int", "nth Fibonacci with Fib(0)=0, Fib(1)=1; return 0 for negative n"),
        ],
        "bugfix_lines": [
            ('a, b = b, a%b', 'a, b = b, a/b', "Euclid uses division instead of modulo"),
            ('return n == 2', 'return false', "IsPrime never recognizes 2 as prime"),
            ('a, b = b, a+b', 'a, b = b, a-b', "Fibonacci uses subtraction instead of addition"),
        ],
    },
    "map_basic": {
        "go.mod": "module taskpkg\n\ngo 1.21\n",
        "map_util.go": '''package taskpkg

// MergeStrings combines two maps; values in b override values in a on conflict.
func MergeStrings(a, b map[string]string) map[string]string {
\tout := make(map[string]string, len(a)+len(b))
\tfor k, v := range a {
\t\tout[k] = v
\t}
\tfor k, v := range b {
\t\tout[k] = v
\t}
\treturn out
}

// CountWords returns a map from each whitespace-separated word in s to its
// occurrence count. Empty input returns an empty map.
func CountWords(s string) map[string]int {
\tout := map[string]int{}
\tword := ""
\tflush := func() {
\t\tif word != "" {
\t\t\tout[word]++
\t\t\tword = ""
\t\t}
\t}
\tfor _, r := range s {
\t\tif r == ' ' || r == '\\t' || r == '\\n' {
\t\t\tflush()
\t\t} else {
\t\t\tword += string(r)
\t\t}
\t}
\tflush()
\treturn out
}
''',
        "map_util_test.go": '''package taskpkg

import "testing"

func TestMergeStrings(t *testing.T) {
\ta := map[string]string{"x": "1", "y": "2"}
\tb := map[string]string{"y": "20", "z": "30"}
\tgot := MergeStrings(a, b)
\twant := map[string]string{"x": "1", "y": "20", "z": "30"}
\tif len(got) != len(want) {
\t\tt.Fatalf("len = %d, want %d", len(got), len(want))
\t}
\tfor k, v := range want {
\t\tif got[k] != v {
\t\t\tt.Errorf("[%s] = %q, want %q", k, got[k], v)
\t\t}
\t}
}

func TestCountWords(t *testing.T) {
\tgot := CountWords("the quick brown fox jumps over the lazy dog the")
\tif got["the"] != 3 {
\t\tt.Errorf("the = %d, want 3", got["the"])
\t}
\tif got["fox"] != 1 {
\t\tt.Errorf("fox = %d, want 1", got["fox"])
\t}
\tif got := CountWords(""); len(got) != 0 {
\t\tt.Errorf("empty = %v, want empty", got)
\t}
}
''',
        "renames": [
            ("MergeStrings", "CombineStrings"),
            ("CountWords", "WordFrequencies"),
        ],
        "implementable": [
            ("MergeStrings", "MergeStrings(a, b map[string]string) map[string]string", "combine two maps; b overrides a on conflict"),
            ("CountWords", "CountWords(s string) map[string]int", "count whitespace-separated word occurrences"),
        ],
        "bugfix_lines": [
            ('out[k] = v\n\t}\n\tfor k, v := range b', 'out[k] = v\n\t}\n\tfor k, _ = range b', "second loop discards values, breaking the merge"),
            ('out[word]++', 'out[word] = 1', "CountWords always sets count to 1 instead of incrementing"),
        ],
    },
}

PYTHON_PACKAGES = {
    "py_strings": {
        "string_util.py": '''"""String helpers."""


def reverse_string(s: str) -> str:
    """Return s with characters reversed."""
    return s[::-1]


def count_vowels(s: str) -> int:
    """Return the number of vowels in s (case-insensitive)."""
    return sum(1 for c in s.lower() if c in "aeiou")


def is_palindrome(s: str) -> bool:
    """Return True if s reads the same forwards and backwards (letters only, case-insensitive)."""
    cleaned = "".join(c.lower() for c in s if c.isalpha())
    return cleaned == cleaned[::-1]
''',
        "test_string_util.py": '''"""Plain-asserts test file. Run with: python3 test_string_util.py"""
import string_util

def main():
    assert string_util.reverse_string("") == ""
    assert string_util.reverse_string("hello") == "olleh"
    assert string_util.reverse_string("a") == "a"

    assert string_util.count_vowels("") == 0
    assert string_util.count_vowels("hello") == 2
    assert string_util.count_vowels("AEIOU") == 5
    assert string_util.count_vowels("xyz") == 0

    assert string_util.is_palindrome("racecar")
    assert string_util.is_palindrome("A man a plan a canal Panama")
    assert not string_util.is_palindrome("hello")
    assert string_util.is_palindrome("")

    print("OK")

if __name__ == "__main__":
    main()
''',
        "renames": [
            ("reverse_string", "flip_string"),
            ("count_vowels", "tally_vowels"),
            ("is_palindrome", "palindrome_check"),
        ],
        "implementable": [
            ("reverse_string", "reverse_string(s: str) -> str", "return s with characters in reverse order"),
            ("count_vowels", "count_vowels(s: str) -> int", "count vowels (a/e/i/o/u, case-insensitive)"),
            ("is_palindrome", "is_palindrome(s: str) -> bool", "case-insensitive palindrome check, letters only"),
        ],
        "bugfix_lines": [
            ('return s[::-1]', 'return s[::1]', "step is +1 instead of -1, returns the original string"),
            ('if c in "aeiou"', 'if c in "aeio"', "u missing from the vowel list"),
            ('return cleaned == cleaned[::-1]', 'return cleaned != cleaned[::-1]', "comparison inverted"),
        ],
    },
    "py_lists": {
        "list_util.py": '''"""List helpers."""

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
    if size <= 0 or not xs:
        return []
    return [xs[i:i + size] for i in range(0, len(xs), size)]


def unique(xs: list) -> list:
    """Return distinct elements in first-occurrence order."""
    seen = set()
    out = []
    for x in xs:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out
''',
        "test_list_util.py": '''"""Plain-asserts test file. Run with: python3 test_list_util.py"""
import list_util

def main():
    assert list_util.flatten([[1, 2], [3, 4]]) == [1, 2, 3, 4]
    assert list_util.flatten([]) == []
    assert list_util.flatten([[]]) == []

    assert list_util.chunk([1, 2, 3, 4], 2) == [[1, 2], [3, 4]]
    assert list_util.chunk([1, 2, 3, 4, 5], 2) == [[1, 2], [3, 4], [5]]
    assert list_util.chunk([], 2) == []
    assert list_util.chunk([1, 2, 3], 0) == []

    assert list_util.unique([1, 1, 2, 3, 2]) == [1, 2, 3]
    assert list_util.unique([]) == []
    assert list_util.unique(["a", "b", "a"]) == ["a", "b"]

    print("OK")

if __name__ == "__main__":
    main()
''',
        "renames": [
            ("flatten", "deep_flatten"),
            ("chunk", "split_into_chunks"),
            ("unique", "distinct"),
        ],
        "implementable": [
            ("flatten", "flatten(nested) -> list", "flatten one level of nesting"),
            ("chunk", "chunk(xs, size) -> list", "split into sublists of length size; empty if size <= 0"),
            ("unique", "unique(xs) -> list", "distinct elements in first-occurrence order"),
        ],
        "bugfix_lines": [
            ('out.append(x)', 'out.append(sub)', "appends the sublist instead of each element"),
            ('xs[i:i + size]', 'xs[i:i + size + 1]', "chunks overlap by one element"),
            ('if x not in seen:', 'if x in seen:', "unique() condition inverted — only keeps duplicates"),
        ],
    },
}


# ── Task generators ─────────────────────────────────────────────────────────

def slug(s):
    return "".join(c if c.isalnum() else "_" for c in s.lower())


def write_seed(task_dir, files):
    seed = task_dir / "seed"
    seed.mkdir(parents=True, exist_ok=True)
    for name, content in files.items():
        (seed / name).write_text(content)


def gen_implement_tasks(out_root, lang, packages):
    """Generate implement tasks: strip function bodies, ask the model to fill them in."""
    tasks = []
    for pkg_name, pkg in packages.items():
        for fn_name, sig, desc in pkg["implementable"]:
            task_id = f"impl_{lang}_{pkg_name}_{slug(fn_name)}"
            task_dir = out_root / task_id
            files = {}
            for fname, content in pkg.items():
                if isinstance(content, str):
                    files[fname] = content
            # Strip the function body for the target function
            files = stub_function(files, lang, fn_name)
            write_seed(task_dir, files)
            verify = "go test ./..." if lang == "go" else "python3 test_*.py"
            prompt = (
                f"Implement the function `{fn_name}` in this {lang} package so that the "
                f"existing tests pass. Signature: `{fn_name}({sig.split('(',1)[1] if '(' in sig else sig})`. "
                f"Behavior: {desc}.\n\n"
                f"Use the file-editing tools, then run the tests to confirm. Stop when "
                f"the tests pass."
            )
            (task_dir / "meta.json").write_text(json.dumps({
                "id": task_id,
                "type": "implement",
                "language": lang,
                "package": pkg_name,
                "function": fn_name,
                "prompt": prompt,
                "verify_cmd": verify,
            }, indent=2))
            tasks.append(task_id)
    return tasks


def gen_refactor_tasks(out_root, lang, packages):
    """Generate refactor tasks: rename a symbol across the package."""
    tasks = []
    for pkg_name, pkg in packages.items():
        for old, new in pkg.get("renames", []):
            task_id = f"refact_{lang}_{pkg_name}_{slug(old)}_to_{slug(new)}"
            task_dir = out_root / task_id
            files = {fname: content for fname, content in pkg.items() if isinstance(content, str)}
            write_seed(task_dir, files)
            verify = "go test ./..." if lang == "go" else "python3 test_*.py"
            prompt = (
                f"Rename the {('function' if lang == 'python' else 'identifier')} `{old}` to "
                f"`{new}` throughout this {lang} package, in every file that references it "
                f"(including test files). Use the file-editing tools — don't shell out to sed "
                f"or perl. After the rename is complete, run the tests to confirm everything "
                f"still passes."
            )
            (task_dir / "meta.json").write_text(json.dumps({
                "id": task_id,
                "type": "refactor",
                "language": lang,
                "package": pkg_name,
                "old_name": old,
                "new_name": new,
                "prompt": prompt,
                "verify_cmd": verify,
            }, indent=2))
            tasks.append(task_id)
    return tasks


def gen_bugfix_tasks(out_root, lang, packages):
    """Generate bugfix tasks: introduce a known bug and ask the model to fix it."""
    tasks = []
    for pkg_name, pkg in packages.items():
        for old_line, new_line, bug_desc in pkg.get("bugfix_lines", []):
            task_id = f"bugfix_{lang}_{pkg_name}_{hashlib.md5(old_line.encode()).hexdigest()[:8]}"
            task_dir = out_root / task_id
            files = {fname: content for fname, content in pkg.items() if isinstance(content, str)}
            # Inject the bug
            modified = False
            for fname in files:
                if not (fname.endswith(".go") or fname.endswith(".py")):
                    continue
                if "test" in fname.lower():
                    continue
                if old_line in files[fname]:
                    files[fname] = files[fname].replace(old_line, new_line, 1)
                    modified = True
                    break
            if not modified:
                continue  # bug pattern didn't apply
            write_seed(task_dir, files)
            verify = "go test ./..." if lang == "go" else "python3 test_*.py"
            prompt = (
                f"This {lang} package has a failing test. Run the tests to see the failure, "
                f"then fix the bug in the source file (not the test file). Stop when the "
                f"tests pass."
            )
            (task_dir / "meta.json").write_text(json.dumps({
                "id": task_id,
                "type": "bugfix",
                "language": lang,
                "package": pkg_name,
                "bug_description": bug_desc,
                "expected_old_line": old_line.strip(),
                "expected_new_line": new_line.strip(),
                "prompt": prompt,
                "verify_cmd": verify,
            }, indent=2))
            tasks.append(task_id)
    return tasks


def gen_explain_tasks(out_root, lang, packages):
    """Generate explain tasks: ask the model to describe the package in plain text."""
    tasks = []
    for pkg_name, pkg in packages.items():
        task_id = f"explain_{lang}_{pkg_name}"
        task_dir = out_root / task_id
        files = {fname: content for fname, content in pkg.items() if isinstance(content, str)}
        write_seed(task_dir, files)
        # Verification: just make sure the model produced a non-trivial final message
        # mentioning at least one function name. We let the distillation runner handle that.
        prompt = (
            f"Read all the source files in this {lang} package and write a one-paragraph "
            f"summary of what it does. Mention each public function by name. Do not edit any "
            f"files — your final response should be the summary itself."
        )
        # Verification command: grep the agent's final message for one of the function names.
        # We can't do this from a shell command, so use a sentinel verification that always
        # passes; the distillation runner will additionally check the final message text.
        keywords = []
        for fname, content in files.items():
            if fname.endswith(".go") or fname.endswith(".py"):
                # Pull out function names heuristically
                for line in content.split("\n"):
                    line = line.strip()
                    if line.startswith("func ") and "(" in line:
                        name = line[5:].split("(", 1)[0].split(" ")[0]
                        if name and name[0].isupper():
                            keywords.append(name)
                    elif line.startswith("def "):
                        name = line[4:].split("(", 1)[0]
                        keywords.append(name)
        meta = {
            "id": task_id,
            "type": "explain",
            "language": lang,
            "package": pkg_name,
            "prompt": prompt,
            "verify_cmd": "true",  # always-pass shell verify
            "expected_keywords_in_final": keywords,  # checked by distill.py
        }
        (task_dir / "meta.json").write_text(json.dumps(meta, indent=2))
        tasks.append(task_id)
    return tasks


def gen_cross_lang_tasks(out_root):
    """Generate cross-language port tasks (Python → Go for now)."""
    tasks = []
    PORTS = [
        ("flatten",
         "def flatten(nested):\n    out = []\n    for sub in nested:\n        for x in sub:\n            out.append(x)\n    return out\n",
         "go",
         "Port the Python function `flatten` (which flattens one level of an iterable of iterables) "
         "to Go in a new file `flatten.go`. Use the signature `func Flatten(nested [][]int) []int`. "
         "Then write a Go test in `flatten_test.go` that checks at least 3 cases including the empty case. "
         "Run `go test ./...` to verify."),
        ("count_vowels",
         "def count_vowels(s: str) -> int:\n    return sum(1 for c in s.lower() if c in 'aeiou')\n",
         "go",
         "Port the Python function `count_vowels` (counts vowels in a string, case-insensitive) "
         "to Go in a new file `vowels.go`. Use the signature `func CountVowels(s string) int`. "
         "Then write a Go test in `vowels_test.go` that checks at least 4 cases. "
         "Run `go test ./...` to verify."),
        ("gcd",
         "def gcd(a, b):\n    if a < 0: a = -a\n    if b < 0: b = -b\n    while b != 0:\n        a, b = b, a % b\n    return a\n",
         "go",
         "Port the Python function `gcd` (Euclidean greatest common divisor) to Go in a new file "
         "`gcd.go`. Use the signature `func Gcd(a, b int) int`. Handle negative inputs as absolute "
         "values. Then write a Go test in `gcd_test.go` covering at least 4 cases including a "
         "negative input. Run `go test ./...` to verify."),
    ]
    for fn_name, src_code, target_lang, prompt in PORTS:
        task_id = f"crosslang_{fn_name}_to_{target_lang}"
        task_dir = out_root / task_id
        files = {
            "go.mod": "module taskpkg\n\ngo 1.21\n",
            "source.py": src_code,
        }
        write_seed(task_dir, files)
        (task_dir / "meta.json").write_text(json.dumps({
            "id": task_id,
            "type": "cross_lang",
            "source_language": "python",
            "target_language": target_lang,
            "function": fn_name,
            "prompt": prompt,
            "verify_cmd": "go test ./...",
        }, indent=2))
        tasks.append(task_id)
    return tasks


def stub_function(files, lang, fn_name):
    """Replace the body of fn_name with a stub that returns a zero value."""
    out = dict(files)
    if lang == "go":
        for fname, content in out.items():
            if not fname.endswith(".go") or "test" in fname:
                continue
            new_content, modified = _stub_go_function(content, fn_name)
            if modified:
                out[fname] = new_content
                break
    elif lang == "python":
        for fname, content in out.items():
            if not fname.endswith(".py") or fname.startswith("test_"):
                continue
            new_content, modified = _stub_python_function(content, fn_name)
            if modified:
                out[fname] = new_content
                break
    return out


def _stub_go_function(content, fn_name):
    lines = content.split("\n")
    out = []
    i = 0
    modified = False
    while i < len(lines):
        line = lines[i]
        if line.startswith(f"func {fn_name}(") and "{" in line:
            # Emit signature, then a stub body that returns a zero value
            out.append(line)
            out.append("\t// TODO: implement")
            # Determine return type from signature for the zero value
            ret_part = line.split(")", 2)[-1].strip().rstrip("{").strip()
            zero = _go_zero_value(ret_part)
            if zero is not None:
                out.append(f"\treturn {zero}")
            out.append("}")
            # Skip the original body
            depth = 1
            i += 1
            while i < len(lines) and depth > 0:
                for c in lines[i]:
                    if c == "{":
                        depth += 1
                    elif c == "}":
                        depth -= 1
                        if depth == 0:
                            break
                i += 1
            modified = True
            continue
        out.append(line)
        i += 1
    return "\n".join(out), modified


def _go_zero_value(ret_type):
    if not ret_type:
        return None
    if ret_type == "string":
        return '""'
    if ret_type == "int" or ret_type == "int64":
        return "0"
    if ret_type == "bool":
        return "false"
    if ret_type.startswith("[]"):
        return "nil"
    if ret_type.startswith("map["):
        return "nil"
    if ret_type.startswith("(") and ret_type.endswith(")"):
        # Tuple return like (int, bool)
        parts = [p.strip() for p in ret_type[1:-1].split(",")]
        zeros = [_go_zero_value(p) or "nil" for p in parts]
        return ", ".join(zeros)
    return "nil"


def _stub_python_function(content, fn_name):
    lines = content.split("\n")
    out = []
    i = 0
    modified = False
    while i < len(lines):
        line = lines[i]
        stripped = line.lstrip()
        if stripped.startswith(f"def {fn_name}("):
            # Find indentation level of the body
            indent = len(line) - len(stripped)
            body_indent = " " * (indent + 4)
            out.append(line)
            i += 1
            # Skip docstring if present, but keep it
            while i < len(lines) and (lines[i].strip().startswith('"""') or lines[i].strip().startswith("'''") or not lines[i].strip()):
                if lines[i].strip().startswith('"""') or lines[i].strip().startswith("'''"):
                    out.append(lines[i])
                    quote = '"""' if '"""' in lines[i] else "'''"
                    if lines[i].count(quote) < 2:  # multiline docstring
                        i += 1
                        while i < len(lines) and quote not in lines[i]:
                            out.append(lines[i])
                            i += 1
                        if i < len(lines):
                            out.append(lines[i])
                            i += 1
                    else:
                        i += 1
                    break
                else:
                    out.append(lines[i])
                    i += 1
            out.append(f"{body_indent}# TODO: implement")
            out.append(f"{body_indent}raise NotImplementedError")
            # Skip the rest of the function body (until dedent or EOF)
            while i < len(lines):
                if lines[i].strip() == "":
                    i += 1
                    continue
                line_indent = len(lines[i]) - len(lines[i].lstrip())
                if line_indent <= indent:
                    break
                i += 1
            out.append("")
            modified = True
            continue
        out.append(line)
        i += 1
    return "\n".join(out), modified


# ── Entry point ─────────────────────────────────────────────────────────────

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--output-dir", default="bench/finetune/qwen3_coder_30b/tasks")
    p.add_argument("--types", default="implement,refactor,bugfix,explain,cross_lang",
                   help="Comma-separated task types to generate")
    p.add_argument("--clean", action="store_true", help="Wipe output dir first")
    args = p.parse_args()

    out_root = Path(args.output_dir)
    if args.clean and out_root.exists():
        shutil.rmtree(out_root)
    out_root.mkdir(parents=True, exist_ok=True)

    types = set(args.types.split(","))
    all_tasks = []
    counts = {}

    if "implement" in types:
        n = gen_implement_tasks(out_root, "go", GO_PACKAGES)
        n += gen_implement_tasks(out_root, "python", PYTHON_PACKAGES)
        all_tasks += n
        counts["implement"] = len(n)
    if "refactor" in types:
        n = gen_refactor_tasks(out_root, "go", GO_PACKAGES)
        n += gen_refactor_tasks(out_root, "python", PYTHON_PACKAGES)
        all_tasks += n
        counts["refactor"] = len(n)
    if "bugfix" in types:
        n = gen_bugfix_tasks(out_root, "go", GO_PACKAGES)
        n += gen_bugfix_tasks(out_root, "python", PYTHON_PACKAGES)
        all_tasks += n
        counts["bugfix"] = len(n)
    if "explain" in types:
        n = gen_explain_tasks(out_root, "go", GO_PACKAGES)
        n += gen_explain_tasks(out_root, "python", PYTHON_PACKAGES)
        all_tasks += n
        counts["explain"] = len(n)
    if "cross_lang" in types:
        n = gen_cross_lang_tasks(out_root)
        all_tasks += n
        counts["cross_lang"] = len(n)

    # Write task index
    (out_root / "task_index.json").write_text(json.dumps({
        "total": len(all_tasks),
        "counts": counts,
        "tasks": sorted(all_tasks),
    }, indent=2))

    print(f"Generated {len(all_tasks)} tasks in {out_root}")
    for t, c in counts.items():
        print(f"  {t}: {c}")


if __name__ == "__main__":
    main()
