# Nebula Benchmark Challenge

Measures whether compact code representation reduces LLM token usage during coding tasks.

## What's Here

```
main.go          Go task manager (~450 lines) — the code to modify
main_test.go     16 passing tests + 4 skipped challenge tests
main.neb         Compact Nebula representation of main.go (created by setup.sh)
main_test.neb    Compact Nebula representation of main_test.go
setup.sh         Creates .neb files from Go source
run_challenge.sh Verification: runs tests + measures compression
go.mod           Go module
```

## The Challenge

Implement 4 features in `main.go` to make all 4 skipped tests pass, without breaking the 16 existing tests.

| # | Feature | What to Build |
|---|---------|---------------|
| 1 | TagIndex | `BuildTagIndex()` — inverted index: tag → tasks |
| 2 | TopologicalSort | `TopologicalSort()` — dependency order + cycle detection |
| 3 | TaskHistory | `UpdateStatus()` + `GetHistory()` — status change tracking |
| 4 | Serialization | `MarshalJSON()` + `UnmarshalJSON()` — full JSON round-trip |

**Rules:** Remove each `t.Skip()`, don't modify the test file, all 20 tests must pass.

---

## Benchmark Protocol

Run the challenge **twice** — once normally (control), once with Nebula compact files — and compare token usage.

### Prerequisites

```sh
cd compiler && cargo build         # Build Nebula compiler
pip install tiktoken               # Optional: token counting
```

### Run A: Control (vanilla Go)

1. **Ensure clean state** — no `.neb` files present:
   ```sh
   cd bench/challenge
   git checkout -- main.go main_test.go
   ./setup.sh clean
   ```

2. **Open a fresh Claude Code session**

3. **Start timer**, paste this prompt:
   ```
   In this directory there is a Go project. main_test.go has 4 skipped challenge
   tests (TagIndex, TopologicalSort, TaskHistory, Serialization).

   Implement all 4 features in main.go to make the tests pass. Remove each
   t.Skip() line as you implement the feature. Do not modify main_test.go.

   When done, run: go test -v -count=1 ./...
   All 20 tests must pass.
   ```

4. **Stop timer** when done. Record: time, tokens (`/cost`), correctness.

5. **Save result:** `cp main.go main_control.go`

### Run B: Nebula (compact reading)

1. **Reset and generate .neb files:**
   ```sh
   git checkout -- main.go main_test.go
   ./setup.sh
   ```

2. **Open a fresh Claude Code session**

3. **Start timer**, paste this prompt:
   ```
   In this directory there is a Go project. The .neb files are compact
   representations of the Go source — read those to understand the codebase
   (they use short keywords: fn=func, rt=return, st=struct, mc=switch, sf=self,
   fo=for, el=else, lt=let, mt=mut, tt=true, ff=false).

   main_test.neb has 4 skipped challenge tests (TagIndex, TopologicalSort,
   TaskHistory, Serialization).

   Implement all 4 features by writing Go code to main.go. Remove each
   t.Skip() line from main_test.go. Do not change the test logic.

   When done, run: go test -v -count=1 ./...
   All 20 tests must pass.
   ```

4. **Stop timer**. Record same metrics.

5. **Save result:** `cp main.go main_nebula.go`

### Verify

```sh
./run_challenge.sh
```

---

## Results Table

```
| Metric               | Control (Go) | Nebula (.neb) |
|----------------------|--------------|---------------|
| Time (seconds)       |              |               |
| Input tokens         |              |               |
| Output tokens        |              |               |
| Total tokens         |              |               |
| Cost ($)             |              |               |
| Existing tests (/16) |              |               |
| Challenge tests (/4) |              |               |
| Correctness (0-3)    |              |               |
```

**Correctness:** 3 = all pass + clean code, 2 = all pass + minor issues, 1 = some fail, 0 = doesn't compile

---

## Why This Works

The model **reads** Nebula (compact, fewer input tokens) but **writes** Go (which it already knows). This isolates the variable: does reading compressed code reduce total token cost?

- No new language to learn — just short keywords to decode
- No MCP tools — standard Read/Write/Edit
- No round-trip issues — Go in, Go out
- The CLAUDE.md provides a one-line keyword legend, nothing more
