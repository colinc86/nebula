#!/usr/bin/env python3
"""Additional curated training pairs to expand the compression corpus.

Focuses on gaps in curated_pairs.py:
- More Go → compact Nebula translations (real-world patterns)
- Error handling patterns (try/catch, Result, ?)
- Struct and enum idioms
- Pattern matching across diverse shapes
- Concurrent/channel patterns
- Module organization
- Testing patterns
- More verbose→compact rewrites (medium difficulty)
"""

import json
from pathlib import Path

SYSTEM = "You are an expert Nebula programmer. Nebula is a compact language designed for maximum code density. Prefer functional patterns (map/filter/fold), short-form functions (fn f(x) = expr), pipes (|>), and match expressions over verbose imperative code. Always write the shortest valid implementation."


# ─────────────────────────────────────────────────────────────────────────
# MORE DRILLS: Struct methods, enum handling, error propagation
# ─────────────────────────────────────────────────────────────────────────

STRUCT_DRILLS = [
    ("Write a Nebula struct Point with x and y floats.",
     "st Point {\n  x: Float\n  y: Float\n}"),
    ("Write a Nebula struct User with id, name, and email.",
     "st User {\n  id: Int\n  name: String\n  email: String\n}"),
    ("Write a Nebula struct Rectangle with width and height.",
     "st Rectangle {\n  width: Float\n  height: Float\n}"),
    ("Write a Nebula struct Circle and an impl block with an area method.",
     "st Circle { radius: Float }\n\nim Circle {\n  fn area(sf) = 3.14159 * sf.radius ** 2\n}"),
    ("Write a Nebula struct Stack and an impl block with push, pop, and peek.",
     "st Stack<T> { items: [T] }\n\nim Stack<T> {\n  fn push(mt sf, x: T) { sf.items = psh(sf.items, x) }\n  fn pop(mt sf) -> T? = sf.items.pop()\n  fn peek(sf) -> T? = if sf.items.len() > 0 { Some(sf.items[sf.items.len() - 1]) } el { None }\n}"),
    ("Write a Nebula struct Counter with a count field and methods increment and reset.",
     "st Counter { count: Int }\n\nim Counter {\n  fn increment(mt sf) { sf.count += 1 }\n  fn reset(mt sf) { sf.count = 0 }\n}"),
    ("Write a Nebula enum for a traffic light with Red, Yellow, Green.",
     "en Light {\n  Red\n  Yellow\n  Green\n}"),
    ("Write a Nebula enum Shape with Circle(Float), Square(Float), Rectangle(Float, Float).",
     "en Shape {\n  Circle(Float)\n  Square(Float)\n  Rectangle(Float, Float)\n}"),
    ("Write a Nebula enum Result with Ok(T) and Err(E).",
     "en Result<T, E> {\n  Ok(T)\n  Err(E)\n}"),
    ("Write a Nebula enum Option with Some(T) and None.",
     "en Option<T> {\n  Some(T)\n  None\n}"),
    ("Write a Nebula impl block for Shape that computes area using match.",
     "im Shape {\n  fn area(sf) = mc sf {\n    Circle(r) => 3.14159 * r * r\n    Square(s) => s * s\n    Rectangle(w, h) => w * h\n  }\n}"),
    ("Write a Nebula trait Printable with a to_string method.",
     "tr Printable {\n  fn to_string(sf) -> String\n}"),
    ("Write a Nebula trait Eq with an equals method.",
     "tr Eq {\n  fn equals(sf, other: Sf) -> Bool\n}"),
    ("Write a Nebula trait Ord with a compare method returning Int.",
     "tr Ord {\n  fn compare(sf, other: Sf) -> Int\n}"),
    ("Write a Nebula implementation of Printable for Point.",
     "im Printable fo Point {\n  fn to_string(sf) = \"({sf.x}, {sf.y})\"\n}"),
    ("Write a Nebula struct LinkedList node with value and next pointer.",
     "st Node<T> {\n  value: T\n  next: Node<T>?\n}"),
    ("Write a Nebula struct Tree node with value and children list.",
     "st Tree<T> {\n  value: T\n  children: [Tree<T>]\n}"),
    ("Write a Nebula struct KeyValue with a key and value of different types.",
     "st KeyValue<K, V> {\n  key: K\n  value: V\n}"),
    ("Write a Nebula struct HttpResponse with status code and body.",
     "st HttpResponse {\n  status: Int\n  body: String\n}"),
    ("Write a Nebula struct Config with host, port, and debug fields.",
     "st Config {\n  host: String\n  port: Int\n  debug: Bool\n}"),
]

ERROR_DRILLS = [
    ("Write a Nebula function that parses an int, returning a Result.",
     "fn parse(s) -> Result<Int, String> = s.to_int() ?? Err(\"not a number\")"),
    ("Write a Nebula function that divides safely, returning nil on divide-by-zero.",
     "fn safe_div(a, b) = if b == 0 { nil } el { Some(a / b) }"),
    ("Write a Nebula function that reads a file and returns Result.",
     "fn read_file(path) = try { Ok(std.fs.read(path)) } ch e { Err(e) }"),
    ("Write a Nebula function that propagates errors with the ? operator.",
     "fn process(path) -> Result<Int, Error> {\n  lt data = read_file(path)?\n  lt n = parse_int(data)?\n  Ok(n * 2)\n}"),
    ("Write a Nebula function with try/catch error handling.",
     "fn run() {\n  try {\n    do_thing()?\n  } ch e {\n    prt(\"error: {e}\")\n  }\n}"),
    ("Write a Nebula function that returns a default on any error.",
     "fn get_or_default(key) = get(key) ?? \"default\""),
    ("Write a Nebula function that validates input and returns an error.",
     "fn validate(age) -> Result<Int, String> = if age < 0 { Err(\"negative age\") } el { Ok(age) }"),
    ("Write a Nebula function that chains three fallible operations.",
     "fn chain(x) -> Result<Int, Error> = step1(x)?.step2()?.step3()"),
]

# ─────────────────────────────────────────────────────────────────────────
# PATTERN MATCHING drills
# ─────────────────────────────────────────────────────────────────────────

MATCH_DRILLS = [
    ("Write a Nebula function that matches on a tuple of two bools.",
     "fn both(a, b) = mc (a, b) {\n  (tt, tt) => \"both\"\n  (tt, ff) => \"first\"\n  (ff, tt) => \"second\"\n  (ff, ff) => \"neither\"\n}"),
    ("Write a Nebula function matching on an Option type.",
     "fn describe(opt) = mc opt {\n  Some(x) => \"got {x}\"\n  None => \"nothing\"\n}"),
    ("Write a Nebula function matching on a Result type.",
     "fn show(res) = mc res {\n  Ok(x) => \"success: {x}\"\n  Err(e) => \"error: {e}\"\n}"),
    ("Write a Nebula function that matches a range of numbers.",
     "fn grade(score) = mc score {\n  90..=100 => \"A\"\n  80..=89 => \"B\"\n  70..=79 => \"C\"\n  60..=69 => \"D\"\n  _ => \"F\"\n}"),
    ("Write a Nebula function matching with guards.",
     "fn classify(n) = mc n {\n  n if n < 0 => \"negative\"\n  0 => \"zero\"\n  n if n % 2 == 0 => \"positive even\"\n  _ => \"positive odd\"\n}"),
    ("Write a Nebula function destructuring a tuple in match.",
     "fn sum_pair(p) = mc p {\n  (a, b) => a + b\n}"),
    ("Write a Nebula function matching on enum variants with fields.",
     "fn describe_shape(s) = mc s {\n  Circle(r) => \"circle r={r}\"\n  Square(side) => \"square {side}\"\n  Rectangle(w, h) => \"{w}x{h}\"\n}"),
    ("Write a Nebula function matching a string against literals.",
     "fn to_num(s) = mc s {\n  \"zero\" => 0\n  \"one\" => 1\n  \"two\" => 2\n  _ => -1\n}"),
]

# ─────────────────────────────────────────────────────────────────────────
# MORE VERBOSE → COMPACT REWRITES
# ─────────────────────────────────────────────────────────────────────────

EXTRA_VERBOSE_REWRITES = [
    (
        """fn is_valid(user: User) -> Bool {
  if user.name == \"\" {
    rt ff
  }
  if user.email == \"\" {
    rt ff
  }
  if user.age < 0 {
    rt ff
  }
  rt tt
}""",
        "fn is_valid(user) = user.name != \"\" and user.email != \"\" and user.age >= 0"
    ),
    (
        """fn first_or_default(xs: [Int], default: Int) -> Int {
  if xs.len() > 0 {
    rt xs[0]
  } el {
    rt default
  }
}""",
        "fn first_or_default(xs, default) = xs.first() ?? default"
    ),
    (
        """fn count_where(xs: [Int], pred: Fn(Int) -> Bool) -> Int {
  mt count = 0
  fo _, x in xs {
    if pred(x) {
      count += 1
    }
  }
  rt count
}""",
        "fn count_where(xs, pred) = xs.filter(pred).len()"
    ),
    (
        """fn process_each(xs: [Int]) {
  fo _, x in xs {
    prt(x)
  }
}""",
        "fn process_each(xs) { xs.each(prt) }"
    ),
    (
        """fn double_positives(xs: [Int]) -> [Int] {
  mt result = []
  fo _, x in xs {
    if x > 0 {
      result = psh(result, x * 2)
    }
  }
  rt result
}""",
        "fn double_positives(xs) = xs.filter(|x| x > 0).map(|x| x * 2)"
    ),
    (
        """fn sum_squares(xs: [Int]) -> Int {
  mt total = 0
  fo _, x in xs {
    total += x * x
  }
  rt total
}""",
        "fn sum_squares(xs) = xs.map(|x| x * x).fold(0, |a, b| a + b)"
    ),
    (
        """fn clamp(x: Int, lo: Int, hi: Int) -> Int {
  if x < lo {
    rt lo
  }
  if x > hi {
    rt hi
  }
  rt x
}""",
        "fn clamp(x, lo, hi) = if x < lo { lo } el if x > hi { hi } el { x }"
    ),
    (
        """fn distance(p1: Point, p2: Point) -> Float {
  lt dx = p2.x - p1.x
  lt dy = p2.y - p1.y
  rt (dx * dx + dy * dy).sqrt()
}""",
        "fn distance(p1, p2) = ((p2.x - p1.x) ** 2 + (p2.y - p1.y) ** 2).sqrt()"
    ),
    (
        """fn apply_twice(f: Fn(Int) -> Int, x: Int) -> Int {
  lt first = f(x)
  lt second = f(first)
  rt second
}""",
        "fn apply_twice(f, x) = f(f(x))"
    ),
    (
        """fn compose(f: Fn(Int) -> Int, g: Fn(Int) -> Int) -> Fn(Int) -> Int {
  rt |x| f(g(x))
}""",
        "fn compose(f, g) = |x| f(g(x))"
    ),
    (
        """fn longest(strings: [String]) -> String {
  mt longest = \"\"
  fo _, s in strings {
    if s.len() > longest.len() {
      longest = s
    }
  }
  rt longest
}""",
        "fn longest(strings) = strings.fold(\"\", |a, s| if s.len() > a.len() { s } el { a })"
    ),
    (
        """fn is_sorted(xs: [Int]) -> Bool {
  mt i = 1
  wl i < xs.len() {
    if xs[i] < xs[i - 1] {
      rt ff
    }
    i += 1
  }
  rt tt
}""",
        "fn is_sorted(xs) = xs.windows(2).all(|w| w[0] <= w[1])"
    ),
    (
        """fn index_of(xs: [Int], target: Int) -> Int {
  fo i, x in xs {
    if x == target {
      rt i
    }
  }
  rt -1
}""",
        "fn index_of(xs, target) = xs.enumerate().find(|_, x| x == target).map(|i, _| i) ?? -1"
    ),
    (
        """fn take_while(xs: [Int], pred: Fn(Int) -> Bool) -> [Int] {
  mt result = []
  fo _, x in xs {
    if pred(x) {
      result = psh(result, x)
    } el {
      bk
    }
  }
  rt result
}""",
        "fn take_while(xs, pred) = xs.take_while(pred)"
    ),
    (
        """fn read_config(path: String) -> Config? {
  try {
    lt data = read_file(path)
    lt parsed = parse_json(data)
    rt Some(parsed)
  } ch e {
    rt None
  }
}""",
        "fn read_config(path) = try { Some(parse_json(read_file(path))) } ch _ { None }"
    ),
    (
        """fn apply_discount(price: Float, percent: Float) -> Float {
  lt discount = price * percent / 100
  lt final = price - discount
  rt final
}""",
        "fn apply_discount(price, percent) = price * (1 - percent / 100)"
    ),
    (
        """fn greet(user: User) -> String {
  if user == nil {
    rt \"Hello, stranger\"
  }
  if user.name == \"\" {
    rt \"Hello, anonymous\"
  }
  rt \"Hello, {user.name}\"
}""",
        "fn greet(user) = mc user { nil => \"Hello, stranger\"; u if u.name == \"\" => \"Hello, anonymous\"; u => \"Hello, {u.name}\" }"
    ),
    (
        """fn total_price(items: [Item]) -> Float {
  mt total = 0.0
  fo _, item in items {
    total += item.price * item.quantity
  }
  rt total
}""",
        "fn total_price(items) = items.map(|i| i.price * i.quantity).fold(0, |a, b| a + b)"
    ),
    (
        """fn active_users(users: [User]) -> [User] {
  mt result = []
  fo _, u in users {
    if u.active and u.verified {
      result = psh(result, u)
    }
  }
  rt result
}""",
        "fn active_users(users) = users.filter(|u| u.active and u.verified)"
    ),
    (
        """fn group_by_age(users: [User]) -> Map<Int, [User]> {
  mt groups = {}
  fo _, u in users {
    if not groups.contains_key(u.age) {
      groups[u.age] = []
    }
    groups[u.age] = psh(groups[u.age], u)
  }
  rt groups
}""",
        "fn group_by_age(users) = users.fold({}, |acc, u| { acc[u.age] = psh(acc[u.age] ?? [], u); acc })"
    ),
    (
        """fn send_if_enabled(msg: Message) {
  if config.enabled {
    if config.verified {
      send(msg)
    }
  }
}""",
        "fn send_if_enabled(msg) { if config.enabled and config.verified { send(msg) } }"
    ),
    (
        """fn greet_all(names: [String]) -> [String] {
  mt greetings = []
  fo _, name in names {
    lt greeting = \"Hello, \" + name
    greetings = psh(greetings, greeting)
  }
  rt greetings
}""",
        "fn greet_all(names) = names.map(|n| \"Hello, {n}\")"
    ),
    (
        """fn keep_positive(xs: [Float]) -> [Float] {
  mt result = []
  fo _, x in xs {
    if x > 0.0 {
      result = psh(result, x)
    }
  }
  rt result
}""",
        "fn keep_positive(xs) = xs.filter(|x| x > 0)"
    ),
    (
        """fn first_adult(users: [User]) -> User? {
  fo _, u in users {
    if u.age >= 18 {
      rt Some(u)
    }
  }
  rt None
}""",
        "fn first_adult(users) = users.find(|u| u.age >= 18)"
    ),
    (
        """fn is_empty_or_blank(s: String) -> Bool {
  if s == \"\" {
    rt tt
  }
  if s.trim() == \"\" {
    rt tt
  }
  rt ff
}""",
        "fn is_empty_or_blank(s) = s.trim() == \"\""
    ),
]

# ─────────────────────────────────────────────────────────────────────────
# MORE GO → NEBULA with compression focus
# ─────────────────────────────────────────────────────────────────────────

EXTRA_GO_TO_NEBULA = [
    (
        """func Abs(x int) int {
    if x < 0 {
        return -x
    }
    return x
}""",
        "fn Abs(x) = if x < 0 { -x } el { x }"
    ),
    (
        """func Sign(x float64) int {
    if x > 0 {
        return 1
    } else if x < 0 {
        return -1
    }
    return 0
}""",
        "fn Sign(x) = if x > 0 { 1 } el if x < 0 { -1 } el { 0 }"
    ),
    (
        """func Clamp(x, lo, hi int) int {
    if x < lo {
        return lo
    }
    if x > hi {
        return hi
    }
    return x
}""",
        "fn Clamp(x, lo, hi) = if x < lo { lo } el if x > hi { hi } el { x }"
    ),
    (
        """func IsValid(u User) bool {
    return u.Name != \"\" && u.Email != \"\" && u.Age >= 0
}""",
        "fn IsValid(u) = u.Name != \"\" and u.Email != \"\" and u.Age >= 0"
    ),
    (
        """func FindByID(users []User, id int) *User {
    for i := range users {
        if users[i].ID == id {
            return &users[i]
        }
    }
    return nil
}""",
        "fn FindByID(users, id) = users.find(|u| u.ID == id)"
    ),
    (
        """func Names(users []User) []string {
    names := make([]string, len(users))
    for i, u := range users {
        names[i] = u.Name
    }
    return names
}""",
        "fn Names(users) = users.map(|u| u.Name)"
    ),
    (
        """func AverageAge(users []User) float64 {
    if len(users) == 0 {
        return 0
    }
    total := 0
    for _, u := range users {
        total += u.Age
    }
    return float64(total) / float64(len(users))
}""",
        "fn AverageAge(users) = if users.len() == 0 { 0 } el { users.map(|u| u.Age).fold(0, |a, b| a + b) / users.len() }"
    ),
    (
        """func Adults(users []User) []User {
    var result []User
    for _, u := range users {
        if u.Age >= 18 {
            result = append(result, u)
        }
    }
    return result
}""",
        "fn Adults(users) = users.filter(|u| u.Age >= 18)"
    ),
    (
        """func TotalPrice(items []Item) float64 {
    total := 0.0
    for _, item := range items {
        total += item.Price * float64(item.Qty)
    }
    return total
}""",
        "fn TotalPrice(items) = items.map(|i| i.Price * i.Qty).fold(0, |a, b| a + b)"
    ),
    (
        """func GroupByCategory(products []Product) map[string][]Product {
    result := make(map[string][]Product)
    for _, p := range products {
        result[p.Category] = append(result[p.Category], p)
    }
    return result
}""",
        "fn GroupByCategory(products) = products.fold({}, |acc, p| { acc[p.Category] = psh(acc[p.Category] ?? [], p); acc })"
    ),
    (
        """func WordCount(text string) map[string]int {
    counts := make(map[string]int)
    for _, word := range strings.Fields(text) {
        counts[strings.ToLower(word)]++
    }
    return counts
}""",
        "fn WordCount(text) = text.split_whitespace().fold({}, |acc, w| { lt k = w.to_lower(); acc[k] = (acc[k] ?? 0) + 1; acc })"
    ),
    (
        """func StripEmpty(strings []string) []string {
    result := make([]string, 0)
    for _, s := range strings {
        if s != \"\" {
            result = append(result, s)
        }
    }
    return result
}""",
        "fn StripEmpty(strings) = strings.filter(|s| s != \"\")"
    ),
    (
        """func Distinct(xs []int) []int {
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
        "fn Distinct(xs) = xs.fold([], |a, x| if a.contains(x) { a } el { a + [x] })"
    ),
    (
        """func HasDuplicates(xs []int) bool {
    seen := make(map[int]bool)
    for _, x := range xs {
        if seen[x] {
            return true
        }
        seen[x] = true
    }
    return false
}""",
        "fn HasDuplicates(xs) = xs.len() != xs.unique().len()"
    ),
    (
        """func Reverse(s string) string {
    runes := []rune(s)
    for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
        runes[i], runes[j] = runes[j], runes[i]
    }
    return string(runes)
}""",
        "fn Reverse(s) = s.chars().reverse().join(\"\")"
    ),
    (
        """func CountVowels(s string) int {
    count := 0
    for _, c := range s {
        if strings.ContainsRune(\"aeiouAEIOU\", c) {
            count++
        }
    }
    return count
}""",
        "fn CountVowels(s) = s.chars().filter(|c| \"aeiouAEIOU\".contains(c)).len()"
    ),
    (
        """func Palindrome(s string) bool {
    for i := 0; i < len(s)/2; i++ {
        if s[i] != s[len(s)-1-i] {
            return false
        }
    }
    return true
}""",
        "fn Palindrome(s) = s == s.chars().reverse().join(\"\")"
    ),
    (
        """func Fibonacci(n int) int {
    if n < 2 {
        return n
    }
    return Fibonacci(n-1) + Fibonacci(n-2)
}""",
        "fn Fibonacci(n) = if n < 2 { n } el { Fibonacci(n - 1) + Fibonacci(n - 2) }"
    ),
    (
        """func Factorial(n int) int {
    if n <= 1 {
        return 1
    }
    return n * Factorial(n-1)
}""",
        "fn Factorial(n) = if n <= 1 { 1 } el { n * Factorial(n - 1) }"
    ),
    (
        """func Power(base, exp int) int {
    result := 1
    for i := 0; i < exp; i++ {
        result *= base
    }
    return result
}""",
        "fn Power(base, exp) = (1..=exp).fold(1, |acc, _| acc * base)"
    ),
    (
        """func GCD(a, b int) int {
    for b != 0 {
        a, b = b, a%b
    }
    return a
}""",
        "fn GCD(a, b) = if b == 0 { a } el { GCD(b, a % b) }"
    ),
    (
        """func Sort(xs []int) []int {
    sort.Ints(xs)
    return xs
}""",
        "fn Sort(xs) = xs.sort()"
    ),
    (
        """func Unique(xs []string) []string {
    seen := make(map[string]bool)
    var result []string
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
        """func Take(xs []int, n int) []int {
    if n > len(xs) {
        n = len(xs)
    }
    return xs[:n]
}""",
        "fn Take(xs, n) = xs[0..n.min(xs.len())]"
    ),
    (
        """func Drop(xs []int, n int) []int {
    if n > len(xs) {
        return []int{}
    }
    return xs[n:]
}""",
        "fn Drop(xs, n) = if n >= xs.len() { [] } el { xs[n..] }"
    ),
    (
        """func Head(xs []int) (int, bool) {
    if len(xs) == 0 {
        return 0, false
    }
    return xs[0], true
}""",
        "fn Head(xs) = xs.first()"
    ),
    (
        """func Tail(xs []int) []int {
    if len(xs) == 0 {
        return xs
    }
    return xs[1:]
}""",
        "fn Tail(xs) = if xs.len() == 0 { xs } el { xs[1..] }"
    ),
    (
        """func Chunks(xs []int, size int) [][]int {
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
        "fn Chunks(xs, size) = (0..xs.len()).step(size).map(|i| xs[i..(i + size).min(xs.len())]).collect()"
    ),
    (
        """func MapKeys(m map[string]int) []string {
    keys := make([]string, 0, len(m))
    for k := range m {
        keys = append(keys, k)
    }
    return keys
}""",
        "fn MapKeys(m) = m.keys()"
    ),
    (
        """func MapValues(m map[string]int) []int {
    values := make([]int, 0, len(m))
    for _, v := range m {
        values = append(values, v)
    }
    return values
}""",
        "fn MapValues(m) = m.values()"
    ),
    (
        """func InvertMap(m map[string]int) map[int]string {
    result := make(map[int]string)
    for k, v := range m {
        result[v] = k
    }
    return result
}""",
        "fn InvertMap(m) = m.entries().fold({}, |acc, (k, v)| { acc[v] = k; acc })"
    ),
    (
        """func Merge(a, b map[string]int) map[string]int {
    result := make(map[string]int)
    for k, v := range a {
        result[k] = v
    }
    for k, v := range b {
        result[k] = v
    }
    return result
}""",
        "fn Merge(a, b) = b.entries().fold(a, |acc, (k, v)| { acc[k] = v; acc })"
    ),
    (
        """func Top3(xs []int) []int {
    sorted := make([]int, len(xs))
    copy(sorted, xs)
    sort.Sort(sort.Reverse(sort.IntSlice(sorted)))
    if len(sorted) > 3 {
        return sorted[:3]
    }
    return sorted
}""",
        "fn Top3(xs) = xs.sort().reverse().take(3)"
    ),
    (
        """func Median(xs []int) float64 {
    sorted := make([]int, len(xs))
    copy(sorted, xs)
    sort.Ints(sorted)
    n := len(sorted)
    if n%2 == 0 {
        return float64(sorted[n/2-1]+sorted[n/2]) / 2.0
    }
    return float64(sorted[n/2])
}""",
        "fn Median(xs) = { lt s = xs.sort(); lt n = s.len(); if n % 2 == 0 { (s[n/2 - 1] + s[n/2]) / 2.0 } el { s[n/2] } }"
    ),
    (
        """func FirstOrZero(xs []int) int {
    if len(xs) > 0 {
        return xs[0]
    }
    return 0
}""",
        "fn FirstOrZero(xs) = xs.first() ?? 0"
    ),
    (
        """func LastOrZero(xs []int) int {
    if len(xs) > 0 {
        return xs[len(xs)-1]
    }
    return 0
}""",
        "fn LastOrZero(xs) = xs.last() ?? 0"
    ),
    (
        """func SwapPairs(xs []int) []int {
    result := make([]int, len(xs))
    for i := 0; i < len(xs)-1; i += 2 {
        result[i] = xs[i+1]
        result[i+1] = xs[i]
    }
    if len(xs)%2 != 0 {
        result[len(xs)-1] = xs[len(xs)-1]
    }
    return result
}""",
        "fn SwapPairs(xs) = xs.chunks(2).flat_map(|pair| if pair.len() == 2 { [pair[1], pair[0]] } el { pair })"
    ),
    (
        """func AnyNegative(xs []int) bool {
    for _, x := range xs {
        if x < 0 {
            return true
        }
    }
    return false
}""",
        "fn AnyNegative(xs) = xs.any(|x| x < 0)"
    ),
    (
        """func AllPositive(xs []int) bool {
    for _, x := range xs {
        if x <= 0 {
            return false
        }
    }
    return true
}""",
        "fn AllPositive(xs) = xs.all(|x| x > 0)"
    ),
    (
        """func NonZero(xs []int) []int {
    var result []int
    for _, x := range xs {
        if x != 0 {
            result = append(result, x)
        }
    }
    return result
}""",
        "fn NonZero(xs) = xs.filter(|x| x != 0)"
    ),
    (
        """func Multiply(xs []int, n int) []int {
    result := make([]int, len(xs))
    for i, x := range xs {
        result[i] = x * n
    }
    return result
}""",
        "fn Multiply(xs, n) = xs.map(|x| x * n)"
    ),
    (
        """func MinMax(xs []int) (int, int) {
    min, max := xs[0], xs[0]
    for _, x := range xs {
        if x < min {
            min = x
        }
        if x > max {
            max = x
        }
    }
    return min, max
}""",
        "fn MinMax(xs) = xs.fold((xs[0], xs[0]), |(lo, hi), x| (if x < lo { x } el { lo }, if x > hi { x } el { hi }))"
    ),
    (
        """func TotalLength(strings []string) int {
    total := 0
    for _, s := range strings {
        total += len(s)
    }
    return total
}""",
        "fn TotalLength(strings) = strings.map(|s| s.len()).fold(0, |a, b| a + b)"
    ),
    (
        """func HasAllTags(item Item, tags []string) bool {
    for _, tag := range tags {
        found := false
        for _, t := range item.Tags {
            if t == tag {
                found = true
                break
            }
        }
        if !found {
            return false
        }
    }
    return true
}""",
        "fn HasAllTags(item, tags) = tags.all(|tag| item.Tags.contains(tag))"
    ),
    (
        """func JoinNonEmpty(parts []string, sep string) string {
    var filtered []string
    for _, p := range parts {
        if p != \"\" {
            filtered = append(filtered, p)
        }
    }
    return strings.Join(filtered, sep)
}""",
        "fn JoinNonEmpty(parts, sep) = parts.filter(|p| p != \"\").join(sep)"
    ),
    (
        """func ToggleCase(s string) string {
    var result strings.Builder
    for _, c := range s {
        if unicode.IsUpper(c) {
            result.WriteRune(unicode.ToLower(c))
        } else if unicode.IsLower(c) {
            result.WriteRune(unicode.ToUpper(c))
        } else {
            result.WriteRune(c)
        }
    }
    return result.String()
}""",
        "fn ToggleCase(s) = s.chars().map(|c| if c.is_upper() { c.to_lower() } el { c.to_upper() }).join(\"\")"
    ),
]

# ─────────────────────────────────────────────────────────────────────────
# Concurrency patterns (spawn/channels)
# ─────────────────────────────────────────────────────────────────────────

CONCURRENCY_DRILLS = [
    ("Write a Nebula function that spawns a goroutine to run a task.",
     "fn run_async(task) { sp task() }"),
    ("Write a Nebula function that creates a channel and sends a value.",
     "fn send_once(value) {\n  lt ch = Channel.new()\n  sp ch.send(value)\n}"),
    ("Write a Nebula async function that fetches data.",
     "ac fn fetch(url) -> Result<String, Error> {\n  lt resp = aw http.get(url)?\n  Ok(resp.body)\n}"),
    ("Write a Nebula function that runs two async tasks in parallel.",
     "ac fn both(url1, url2) {\n  lt a = sp fetch(url1)\n  lt b = sp fetch(url2)\n  (aw a, aw b)\n}"),
    ("Write a Nebula function that receives from a channel.",
     "fn consume(ch) { wl ch.has_more() { lt msg = ch.recv(); prt(msg) } }"),
]

# ─────────────────────────────────────────────────────────────────────────
# Tests
# ─────────────────────────────────────────────────────────────────────────

TEST_DRILLS = [
    ("Write a Nebula test for addition.",
     "#[test]\nfn test_add() {\n  assert_eq(add(2, 3), 5)\n}"),
    ("Write a Nebula test that checks a list is sorted.",
     "#[test]\nfn test_sort() {\n  assert_eq([3, 1, 2].sort(), [1, 2, 3])\n}"),
    ("Write a Nebula test for a function that might fail.",
     "#[test]\nfn test_divide() {\n  assert_eq(divide(10, 2), Ok(5))\n  assert_err(divide(10, 0))\n}"),
    ("Write a Nebula test with multiple assertions.",
     "#[test]\nfn test_string_ops() {\n  assert_eq(\"hello\".len(), 5)\n  assert(\"hello\".starts_with(\"he\"))\n  assert_eq(\"hello\".to_upper(), \"HELLO\")\n}"),
    ("Write a Nebula test that expects an error.",
     "#[test]\nfn test_invalid() {\n  assert_err(parse(\"not a number\"))\n}"),
]

# ─────────────────────────────────────────────────────────────────────────
# BONUS: Conversational Q&A pairs about Nebula idioms
# ─────────────────────────────────────────────────────────────────────────

QA_PAIRS = [
    ("What's the Nebula way to write a boolean return?",
     "Return the expression directly: `rt x > 0`. Never write `if x > 0 { rt tt } el { rt ff }` — that's verbose."),
    ("When should I use short-form `fn f(x) = expr`?",
     "Whenever the function body is a single expression. It's the shortest way to define simple functions."),
    ("What's the idiomatic way to provide defaults in Nebula?",
     "Use the nil-coalescing operator: `value ?? default`. Much shorter than if-checks."),
    ("How do I iterate and collect in Nebula?",
     "Use `map` + `collect`: `xs.map(|x| transform(x))`. Never use an accumulator loop."),
    ("How do I chain operations in Nebula?",
     "Use the pipe operator: `xs |> filter(pred) |> map(f) |> sum()`. Or method chaining: `xs.filter(pred).map(f).sum()`."),
    ("What's the shortest way to write if-else in Nebula?",
     "As an expression: `lt x = if cond { a } el { b }`. No return statements needed when assigning."),
    ("Why should I prefer `mc` over if-else chains?",
     "Match is more compact and handles ranges, guards, destructuring, and wildcards all in one. Use it whenever you have 3+ branches."),
    ("What's the Nebula idiom for counting by a key?",
     "`xs.fold({}, |acc, x| { acc[key(x)] = (acc[key(x)] ?? 0) + 1; acc })`. Fold is your friend for building maps."),
    ("How do I flatten a list of lists in Nebula?",
     "`xss.fold([], |a, xs| a + xs)` or if available `xss.flatten()`."),
    ("What's wrong with `fo _, x in xs { result = psh(result, f(x)) }`?",
     "That's just `xs.map(f)`. Always prefer map over loop+append."),
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

    # Struct/enum drills with multiple framings
    for q, a in STRUCT_DRILLS + ERROR_DRILLS + MATCH_DRILLS:
        pairs.append(make_pair(q, f"```nebula\n{a}\n```"))
        pairs.append(make_pair(
            q.replace("Write a Nebula", "Show me").replace(".", "."),
            f"```nebula\n{a}\n```"
        ))

    # Extra verbose rewrites
    for verbose, compact in EXTRA_VERBOSE_REWRITES:
        pairs.append(make_pair(
            f"Rewrite this verbose Nebula to be as compact as possible:\n\n```nebula\n{verbose}\n```",
            f"```nebula\n{compact}\n```"
        ))
        pairs.append(make_pair(
            f"Shorten this Nebula code:\n\n```nebula\n{verbose}\n```",
            f"```nebula\n{compact}\n```"
        ))
        pairs.append(make_pair(
            f"What's a more idiomatic Nebula version of:\n\n```nebula\n{verbose}\n```",
            f"```nebula\n{compact}\n```"
        ))

    # Extra Go → Nebula
    for go, neb in EXTRA_GO_TO_NEBULA:
        pairs.append(make_pair(
            f"Rewrite this Go in compact Nebula:\n\n```go\n{go}\n```",
            f"```nebula\n{neb}\n```"
        ))
        pairs.append(make_pair(
            f"What's the most compact Nebula equivalent?\n\n```go\n{go}\n```",
            f"```nebula\n{neb}\n```"
        ))
        pairs.append(make_pair(
            f"Translate to Nebula (shortest possible):\n\n```go\n{go}\n```",
            f"```nebula\n{neb}\n```"
        ))

    # Concurrency and tests (single framing, since rarer)
    for q, a in CONCURRENCY_DRILLS + TEST_DRILLS:
        pairs.append(make_pair(q, f"```nebula\n{a}\n```"))

    # Q&A pairs (conversational, no code block)
    for q, a in QA_PAIRS:
        pairs.append(make_pair(q, a))

    # Save
    out = Path(__file__).parent / "curated_pairs_extra.jsonl"
    with open(out, "w") as f:
        for p in pairs:
            f.write(json.dumps(p) + "\n")

    print(f"Generated {len(pairs)} extra curated pairs → {out}")
    print(f"Size: {out.stat().st_size:,} bytes")

    try:
        import tiktoken
        enc = tiktoken.get_encoding("cl100k_base")
        total = sum(len(enc.encode(json.dumps(p))) for p in pairs)
        print(f"Tokens: {total:,}")
    except ImportError:
        pass

    print(f"\nCategories:")
    print(f"  Struct drills:       {len(STRUCT_DRILLS)}")
    print(f"  Error drills:        {len(ERROR_DRILLS)}")
    print(f"  Match drills:        {len(MATCH_DRILLS)}")
    print(f"  Extra verbose rewrites: {len(EXTRA_VERBOSE_REWRITES)}")
    print(f"  Extra Go→Nebula:     {len(EXTRA_GO_TO_NEBULA)}")
    print(f"  Concurrency:         {len(CONCURRENCY_DRILLS)}")
    print(f"  Tests:               {len(TEST_DRILLS)}")
    print(f"  Q&A:                 {len(QA_PAIRS)}")


if __name__ == "__main__":
    main()
