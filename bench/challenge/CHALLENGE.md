# Nebula Benchmark Challenge

A standardized test for measuring Nebula's value as an LLM coding medium.

## What's Here

- `main.go` — A ~450-line Go task management system with concurrency, algorithms, and data structures
- `main_test.go` — 16 passing tests + **4 skipped challenge tests** defining features to implement
- `run_challenge.sh` — Automated verification: runs tests, measures token compression
- `go.mod` — Go module file

## The Challenge

**Make all 4 challenge tests pass** without breaking the 16 existing tests.

### The 4 Features to Implement

| # | Feature | Difficulty | What to Build |
|---|---------|-----------|---------------|
| 1 | **TagIndex** | Easy | `BuildTagIndex()` — inverted index mapping tags to tasks |
| 2 | **TopologicalSort** | Medium | `TopologicalSort()` — dependency-order traversal with cycle detection |
| 3 | **TaskHistory** | Medium | `UpdateStatus()` + `GetHistory()` — status change tracking with timestamps |
| 4 | **Serialization** | Medium | `MarshalJSON()` + `UnmarshalJSON()` — full round-trip including NextID |

### Rules
1. Remove the `t.Skip(...)` line from each test you implement
2. All 20 tests must pass: `go test -v -count=1 ./...`
3. Do not modify the test file
4. All new code goes in `main.go`

---

## How to Run the Benchmark

### Prerequisites

```sh
# 1. Build the Nebula compiler
cd compiler && cargo build

# 2. Install tiktoken for token counting (optional but recommended)
pip install tiktoken
```

### Step-by-step Protocol

You will run this challenge **twice** — once with vanilla Claude Code, once through Nebula — and compare results.

#### Run A: Control (vanilla Claude Code)

1. **Reset to clean state:**
   ```sh
   cd bench/challenge
   git checkout -- main.go main_test.go
   ```

2. **Open a fresh Claude Code session** (no Nebula MCP server configured)

3. **Start a timer**, then paste this exact prompt:
   ```
   I have a Go project in bench/challenge/. The file main_test.go has 4 skipped
   challenge tests (TagIndex, TopologicalSort, TaskHistory, Serialization).

   Implement all 4 features in main.go to make the tests pass. Remove each
   t.Skip() line as you implement the feature. Do not modify main_test.go.

   When done, run: go test -v -count=1 ./...
   All 20 tests must pass.
   ```

4. **Stop the timer** when Claude says it's done

5. **Record results:**
   - Time (wall clock from Enter to completion)
   - Tokens used (from Claude Code session summary / `/cost`)
   - Run `./run_challenge.sh` to get correctness + compression metrics

6. **Save the modified main.go** as `main_control.go` for comparison:
   ```sh
   cp main.go main_control.go
   ```

#### Run B: Nebula MCP

1. **Reset to clean state:**
   ```sh
   git checkout -- main.go main_test.go
   ```

2. **Configure the Nebula MCP server** in Claude Code settings:
   ```json
   {
     "mcpServers": {
       "nebula": {
         "command": "/absolute/path/to/compiler/target/debug/nebula",
         "args": ["mcp"]
       }
     }
   }
   ```

3. **Open a fresh Claude Code session**

4. **Start a timer**, then paste this exact prompt:
   ```
   I have a Go project in bench/challenge/. The file main_test.go has 4 skipped
   challenge tests (TagIndex, TopologicalSort, TaskHistory, Serialization).

   Implement all 4 features in main.go to make the tests pass. Remove each
   t.Skip() line as you implement the feature. Do not modify main_test.go.

   Use the nebula MCP tools (read_code, write_code, search_code) for all file
   operations instead of the built-in Read/Write tools.

   When done, run: go test -v -count=1 ./...
   All 20 tests must pass.
   ```

5. **Stop the timer** and record the same metrics

6. **Save:** `cp main.go main_nebula.go`

### Verification

After each run:
```sh
./run_challenge.sh
```

This automatically checks:
- All 16 existing tests still pass (no regressions)
- How many of the 4 challenge features were implemented
- Line/byte/token compression ratios (Go vs Nebula representation)

---

## What to Report

Fill in this table after both runs:

```
| Metric               | Control (vanilla) | Nebula MCP |
|----------------------|-------------------|------------|
| Time (seconds)       |                   |            |
| Input tokens         |                   |            |
| Output tokens        |                   |            |
| Total tokens         |                   |            |
| Cost ($)             |                   |            |
| Existing tests (/16) |                   |            |
| Challenge tests (/4) |                   |            |
| Correctness (0-3)    |                   |            |
```

**Correctness scoring:**
| Score | Meaning |
|-------|---------|
| 3     | All 20 tests pass, code is clean |
| 2     | All tests pass but code has minor issues (style, unnecessary complexity) |
| 1     | Some tests fail or code has logic errors |
| 0     | Doesn't compile or couldn't complete |

**Bonus metrics** (from `run_challenge.sh`):
- Line compression %
- Byte compression %
- Token compression % (requires tiktoken)

---

## Tips

- Use the **same model** for both runs (e.g., both Sonnet, or both Opus)
- Run control first so you know what "normal" looks like
- If Claude gets stuck in either mode, note how many back-and-forths it took
- The challenge is designed to take 2-5 minutes for Claude — complex enough to be meaningful but not so hard it's random
