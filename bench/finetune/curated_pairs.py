#!/usr/bin/env python3
"""Generate curated Nebula training pairs emphasizing compression and idiom.

This is NOT Go→Nebula transliteration. These pairs show the shortest possible
Nebula for common programming tasks, leveraging functional patterns, short-form
functions, pipes, and every compression trick the language offers.

Categories:
    1. Verbose→compact rewrites (showing the model what NOT to do)
    2. Functional-style implementations (map/filter/fold over loops)
    3. Shortest-valid-answer drills (single-line where possible)
    4. Real-world idiom cheat sheet
    5. Structural compression (match over if-chains, short-form fn, etc.)

Each pair is weighted in training by being repeated multiple times with
different prompt framings.

Output: bench/finetune/curated_pairs.jsonl
"""

import json
from pathlib import Path

SYSTEM = "You are an expert Nebula programmer. Nebula is a compact language designed for maximum code density. Prefer functional patterns (map/filter/fold), short-form functions (fn f(x) = expr), pipes (|>), and match expressions over verbose imperative code. Always write the shortest valid implementation."

# ─────────────────────────────────────────────────────────────────────────
# CATEGORY 1: Shortest-valid-answer drills
# "Write X" → single-line or near-single-line compact Nebula
# ─────────────────────────────────────────────────────────────────────────

DRILLS = [
    # Math / arithmetic
    ("Write a Nebula function that doubles a number.",
     "fn double(x) = x * 2"),
    ("Write a Nebula function that squares a number.",
     "fn square(x) = x * x"),
    ("Write a Nebula function that cubes a number.",
     "fn cube(x) = x * x * x"),
    ("Write a Nebula function that returns the absolute value of a number.",
     "fn abs(x) = if x < 0 { -x } el { x }"),
    ("Write a Nebula function that returns the max of two numbers.",
     "fn max(a, b) = if a > b { a } el { b }"),
    ("Write a Nebula function that returns the min of two numbers.",
     "fn min(a, b) = if a < b { a } el { b }"),
    ("Write a Nebula function that checks if a number is even.",
     "fn is_even(n) = n % 2 == 0"),
    ("Write a Nebula function that checks if a number is odd.",
     "fn is_odd(n) = n % 2 != 0"),
    ("Write a Nebula function that checks if a number is positive.",
     "fn is_positive(n) = n > 0"),
    ("Write a Nebula function that checks if a number is negative.",
     "fn is_negative(n) = n < 0"),
    ("Write a Nebula function that checks if a number is zero.",
     "fn is_zero(n) = n == 0"),
    ("Write a Nebula function that adds two numbers.",
     "fn add(a, b) = a + b"),
    ("Write a Nebula function that subtracts two numbers.",
     "fn sub(a, b) = a - b"),
    ("Write a Nebula function that multiplies two numbers.",
     "fn mul(a, b) = a * b"),
    ("Write a Nebula function that divides two numbers.",
     "fn div(a, b) = a / b"),
    ("Write a Nebula function that computes the modulo.",
     "fn mod(a, b) = a % b"),
    ("Write a Nebula function that increments a number.",
     "fn inc(x) = x + 1"),
    ("Write a Nebula function that decrements a number.",
     "fn dec(x) = x - 1"),
    ("Write a Nebula function that negates a number.",
     "fn neg(x) = -x"),
    ("Write a Nebula function that clamps x between lo and hi.",
     "fn clamp(x, lo, hi) = if x < lo { lo } el if x > hi { hi } el { x }"),

    # List operations (one-liners via functional patterns)
    ("Write a Nebula function that returns the sum of a list of numbers.",
     "fn sum(xs) = xs.fold(0, |a, b| a + b)"),
    ("Write a Nebula function that returns the product of a list of numbers.",
     "fn product(xs) = xs.fold(1, |a, b| a * b)"),
    ("Write a Nebula function that doubles every element in a list.",
     "fn double_all(xs) = xs.map(|x| x * 2)"),
    ("Write a Nebula function that squares every element in a list.",
     "fn square_all(xs) = xs.map(|x| x * x)"),
    ("Write a Nebula function that filters a list to only positive numbers.",
     "fn positives(xs) = xs.filter(|x| x > 0)"),
    ("Write a Nebula function that filters a list to only even numbers.",
     "fn evens(xs) = xs.filter(|x| x % 2 == 0)"),
    ("Write a Nebula function that filters a list to only odd numbers.",
     "fn odds(xs) = xs.filter(|x| x % 2 != 0)"),
    ("Write a Nebula function that counts positive numbers in a list.",
     "fn count_positive(xs) = xs.filter(|x| x > 0).len()"),
    ("Write a Nebula function that checks if any element is negative.",
     "fn has_negative(xs) = xs.any(|x| x < 0)"),
    ("Write a Nebula function that checks if all elements are positive.",
     "fn all_positive(xs) = xs.all(|x| x > 0)"),
    ("Write a Nebula function that returns the max of a list.",
     "fn max_of(xs) = xs.fold(xs[0], |a, b| if b > a { b } el { a })"),
    ("Write a Nebula function that returns the min of a list.",
     "fn min_of(xs) = xs.fold(xs[0], |a, b| if b < a { b } el { a })"),
    ("Write a Nebula function that returns the average of a list.",
     "fn avg(xs) = xs.fold(0, |a, b| a + b) / xs.len()"),
    ("Write a Nebula function that reverses a list.",
     "fn reverse(xs) = xs.fold([], |a, x| [x] + a)"),
    ("Write a Nebula function that counts occurrences of x in a list.",
     "fn count_of(xs, x) = xs.filter(|y| y == x).len()"),
    ("Write a Nebula function that checks if a list contains x.",
     "fn contains(xs, x) = xs.any(|y| y == x)"),
    ("Write a Nebula function that finds the first element matching a predicate.",
     "fn find_first(xs, pred) = xs.find(pred)"),
    ("Write a Nebula function that removes all occurrences of x from a list.",
     "fn remove_all(xs, x) = xs.filter(|y| y != x)"),
    ("Write a Nebula function that takes the first n elements of a list.",
     "fn take(xs, n) = xs[0..n]"),
    ("Write a Nebula function that drops the first n elements of a list.",
     "fn drop(xs, n) = xs[n..]"),
    ("Write a Nebula function that zips two lists into pairs.",
     "fn zip(xs, ys) = xs.enumerate().map(|i, x| (x, ys[i]))"),
    ("Write a Nebula function that flattens a nested list by one level.",
     "fn flatten(xss) = xss.fold([], |a, b| a + b)"),
    ("Write a Nebula function that returns unique elements of a list.",
     "fn unique(xs) = xs.fold([], |a, x| if a.contains(x) { a } el { a + [x] })"),
    ("Write a Nebula function that joins a list of strings with a separator.",
     "fn join(xs, sep) = xs.fold(\"\", |a, s| if a == \"\" { s } el { a + sep + s })"),

    # String operations
    ("Write a Nebula function that returns the length of a string.",
     "fn str_len(s) = s.len()"),
    ("Write a Nebula function that checks if a string is empty.",
     "fn is_empty(s) = s.len() == 0"),
    ("Write a Nebula function that uppercases a string.",
     "fn upper(s) = s.to_upper()"),
    ("Write a Nebula function that lowercases a string.",
     "fn lower(s) = s.to_lower()"),
    ("Write a Nebula function that checks if a string starts with a prefix.",
     "fn starts_with(s, prefix) = s.starts_with(prefix)"),
    ("Write a Nebula function that checks if a string ends with a suffix.",
     "fn ends_with(s, suffix) = s.ends_with(suffix)"),
    ("Write a Nebula function that reverses a string.",
     "fn reverse_str(s) = s.chars().reverse().join(\"\")"),
    ("Write a Nebula function that counts occurrences of char c in string s.",
     "fn count_char(s, c) = s.chars().filter(|x| x == c).len()"),

    # Map operations
    ("Write a Nebula function that gets a value from a map with a default.",
     "fn get_or(m, k, default) = m[k] ?? default"),
    ("Write a Nebula function that increments a counter in a map.",
     "fn inc_count(m, k) = { m[k] = (m[k] ?? 0) + 1; m }"),
    ("Write a Nebula function that returns all keys of a map.",
     "fn keys_of(m) = m.keys()"),
    ("Write a Nebula function that returns all values of a map.",
     "fn values_of(m) = m.values()"),
    ("Write a Nebula function that checks if a map contains a key.",
     "fn has_key(m, k) = m.contains_key(k)"),
    ("Write a Nebula function that merges two maps.",
     "fn merge(a, b) = b.entries().fold(a, |acc, (k, v)| { acc[k] = v; acc })"),

    # Control flow compressed
    ("Write a Nebula function that returns 'yes' if x > 0 else 'no'.",
     "fn yes_no(x) = if x > 0 { \"yes\" } el { \"no\" }"),
    ("Write a Nebula function that converts a bool to 1 or 0.",
     "fn bool_to_int(b) = if b { 1 } el { 0 }"),
    ("Write a Nebula function that returns the sign of a number (-1, 0, 1).",
     "fn sign(x) = if x > 0 { 1 } el if x < 0 { -1 } el { 0 }"),
    ("Write a Nebula function that returns the day name for a weekday number.",
     "fn day_name(n) = mc n { 0 => \"Mon\" 1 => \"Tue\" 2 => \"Wed\" 3 => \"Thu\" 4 => \"Fri\" 5 => \"Sat\" 6 => \"Sun\" _ => \"?\" }"),
    ("Write a Nebula function that categorizes age into child/teen/adult/senior.",
     "fn age_group(age) = mc age { 0..=12 => \"child\" 13..=19 => \"teen\" 20..=64 => \"adult\" _ => \"senior\" }"),

    # Recursion / classic algorithms
    ("Write a Nebula function for factorial.",
     "fn fact(n) = if n <= 1 { 1 } el { n * fact(n - 1) }"),
    ("Write a Nebula function for Fibonacci.",
     "fn fib(n) = if n < 2 { n } el { fib(n - 1) + fib(n - 2) }"),
    ("Write a Nebula function for greatest common divisor.",
     "fn gcd(a, b) = if b == 0 { a } el { gcd(b, a % b) }"),
    ("Write a Nebula function for least common multiple.",
     "fn lcm(a, b) = a * b / gcd(a, b)"),
    ("Write a Nebula function for power using recursion.",
     "fn pow(b, e) = if e == 0 { 1 } el { b * pow(b, e - 1) }"),
    ("Write a Nebula function that checks if a number is prime.",
     "fn is_prime(n) = n > 1 and (2..=n.sqrt()).all(|i| n % i != 0)"),
    ("Write a Nebula function that returns the first n primes.",
     "fn primes(n) = (2..).filter(is_prime).take(n).collect()"),
    ("Write a Nebula function that returns the nth triangular number.",
     "fn triangular(n) = n * (n + 1) / 2"),
    ("Write a Nebula function that computes the sum 1..n.",
     "fn sum_to(n) = (1..=n).fold(0, |a, b| a + b)"),
    ("Write a Nebula function that computes binomial coefficient C(n, k).",
     "fn binomial(n, k) = fact(n) / (fact(k) * fact(n - k))"),
]

# ─────────────────────────────────────────────────────────────────────────
# CATEGORY 2: Verbose → compact rewrites
# Show the model: "this is verbose, rewrite it shorter"
# ─────────────────────────────────────────────────────────────────────────

VERBOSE_REWRITES = [
    (
        # Verbose
        """fn sum_even(xs: [Int]) -> Int {
  mt total = 0
  fo _, x in xs {
    if x % 2 == 0 {
      total += x
    }
  }
  rt total
}""",
        # Compact
        "fn sum_even(xs) = xs.filter(|x| x % 2 == 0).fold(0, |a, b| a + b)"
    ),
    (
        """fn count_positives(xs: [Int]) -> Int {
  mt count = 0
  fo _, x in xs {
    if x > 0 {
      count += 1
    }
  }
  rt count
}""",
        "fn count_positives(xs) = xs.filter(|x| x > 0).len()"
    ),
    (
        """fn doubled(xs: [Int]) -> [Int] {
  mt result = []
  fo _, x in xs {
    result = psh(result, x * 2)
  }
  rt result
}""",
        "fn doubled(xs) = xs.map(|x| x * 2)"
    ),
    (
        """fn has_negative(xs: [Int]) -> Bool {
  fo _, x in xs {
    if x < 0 {
      rt tt
    }
  }
  rt ff
}""",
        "fn has_negative(xs) = xs.any(|x| x < 0)"
    ),
    (
        """fn all_positive(xs: [Int]) -> Bool {
  fo _, x in xs {
    if x <= 0 {
      rt ff
    }
  }
  rt tt
}""",
        "fn all_positive(xs) = xs.all(|x| x > 0)"
    ),
    (
        """fn find_first_negative(xs: [Int]) -> Int? {
  fo _, x in xs {
    if x < 0 {
      rt Some(x)
    }
  }
  rt None
}""",
        "fn find_first_negative(xs) = xs.find(|x| x < 0)"
    ),
    (
        """fn max_value(xs: [Int]) -> Int {
  mt max = xs[0]
  fo _, x in xs {
    if x > max {
      max = x
    }
  }
  rt max
}""",
        "fn max_value(xs) = xs.fold(xs[0], |a, b| if b > a { b } el { a })"
    ),
    (
        """fn is_positive(x: Int) -> Bool {
  if x > 0 {
    rt tt
  } el {
    rt ff
  }
}""",
        "fn is_positive(x) = x > 0"
    ),
    (
        """fn day_name(n: Int) -> String {
  if n == 0 {
    rt \"Monday\"
  } el if n == 1 {
    rt \"Tuesday\"
  } el if n == 2 {
    rt \"Wednesday\"
  } el if n == 3 {
    rt \"Thursday\"
  } el if n == 4 {
    rt \"Friday\"
  } el if n == 5 {
    rt \"Saturday\"
  } el if n == 6 {
    rt \"Sunday\"
  } el {
    rt \"Unknown\"
  }
}""",
        """fn day_name(n) = mc n {
  0 => "Monday"
  1 => "Tuesday"
  2 => "Wednesday"
  3 => "Thursday"
  4 => "Friday"
  5 => "Saturday"
  6 => "Sunday"
  _ => "Unknown"
}"""
    ),
    (
        """fn get_name(user: User) -> String {
  if user.name != nil {
    rt user.name
  } el {
    rt \"Anonymous\"
  }
}""",
        "fn get_name(user) = user.name ?? \"Anonymous\""
    ),
    (
        """fn increment_count(m: Map<String, Int>, key: String) -> Map<String, Int> {
  mt count = m[key]
  if count == nil {
    count = 0
  }
  m[key] = count + 1
  rt m
}""",
        "fn increment_count(m, key) = { m[key] = (m[key] ?? 0) + 1; m }"
    ),
    (
        """fn flatten(xss: [[Int]]) -> [Int] {
  mt result = []
  fo _, xs in xss {
    fo _, x in xs {
      result = psh(result, x)
    }
  }
  rt result
}""",
        "fn flatten(xss) = xss.fold([], |a, xs| a + xs)"
    ),
    (
        """fn product(xs: [Int]) -> Int {
  mt result = 1
  fo _, x in xs {
    result = result * x
  }
  rt result
}""",
        "fn product(xs) = xs.fold(1, |a, b| a * b)"
    ),
    (
        """fn first_even(xs: [Int]) -> Int? {
  fo _, x in xs {
    if x % 2 == 0 {
      rt Some(x)
    }
  }
  rt None
}""",
        "fn first_even(xs) = xs.find(|x| x % 2 == 0)"
    ),
    (
        """fn uppercase_all(strings: [String]) -> [String] {
  mt result = []
  fo _, s in strings {
    result = psh(result, s.to_upper())
  }
  rt result
}""",
        "fn uppercase_all(strings) = strings.map(|s| s.to_upper())"
    ),
    (
        """fn count_words(text: String) -> Map<String, Int> {
  mt counts = {}
  fo _, word in text.split(\" \") {
    if counts.contains_key(word) {
      counts[word] = counts[word] + 1
    } el {
      counts[word] = 1
    }
  }
  rt counts
}""",
        "fn count_words(text) = text.split(\" \").fold({}, |acc, w| { acc[w] = (acc[w] ?? 0) + 1; acc })"
    ),
    (
        """fn total_age(users: [User]) -> Int {
  mt total = 0
  fo _, u in users {
    total += u.age
  }
  rt total
}""",
        "fn total_age(users) = users.map(|u| u.age).fold(0, |a, b| a + b)"
    ),
    (
        """fn names_of_adults(users: [User]) -> [String] {
  mt result = []
  fo _, u in users {
    if u.age >= 18 {
      result = psh(result, u.name)
    }
  }
  rt result
}""",
        "fn names_of_adults(users) = users.filter(|u| u.age >= 18).map(|u| u.name)"
    ),
    (
        """fn safe_divide(a: Float, b: Float) -> Float {
  if b == 0 {
    rt 0
  } el {
    rt a / b
  }
}""",
        "fn safe_divide(a, b) = if b == 0 { 0 } el { a / b }"
    ),
    (
        """fn classify(n: Int) -> String {
  if n < 0 {
    rt \"negative\"
  } el if n == 0 {
    rt \"zero\"
  } el {
    rt \"positive\"
  }
}""",
        "fn classify(n) = mc n { n if n < 0 => \"negative\"; 0 => \"zero\"; _ => \"positive\" }"
    ),
]

# ─────────────────────────────────────────────────────────────────────────
# CATEGORY 3: Go → Nebula with massive compression
# Show that Nebula versions are dramatically shorter
# ─────────────────────────────────────────────────────────────────────────

GO_TO_NEBULA_COMPRESSED = [
    (
        """func Sum(xs []int) int {
    total := 0
    for _, x := range xs {
        total += x
    }
    return total
}""",
        "fn Sum(xs) = xs.fold(0, |a, b| a + b)"
    ),
    (
        """func Map(xs []int, f func(int) int) []int {
    result := make([]int, 0, len(xs))
    for _, x := range xs {
        result = append(result, f(x))
    }
    return result
}""",
        "fn Map(xs, f) = xs.map(f)"
    ),
    (
        """func Filter(xs []int, pred func(int) bool) []int {
    result := make([]int, 0)
    for _, x := range xs {
        if pred(x) {
            result = append(result, x)
        }
    }
    return result
}""",
        "fn Filter(xs, pred) = xs.filter(pred)"
    ),
    (
        """func Contains(xs []int, target int) bool {
    for _, x := range xs {
        if x == target {
            return true
        }
    }
    return false
}""",
        "fn Contains(xs, target) = xs.any(|x| x == target)"
    ),
    (
        """func IndexOf(xs []int, target int) int {
    for i, x := range xs {
        if x == target {
            return i
        }
    }
    return -1
}""",
        "fn IndexOf(xs, target) = xs.enumerate().find(|i, x| x == target).map(|i, _| i) ?? -1"
    ),
    (
        """func GroupBy(xs []Item, key func(Item) string) map[string][]Item {
    result := make(map[string][]Item)
    for _, x := range xs {
        k := key(x)
        result[k] = append(result[k], x)
    }
    return result
}""",
        "fn GroupBy(xs, key) = xs.fold({}, |acc, x| { lt k = key(x); acc[k] = psh(acc[k] ?? [], x); acc })"
    ),
    (
        """func Unique(xs []int) []int {
    seen := make(map[int]bool)
    result := make([]int, 0)
    for _, x := range xs {
        if !seen[x] {
            seen[x] = true
            result = append(result, x)
        }
    }
    return result
}""",
        "fn Unique(xs) = xs.fold([], |a, x| if a.contains(x) { a } el { a + [x] })"
    ),
    (
        """func Reverse(xs []int) []int {
    result := make([]int, len(xs))
    for i, x := range xs {
        result[len(xs)-1-i] = x
    }
    return result
}""",
        "fn Reverse(xs) = xs.fold([], |a, x| [x] + a)"
    ),
    (
        """func CountOccurrences(xs []string, target string) int {
    count := 0
    for _, x := range xs {
        if x == target {
            count++
        }
    }
    return count
}""",
        "fn CountOccurrences(xs, target) = xs.filter(|x| x == target).len()"
    ),
    (
        """func Max(xs []int) int {
    if len(xs) == 0 {
        return 0
    }
    max := xs[0]
    for _, x := range xs[1:] {
        if x > max {
            max = x
        }
    }
    return max
}""",
        "fn Max(xs) = xs.fold(xs[0], |a, b| if b > a { b } el { a })"
    ),
    (
        """func Min(xs []float64) float64 {
    min := xs[0]
    for _, x := range xs {
        if x < min {
            min = x
        }
    }
    return min
}""",
        "fn Min(xs) = xs.fold(xs[0], |a, b| if b < a { b } el { a })"
    ),
    (
        """func Average(xs []float64) float64 {
    if len(xs) == 0 {
        return 0
    }
    sum := 0.0
    for _, x := range xs {
        sum += x
    }
    return sum / float64(len(xs))
}""",
        "fn Average(xs) = if xs.len() == 0 { 0 } el { xs.fold(0, |a, b| a + b) / xs.len() }"
    ),
    (
        """func Range(start, end int) []int {
    result := make([]int, 0, end-start)
    for i := start; i < end; i++ {
        result = append(result, i)
    }
    return result
}""",
        "fn Range(start, end) = (start..end).collect()"
    ),
    (
        """func Repeat(value string, n int) []string {
    result := make([]string, n)
    for i := range result {
        result[i] = value
    }
    return result
}""",
        "fn Repeat(value, n) = (0..n).map(|_| value).collect()"
    ),
    (
        """func IsSorted(xs []int) bool {
    for i := 1; i < len(xs); i++ {
        if xs[i] < xs[i-1] {
            return false
        }
    }
    return true
}""",
        "fn IsSorted(xs) = xs.windows(2).all(|w| w[0] <= w[1])"
    ),
    (
        """func Intersperse(xs []string, sep string) []string {
    if len(xs) == 0 {
        return xs
    }
    result := make([]string, 0, 2*len(xs)-1)
    result = append(result, xs[0])
    for _, x := range xs[1:] {
        result = append(result, sep, x)
    }
    return result
}""",
        "fn Intersperse(xs, sep) = xs.fold([], |a, x| if a.len() == 0 { [x] } el { a + [sep, x] })"
    ),
    (
        """func Partition(xs []int, pred func(int) bool) ([]int, []int) {
    yes := make([]int, 0)
    no := make([]int, 0)
    for _, x := range xs {
        if pred(x) {
            yes = append(yes, x)
        } else {
            no = append(no, x)
        }
    }
    return yes, no
}""",
        "fn Partition(xs, pred) = (xs.filter(pred), xs.filter(|x| not pred(x)))"
    ),
    (
        """func Zip(xs, ys []int) [][2]int {
    n := len(xs)
    if len(ys) < n {
        n = len(ys)
    }
    result := make([][2]int, n)
    for i := 0; i < n; i++ {
        result[i] = [2]int{xs[i], ys[i]}
    }
    return result
}""",
        "fn Zip(xs, ys) = (0..min(xs.len(), ys.len())).map(|i| (xs[i], ys[i])).collect()"
    ),
    (
        """func FlatMap(xs []int, f func(int) []int) []int {
    result := make([]int, 0)
    for _, x := range xs {
        result = append(result, f(x)...)
    }
    return result
}""",
        "fn FlatMap(xs, f) = xs.map(f).fold([], |a, b| a + b)"
    ),
    (
        """func DropWhile(xs []int, pred func(int) bool) []int {
    i := 0
    for i < len(xs) && pred(xs[i]) {
        i++
    }
    return xs[i:]
}""",
        "fn DropWhile(xs, pred) = xs[xs.enumerate().find(|i, x| not pred(x)).map(|i, _| i) ?? xs.len()..]"
    ),
    (
        """func Chunk(xs []int, size int) [][]int {
    var result [][]int
    for i := 0; i < len(xs); i += size {
        end := i + size
        if end > len(xs) {
            end = len(xs)
        }
        result = append(result, xs[i:end])
    }
    return result
}""",
        "fn Chunk(xs, size) = (0..xs.len()).step(size).map(|i| xs[i..min(i + size, xs.len())]).collect()"
    ),
    (
        """func AllEqual(xs []int) bool {
    if len(xs) < 2 {
        return true
    }
    first := xs[0]
    for _, x := range xs[1:] {
        if x != first {
            return false
        }
    }
    return true
}""",
        "fn AllEqual(xs) = xs.len() < 2 or xs.all(|x| x == xs[0])"
    ),
    (
        """func CountBy(xs []string, key func(string) string) map[string]int {
    counts := make(map[string]int)
    for _, x := range xs {
        counts[key(x)]++
    }
    return counts
}""",
        "fn CountBy(xs, key) = xs.fold({}, |acc, x| { lt k = key(x); acc[k] = (acc[k] ?? 0) + 1; acc })"
    ),
    (
        """func Sum2D(grid [][]int) int {
    total := 0
    for _, row := range grid {
        for _, x := range row {
            total += x
        }
    }
    return total
}""",
        "fn Sum2D(grid) = grid.fold(0, |a, row| a + row.fold(0, |b, x| b + x))"
    ),
    (
        """func FindMaxBy(xs []Item, score func(Item) int) Item {
    max := xs[0]
    maxScore := score(max)
    for _, x := range xs[1:] {
        s := score(x)
        if s > maxScore {
            max = x
            maxScore = s
        }
    }
    return max
}""",
        "fn FindMaxBy(xs, score) = xs.fold(xs[0], |a, b| if score(b) > score(a) { b } el { a })"
    ),
]

# ─────────────────────────────────────────────────────────────────────────
# CATEGORY 4: "Write shortest" explicit prompts
# ─────────────────────────────────────────────────────────────────────────

SHORTEST_PROMPTS = [
    ("Give me the shortest Nebula function that checks if a string is a palindrome.",
     "fn is_palindrome(s) = s == s.chars().reverse().join(\"\")"),
    ("Give me the shortest Nebula function that reverses a list.",
     "fn rev(xs) = xs.fold([], |a, x| [x] + a)"),
    ("Give me the shortest Nebula function that counts vowels in a string.",
     "fn vowels(s) = s.chars().filter(|c| \"aeiouAEIOU\".contains(c)).len()"),
    ("Give me the shortest Nebula function that checks if a year is a leap year.",
     "fn is_leap(y) = y % 4 == 0 and (y % 100 != 0 or y % 400 == 0)"),
    ("Give me the shortest Nebula function that computes the Hamming distance between two strings.",
     "fn hamming(a, b) = a.chars().enumerate().filter(|i, c| c != b.chars()[i]).len()"),
    ("Give me the shortest Nebula function that FizzBuzz-labels a number.",
     "fn fizzbuzz(n) = mc (n % 3, n % 5) { (0, 0) => \"FizzBuzz\"; (0, _) => \"Fizz\"; (_, 0) => \"Buzz\"; _ => \"{n}\" }"),
    ("Give me the shortest Nebula function that converts Celsius to Fahrenheit.",
     "fn c_to_f(c) = c * 9 / 5 + 32"),
    ("Give me the shortest Nebula function that converts Fahrenheit to Celsius.",
     "fn f_to_c(f) = (f - 32) * 5 / 9"),
    ("Give me the shortest Nebula function that returns the number of digits in an integer.",
     "fn num_digits(n) = if n == 0 { 1 } el { n.abs().to_string().len() }"),
    ("Give me the shortest Nebula function that reverses the digits of an integer.",
     "fn reverse_digits(n) = n.to_string().chars().reverse().join(\"\").to_int()"),
    ("Give me the shortest Nebula function that checks if two strings are anagrams.",
     "fn anagrams(a, b) = a.chars().sort() == b.chars().sort()"),
    ("Give me the shortest Nebula function that removes duplicates from a sorted list.",
     "fn dedup(xs) = xs.fold([], |a, x| if a.len() > 0 and a[a.len()-1] == x { a } el { a + [x] })"),
    ("Give me the shortest Nebula function that checks if a string is all uppercase.",
     "fn is_upper(s) = s == s.to_upper()"),
    ("Give me the shortest Nebula function that returns the most common element in a list.",
     "fn most_common(xs) = xs.fold({}, |acc, x| { acc[x] = (acc[x] ?? 0) + 1; acc }).entries().fold(xs[0], |a, (k, v)| if v > acc[a] { k } el { a })"),
    ("Give me the shortest Nebula function for the power set of a list.",
     "fn power_set(xs) = xs.fold([[]], |acc, x| acc + acc.map(|s| s + [x]))"),
]

# ─────────────────────────────────────────────────────────────────────────
# CATEGORY 5: Idiom lookup - direct patterns
# ─────────────────────────────────────────────────────────────────────────

IDIOMS = [
    ("In Nebula, how do I sum all numbers in a list?",
     "Use fold: `xs.fold(0, |a, b| a + b)` or if you have it, `xs.sum()`."),
    ("In Nebula, how do I count items matching a condition?",
     "Filter and then take length: `xs.filter(pred).len()`"),
    ("In Nebula, how do I check if any item matches a condition?",
     "Use `any`: `xs.any(|x| pred(x))`"),
    ("In Nebula, how do I check if all items match a condition?",
     "Use `all`: `xs.all(|x| pred(x))`"),
    ("In Nebula, how do I find the first matching item?",
     "Use `find`: `xs.find(|x| pred(x))` — returns an Option."),
    ("In Nebula, how do I transform each item in a list?",
     "Use `map`: `xs.map(|x| transform(x))`"),
    ("In Nebula, how do I chain multiple operations?",
     "Use the pipe operator: `xs |> filter(pred) |> map(transform) |> sum()`"),
    ("In Nebula, how do I provide a default for a nil value?",
     "Use the nil-coalesce operator: `value ?? default`"),
    ("In Nebula, how do I write a one-line function?",
     "Use short-form: `fn name(params) = expression`"),
    ("In Nebula, how do I write a match expression inline?",
     "`mc value { 0 => \"zero\"; 1 => \"one\"; _ => \"other\" }`"),
    ("In Nebula, how do I check if a map has a key?",
     "Use `contains_key`: `m.contains_key(k)` or `m[k] != nil`"),
    ("In Nebula, how do I increment a counter in a map?",
     "`m[k] = (m[k] ?? 0) + 1`"),
    ("In Nebula, how do I return a boolean expression directly?",
     "Just return the expression: `rt x > 0` not `if x > 0 { rt tt } el { rt ff }`"),
    ("In Nebula, what's the shortest way to double every element in a list?",
     "`xs.map(|x| x * 2)`"),
    ("In Nebula, what's the shortest way to filter and then sum?",
     "`xs.filter(pred).fold(0, |a, b| a + b)`"),
    ("In Nebula, how do I create an empty list?",
     "`[]`"),
    ("In Nebula, how do I create an empty map?",
     "`{}`"),
    ("In Nebula, how do I get a range of numbers?",
     "Use range syntax: `0..10` (exclusive) or `0..=10` (inclusive)"),
    ("In Nebula, how do I convert a range to a list?",
     "`(0..10).collect()` or use it directly in map/filter chains."),
    ("In Nebula, what's the idiomatic way to write if-else that returns a value?",
     "As an expression: `lt x = if cond { a } el { b }`"),
]


def make_pair(user, assistant, system=SYSTEM):
    return {
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
            {"role": "assistant", "content": assistant},
        ]
    }


def main():
    pairs = []

    # Category 1: Drills with multiple framings
    drill_framings = [
        lambda q, a: (q, a),
        lambda q, a: (q.replace("Write a Nebula function", "Show me the shortest Nebula function"), a),
        lambda q, a: (q.replace("Write a Nebula function", "In Nebula, write a function"), a),
        lambda q, a: (f"Implement in Nebula: {q.lower().replace('write a nebula function that ', '').rstrip('.')}.", a),
    ]
    for q, a in DRILLS:
        for frame in drill_framings:
            fq, fa = frame(q, a)
            pairs.append(make_pair(fq, f"```nebula\n{fa}\n```"))

    # Category 2: Verbose → compact
    for verbose, compact in VERBOSE_REWRITES:
        pairs.append(make_pair(
            f"Rewrite this verbose Nebula to be as compact as possible:\n\n```nebula\n{verbose}\n```",
            f"```nebula\n{compact}\n```"
        ))
        pairs.append(make_pair(
            f"Make this Nebula code shorter without changing its behavior:\n\n```nebula\n{verbose}\n```",
            f"```nebula\n{compact}\n```"
        ))
        pairs.append(make_pair(
            f"Give me the most compact Nebula equivalent of:\n\n```nebula\n{verbose}\n```",
            f"```nebula\n{compact}\n```"
        ))

    # Category 3: Go → compact Nebula
    for go, compact_neb in GO_TO_NEBULA_COMPRESSED:
        pairs.append(make_pair(
            f"Translate this Go to the most compact valid Nebula:\n\n```go\n{go}\n```",
            f"```nebula\n{compact_neb}\n```"
        ))
        pairs.append(make_pair(
            f"What's the shortest Nebula equivalent of this Go function?\n\n```go\n{go}\n```",
            f"```nebula\n{compact_neb}\n```"
        ))
        pairs.append(make_pair(
            f"Rewrite this Go in idiomatic compact Nebula:\n\n```go\n{go}\n```",
            f"```nebula\n{compact_neb}\n```"
        ))

    # Category 4: Shortest prompts
    for q, a in SHORTEST_PROMPTS:
        pairs.append(make_pair(q, f"```nebula\n{a}\n```"))
        pairs.append(make_pair(q.replace("Give me", "Show me"), f"```nebula\n{a}\n```"))
        pairs.append(make_pair(q.replace("Give me the shortest", "What's the shortest"), f"```nebula\n{a}\n```"))

    # Category 5: Idioms (text answers, not code blocks)
    for q, a in IDIOMS:
        pairs.append(make_pair(q, a))
        # Rephrased
        pairs.append(make_pair(q.replace("In Nebula,", "").strip().capitalize() + " (in Nebula)", a))

    # Save
    out = Path(__file__).parent / "curated_pairs.jsonl"
    with open(out, "w") as f:
        for p in pairs:
            f.write(json.dumps(p) + "\n")

    print(f"Generated {len(pairs)} curated pairs → {out}")
    print(f"Size: {out.stat().st_size:,} bytes")

    # Token count
    try:
        import tiktoken
        enc = tiktoken.get_encoding("cl100k_base")
        total = sum(len(enc.encode(json.dumps(p))) for p in pairs)
        print(f"Tokens: {total:,}")
    except ImportError:
        pass

    # Category breakdown
    print(f"\nCategory breakdown:")
    print(f"  Drills:              {len(DRILLS)} base × 4 framings = {len(DRILLS) * 4}")
    print(f"  Verbose rewrites:    {len(VERBOSE_REWRITES)} base × 3 framings = {len(VERBOSE_REWRITES) * 3}")
    print(f"  Go→Nebula compress:  {len(GO_TO_NEBULA_COMPRESSED)} base × 3 framings = {len(GO_TO_NEBULA_COMPRESSED) * 3}")
    print(f"  Shortest prompts:    {len(SHORTEST_PROMPTS)} base × 3 framings = {len(SHORTEST_PROMPTS) * 3}")
    print(f"  Idioms:              {len(IDIOMS)} base × 2 framings = {len(IDIOMS) * 2}")


if __name__ == "__main__":
    main()
