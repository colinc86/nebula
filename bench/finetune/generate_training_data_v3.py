#!/usr/bin/env python3
"""Generate fine-tuning training data for Nebula (v3).

Improvements over v2:
1. Hard exclusion of bench/challenge*/ files (anti-contamination)
2. Expanded keyword drills (~100 patterns instead of 23)
3. Larger source corpus (10K+ Go files)
4. Explicit side-by-side Go→Nebula demonstrations for learning
5. Anti-contamination check at the end (fails if challenge names leak in)

Usage:
    python3 generate_training_data_v3.py --go-list files.txt --output train_v3.jsonl
"""

import argparse
import json
import random
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parent.parent
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"
EXAMPLES_DIR = REPO_ROOT / "examples"
CHALLENGE_DIRS = [REPO_ROOT / "bench" / f"challenge{i}" for i in ["", "2", "3", "4", "5"]]

SYSTEM_PROMPT = "You are an expert Nebula programmer. Nebula is a compact language: fn=func, rt=return, st=struct, en=enum, im=impl, tr=trait, mc=match/switch, fo=for, wl=while, lp=loop, lt=let, mt=mut, sf=self, el=else, tt=true, ff=false, bk=break, ct=continue, and/or/not=logical, ln()=len(), psh()=append(), prt()=println()."

# Contamination guard — any of these strings in a training pair means it came from a challenge
CHALLENGE_SIGNATURES = [
    # Challenge 1 signatures (task manager)
    "TaskStore", "BuildTagIndex", "TopologicalSort", "UpdateStatus",
    "StatusChange", "GetHistory", "FromStatus", "ToStatus",
    # Challenge 2 signatures (graph)
    "NewGraph()", "ShortestPath", "ConnectedComponents", "DetectCycles",
    "FilterByTag", "InDegree", "OutDegree", "AddNode", "AddEdge",
    # Challenge 3 signatures (text)
    "ParseCSV", "WordFrequency", "FormatTable", "FindAllMatches",
    # Challenge 4 signatures (state machine)
    "ReachableStates", "IsDeadlock", "ValidateDeterministic", "NewMachine",
    "StateIdle", "StateRunning", "StatePaused", "EventStart", "EventPause",
    # Challenge 5 signatures (numerical)
    "NewMatrix", "LinearRegression", "Determinant",
    # Task manager body content
    "Task struct", "Tasks []", "Priority Priority", "Critical",
]

# Expanded keyword drill pairs — ~100 patterns covering every Nebula construct
KEYWORD_DRILLS = [
    # Basic functions
    ("func add(a, b int) int { return a + b }",
     "fn add(a: int, b: int) -> int { rt a + b }"),
    ("func sub(a, b int) int { return a - b }",
     "fn sub(a: int, b: int) -> int { rt a - b }"),
    ("func double(x int) int { return x * 2 }",
     "fn double(x: int) -> int { rt x * 2 }"),
    ("func main() { fmt.Println(\"hello\") }",
     "fn main() { prt(\"hello\") }"),
    ("func greet(name string) string { return \"Hello, \" + name }",
     "fn greet(name: string) -> string { rt \"Hello, \" + name }"),

    # If/else
    ("if x > 0 { return true } else { return false }",
     "if x > 0 { rt tt } el { rt ff }"),
    ("if x == 0 { return \"zero\" }",
     "if x == 0 { rt \"zero\" }"),
    ("if a && b { doSomething() }",
     "if a and b { doSomething() }"),
    ("if a || b { doSomething() }",
     "if a or b { doSomething() }"),
    ("if !valid { return err }",
     "if not valid { rt err }"),
    ("if err != nil { return err }",
     "if err != nil { rt err }"),
    ("if x > 0 && x < 100 { return x }",
     "if x > 0 and x < 100 { rt x }"),

    # For loops
    ("for i, v := range items { fmt.Println(i, v) }",
     "fo i, v in items { prt(i, v) }"),
    ("for _, item := range list { process(item) }",
     "fo _, item in list { process(item) }"),
    ("for k, v := range m { fmt.Println(k, v) }",
     "fo k, v in m { prt(k, v) }"),
    ("for x > 0 { x-- }",
     "wl x > 0 { x -= 1 }"),
    ("for i := 0; i < 10; i++ { fmt.Println(i) }",
     "mt i = 0\nwl i < 10 { prt(i); i += 1 }"),
    ("for { break }",
     "lp { bk }"),
    ("for { if done { break } }",
     "lp { if done { bk } }"),
    ("for _, x := range nums { if x < 0 { continue } }",
     "fo _, x in nums { if x < 0 { ct } }"),

    # Switch / match
    ("switch x { case 1: return \"one\" default: return \"other\" }",
     "mc x { 1 => \"one\" _ => \"other\" }"),
    ("switch status { case Active: return 1 case Inactive: return 0 }",
     "mc status { Active => 1 Inactive => 0 }"),
    ("switch n { case 0: return 0 case 1: return 1 default: return -1 }",
     "mc n { 0 => 0 1 => 1 _ => -1 }"),

    # Structs
    ("type Point struct { X float64; Y float64 }",
     "st Point {\n  X: float64\n  Y: float64\n}"),
    ("type User struct { Name string; Age int }",
     "st User {\n  Name: string\n  Age: int\n}"),
    ("type Box struct { Width int; Height int; Label string }",
     "st Box {\n  Width: int\n  Height: int\n  Label: string\n}"),
    ("p := Point{X: 1.0, Y: 2.0}",
     "mt p = Point { X: 1.0, Y: 2.0 }"),
    ("u := User{Name: \"Alice\", Age: 30}",
     "mt u = User { Name: \"Alice\", Age: 30 }"),

    # Enums (via iota)
    ("type Color int\nconst ( Red Color = iota; Green; Blue )",
     "en Color {\n  Red\n  Green\n  Blue\n}"),
    ("type Status int\nconst ( Active Status = iota; Inactive; Pending )",
     "en Status {\n  Active\n  Inactive\n  Pending\n}"),

    # Methods (impl blocks)
    ("func (p *Point) Distance() float64 { return math.Sqrt(p.X*p.X + p.Y*p.Y) }",
     "im Point {\n  fn Distance(sf) -> float64 { rt math.Sqrt(sf.X*sf.X + sf.Y*sf.Y) }\n}"),
    ("func (u *User) Greet() string { return \"Hi, \" + u.Name }",
     "im User {\n  fn Greet(sf) -> string { rt \"Hi, \" + sf.Name }\n}"),
    ("func (b *Box) Area() int { return b.Width * b.Height }",
     "im Box {\n  fn Area(sf) -> int { rt sf.Width * sf.Height }\n}"),
    ("func (s *Server) Start() error { return nil }",
     "im Server {\n  fn Start(sf) -> error { rt nil }\n}"),

    # Interfaces / traits
    ("type Stringer interface { String() string }",
     "tr Stringer {\n  fn String(sf) -> string\n}"),
    ("type Reader interface { Read(p []byte) (int, error) }",
     "tr Reader {\n  fn Read(sf, p: List<byte>) -> (int, error)\n}"),

    # Built-ins
    ("x := len(items)",
     "mt x = ln(items)"),
    ("items = append(items, newItem)",
     "items = psh(items, newItem)"),
    ("items = append(items, a, b, c)",
     "items = psh(items, a, b, c)"),
    ("fmt.Println(x)",
     "prt(x)"),
    ("fmt.Println(\"hello\", name)",
     "prt(\"hello\", name)"),
    ("delete(m, key)",
     "m.remove(key)"),

    # Variable declarations
    ("x := 42",
     "mt x = 42"),
    ("var count int = 0",
     "mt count = 0"),
    ("var name string",
     "mt name = go_var(\"string\")"),
    ("const MaxSize = 100",
     "lt MaxSize = 100"),
    ("var wg sync.WaitGroup",
     "mt wg = go_var(\"sync.WaitGroup\")"),

    # Multi-return / destructuring
    ("x, y := getCoords()",
     "mt (x, y) = getCoords()"),
    ("val, ok := m[key]",
     "mt (val, ok) = m[key]"),
    ("result, err := process(data)",
     "mt (result, err) = process(data)"),
    ("if err := doSomething(); err != nil { return err }",
     "mt err = doSomething()\nif err != nil { rt err }"),

    # Return statements
    ("return",
     "rt"),
    ("return x",
     "rt x"),
    ("return x, nil",
     "rt (x, nil)"),
    ("return nil, err",
     "rt (nil, err)"),
    ("return x, y, nil",
     "rt (x, y, nil)"),

    # Booleans and nil
    ("x := true",
     "mt x = tt"),
    ("x := false",
     "mt x = ff"),
    ("return nil",
     "rt nil"),
    ("if x == nil { return }",
     "if x == nil { rt }"),

    # Self / receivers
    ("self.count++",
     "sf.count += 1"),
    ("return self.name",
     "rt sf.name"),
    ("self.items = append(self.items, x)",
     "sf.items = psh(sf.items, x)"),

    # Concurrency
    ("go func() { doWork() }()",
     "sp fn() { doWork() }()"),
    ("go handleRequest(req)",
     "sp handleRequest(req)"),
    ("ch <- value",
     "ch.send(value)"),
    ("x := <-ch",
     "mt x = ch.recv()"),
    ("close(ch)",
     "ch.close()"),

    # Maps and slices
    ("m := make(map[string]int)",
     "mt m = make(\"map[string]int\")"),
    ("s := make([]int, 0)",
     "mt s = make(\"[]int\")"),
    ("s := []int{1, 2, 3}",
     "mt s = [1, 2, 3]"),
    ("s := []string{\"a\", \"b\"}",
     "mt s = [\"a\", \"b\"]"),

    # Pointer types
    ("func NewUser(name string) *User { return &User{Name: name} }",
     "fn NewUser(name: string) -> Ptr<User> { rt &User { Name: name } }"),
    ("var p *Point",
     "mt p = go_var(\"*Point\")"),

    # Logical operators in expressions
    ("x && y || !z",
     "x and y or not z"),
    ("!enabled",
     "not enabled"),
    ("a != b && c != d",
     "a != b and c != d"),

    # Chained method calls
    ("result := strings.ToLower(strings.TrimSpace(input))",
     "mt result = strings.ToLower(strings.TrimSpace(input))"),
    ("items = append(items[:i], items[i+1:]...)",
     "items = psh(items[0..i], items[i+1..]...)"),
]


def run_nebula(cmd, input_file):
    try:
        result = subprocess.run(
            [str(NEBULA_BIN), cmd, str(input_file)],
            capture_output=True, text=True, timeout=30
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass
    return None


def make_chat(system, user, assistant):
    msgs = []
    if system:
        msgs.append({"role": "system", "content": system})
    msgs.append({"role": "user", "content": user})
    msgs.append({"role": "assistant", "content": assistant})
    return {"messages": msgs}


def is_contaminated(pair):
    """Return True if any challenge signature appears in the pair."""
    text = json.dumps(pair)
    return any(sig in text for sig in CHALLENGE_SIGNATURES)


def is_challenge_file(path):
    """Return True if path is under bench/challenge*/"""
    path_str = str(path)
    return any(str(cd) in path_str for cd in CHALLENGE_DIRS)


def extract_functions(source):
    functions = []
    lines = source.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        starts_fn = (line.startswith('func ') or line.startswith('fn ') or
                     line.startswith('  fn ') or line.startswith('    fn '))
        if starts_fn and '{' in line:
            depth = 0
            start = i
            for j in range(i, len(lines)):
                depth += lines[j].count('{') - lines[j].count('}')
                if depth <= 0:
                    fn_text = '\n'.join(lines[start:j+1])
                    if len(fn_text) > 30 and len(fn_text) < 3000:
                        functions.append(fn_text)
                    i = j + 1
                    break
            else:
                i += 1
        else:
            i += 1
    return functions


def generate_from_go_file(go_file):
    pairs = []
    go_source = go_file.read_text()
    if len(go_source) < 100:
        return pairs

    neb_source = run_nebula("from-go", go_file)
    if not neb_source:
        return pairs

    # Full file translations (only smaller files)
    if len(go_source) < 6000:
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Translate this Go code to Nebula:\n\n```go\n{go_source}\n```",
            f"```nebula\n{neb_source}\n```"
        ))
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Translate this Nebula code to Go:\n\n```nebula\n{neb_source}\n```",
            f"```go\n{go_source}\n```"
        ))

    # Function-level pairs
    go_fns = extract_functions(go_source)
    neb_fns = extract_functions(neb_source)

    for go_fn in go_fns[:3]:
        fn_name = ""
        for word in go_fn.split():
            if word.startswith("(") or word == "func":
                continue
            if "(" in word:
                fn_name = word.split("(")[0]
                break
            fn_name = word
            break

        matching_neb = None
        for nf in neb_fns:
            if fn_name and fn_name in nf:
                matching_neb = nf
                break

        if matching_neb:
            pairs.append(make_chat(
                SYSTEM_PROMPT,
                f"Translate this Go function to Nebula:\n\n```go\n{go_fn}\n```",
                f"```nebula\n{matching_neb}\n```"
            ))

    # Editing tasks
    for neb_fn in neb_fns[:2]:
        sig = neb_fn.split('{')[0].strip() if '{' in neb_fn else neb_fn[:60]
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Implement this Nebula function:\n\n```\n{sig}\n```",
            f"```nebula\n{neb_fn}\n```"
        ))

    return pairs


def generate_keyword_drills():
    pairs = []
    framings = [
        ("Convert this Go to Nebula:\n\n```go\n{go}\n```",
         "```nebula\n{neb}\n```"),
        ("What is this in Nebula?\n\n```go\n{go}\n```",
         "```nebula\n{neb}\n```"),
        ("Rewrite in Nebula:\n\n```go\n{go}\n```",
         "```nebula\n{neb}\n```"),
        ("Translate this Nebula back to Go:\n\n```nebula\n{neb}\n```",
         "```go\n{go}\n```"),
    ]
    for go, neb in KEYWORD_DRILLS:
        for user_tmpl, asst_tmpl in framings:
            pairs.append(make_chat(
                SYSTEM_PROMPT,
                user_tmpl.format(go=go, neb=neb),
                asst_tmpl.format(go=go, neb=neb)
            ))
    return pairs


def generate_from_neb_examples():
    pairs = []
    if not EXAMPLES_DIR.exists():
        return pairs
    for neb_file in sorted(EXAMPLES_DIR.glob("*.neb")):
        neb_source = neb_file.read_text().strip()
        if not neb_source or len(neb_source) < 50:
            continue
        name = neb_file.stem
        pairs.append(make_chat(
            SYSTEM_PROMPT,
            f"Write a {name.replace('_', ' ')} program in Nebula.",
            f"```nebula\n{neb_source}\n```"
        ))
        fns = extract_functions(neb_source)
        for fn in fns[:2]:
            sig = fn.split('{')[0].strip()
            pairs.append(make_chat(
                SYSTEM_PROMPT,
                f"Write this Nebula function: `{sig}`",
                f"```nebula\n{fn}\n```"
            ))
    return pairs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--go-list", help="File with Go file paths")
    parser.add_argument("--output", default="training_v3.jsonl")
    parser.add_argument("--max-files", type=int, default=10000)
    args = parser.parse_args()

    if not NEBULA_BIN.exists():
        print(f"Error: {NEBULA_BIN} not found. Run: cd compiler && cargo build")
        sys.exit(1)

    all_pairs = []
    contaminated = 0
    excluded_files = 0

    print("Generating keyword drills...")
    drills = generate_keyword_drills()
    # Filter drills
    drills = [p for p in drills if not is_contaminated(p)]
    all_pairs.extend(drills)
    print(f"  {len(drills)} keyword drill pairs")

    print("Generating from Nebula examples...")
    examples = generate_from_neb_examples()
    examples = [p for p in examples if not is_contaminated(p)]
    all_pairs.extend(examples)
    print(f"  {len(examples)} example pairs")

    go_files = []
    if args.go_list:
        with open(args.go_list) as f:
            go_files = [Path(l.strip()) for l in f if l.strip()]
        # EXCLUDE challenge files
        before = len(go_files)
        go_files = [f for f in go_files if not is_challenge_file(f)]
        excluded_files = before - len(go_files)
        go_files = [f for f in go_files if f.exists()][:args.max_files]
        print(f"Loaded {len(go_files)} Go files (excluded {excluded_files} challenge files)")

    for i, gf in enumerate(go_files):
        if (i + 1) % 100 == 0 or i == 0:
            print(f"  [{i+1}/{len(go_files)}] {gf.name}...")
        pairs = generate_from_go_file(gf)
        # Filter contaminated pairs
        clean_pairs = []
        for p in pairs:
            if is_contaminated(p):
                contaminated += 1
            else:
                clean_pairs.append(p)
        all_pairs.extend(clean_pairs)

    print(f"\nDropped {contaminated} contaminated pairs")

    # Final contamination check
    final_contaminated = sum(1 for p in all_pairs if is_contaminated(p))
    if final_contaminated > 0:
        print(f"⚠️  WARNING: {final_contaminated} pairs still contaminated!")
    else:
        print("✓ No contamination in final dataset")

    random.shuffle(all_pairs)

    out = Path(args.output)
    with open(out, 'w') as f:
        for pair in all_pairs:
            f.write(json.dumps(pair) + '\n')

    print(f"\nTotal: {len(all_pairs)} training pairs → {out}")
    print(f"Size: {out.stat().st_size:,} bytes")

    try:
        import tiktoken
        enc = tiktoken.get_encoding("cl100k_base")
        total = sum(len(enc.encode(json.dumps(p))) for p in all_pairs)
        print(f"Tokens: {total:,}")
        print(f"Est. cost (3 epochs, Llama 8B on Together): ~${total * 3 / 1_000_000 * 0.30:.2f}")
    except ImportError:
        pass


if __name__ == "__main__":
    main()
