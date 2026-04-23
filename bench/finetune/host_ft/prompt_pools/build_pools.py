"""Build `instruction_prompts.jsonl` + `anchor_prompts.jsonl` from existing
sources in the repo. $0 — no API calls, pure reshape.

instruction_prompts: ~650 seeds total (enough to seed Sonnet synthesis,
below the plan's 1,800 target — we'd supplement later). Row shape:
  {"prompt": "<user-facing coding prompt>", "source": "<tag>"}

anchor_prompts: ~220 seeds (HumanEval-as-prose + hand-crafted concept Qs).
  {"prompt": "<non-code-generation question>", "source": "<tag>"}
"""
from __future__ import annotations
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent.parent
HUMAN_EVAL = REPO / "bench" / "corpus" / "humaneval_py" / "humaneval.jsonl"

INSTRUCTION_OUT = HERE / "instruction_prompts.jsonl"
ANCHOR_OUT = HERE / "anchor_prompts.jsonl"


def build_instruction_pool() -> list[dict]:
    out: list[dict] = []

    # Source 1 — curated drill prompts (already used by slice2_direct.py, but
    # we re-seed so Sonnet synthesis can generate ADDITIONAL variants).
    sys.path.insert(0, str(REPO / "bench" / "finetune"))
    import curated_pairs as cp  # type: ignore
    import curated_pairs_extra as cpe  # type: ignore
    for i, (prompt, _) in enumerate(cp.DRILLS):
        out.append({"prompt": prompt, "source": f"curated_pairs.DRILLS:{i}"})
    for i, (prompt, _) in enumerate(cp.SHORTEST_PROMPTS):
        out.append({"prompt": prompt, "source": f"curated_pairs.SHORTEST_PROMPTS:{i}"})
    for list_name in ("CONCURRENCY_DRILLS", "ERROR_DRILLS", "MATCH_DRILLS",
                      "STRUCT_DRILLS", "TEST_DRILLS"):
        lst = getattr(cpe, list_name, [])
        for i, (prompt, _) in enumerate(lst):
            out.append({"prompt": prompt, "source": f"curated_pairs_extra.{list_name}:{i}"})

    # Source 2 — HumanEval prompts, stripped of the function header and
    # reframed as a natural coding request. We hold back 50 IDs for eval 2.
    if HUMAN_EVAL.exists():
        eval_holdout = set(f"HumanEval/{i}" for i in range(0, 50))
        for i, line in enumerate(HUMAN_EVAL.open()):
            line = line.strip()
            if not line: continue
            try: row = json.loads(line)
            except Exception: continue
            if row.get("task_id") in eval_holdout:
                continue
            raw = row.get("prompt", "")
            # Strip the `def X(...):` stub and docstring fences, keep the
            # description text as a plain prompt.
            import re
            m = re.search(r'"""(.*?)"""', raw, re.DOTALL)
            if not m:
                continue
            doc = m.group(1).strip()
            doc = re.sub(r"^\s*>>> .*$", "", doc, flags=re.MULTILINE).strip()
            if not doc or len(doc) < 30:
                continue
            name = row.get("entry_point", "the_function")
            prompt = f"Write a function `{name}` that {doc[:400]}"
            out.append({"prompt": prompt, "source": f"humaneval:{row.get('task_id')}"})

    # Dedupe on prompt text.
    seen = set()
    dedup = []
    for r in out:
        if r["prompt"] in seen:
            continue
        seen.add(r["prompt"])
        dedup.append(r)
    return dedup


ANCHOR_CONCEPT_QS = [
    # Language concepts
    ("What's the difference between Python's `is` and `==`?", "concept:python:is-vs-eq"),
    ("What's the difference between `let` and `const` in JavaScript?", "concept:js:let-vs-const"),
    ("Explain how the Go garbage collector handles goroutines.", "concept:go:gc"),
    ("What's the difference between a Rust `&str` and `String`?", "concept:rust:str-vs-string"),
    ("Explain what happens during Python class method resolution (MRO).", "concept:python:mro"),
    ("What's the difference between TypeScript's `any`, `unknown`, and `never`?", "concept:ts:any-unknown-never"),
    ("When should you use a Go channel vs a mutex?", "concept:go:channel-vs-mutex"),
    ("What's the big-O of Python's `list.insert(0, x)` and why?", "concept:python:list-insert"),
    ("Explain the difference between a shallow and deep copy in Python.", "concept:python:copy"),
    ("What's the difference between an iterator and a generator in Python?", "concept:python:iter-vs-gen"),
    # Debugging
    ("I'm getting `UnboundLocalError: local variable 'x' referenced before assignment` in Python. What causes this?", "debug:python:unboundlocal"),
    ("My Go goroutine never runs — what's likely wrong?", "debug:go:goroutine-schedule"),
    ("Why does my JavaScript `for...in` loop include properties like `length`?", "debug:js:for-in"),
    ("What does `UnsafeMutablePointer` mean in a Swift crash report?", "debug:swift:unsafemutable"),
    ("My Rust borrow checker is complaining about `cannot move out of borrowed content`. What do I do?", "debug:rust:move-out-borrowed"),
    ("My Python recursive function hits `RecursionError: maximum recursion depth exceeded`. How do I fix?", "debug:python:recursion-depth"),
    # Design / review
    ("When should I reach for a trie vs a hashmap?", "design:ds:trie-vs-hashmap"),
    ("What's the cleanest way to implement a retry-with-backoff in Python?", "design:python:retry-backoff"),
    ("Is it better to return multiple values or a tuple in Go?", "design:go:multi-return"),
    ("What's a good pattern for dependency injection in Rust?", "design:rust:di"),
    # Explain code (paste-and-ask)
    ("What does this Python expression do? `[x for x in xs if x > 0][:5]`", "explain:python:listcomp-slice"),
    ("Explain this Go idiom: `if err != nil { return err }`.", "explain:go:err-return"),
    ("What does `Object.freeze(obj)` do in JavaScript, and what are its limits?", "explain:js:freeze"),
    ("What's the difference between `async fn` and `fn -> impl Future` in Rust?", "explain:rust:async-vs-future"),
    # Git / tooling
    ("How do I undo the last commit but keep the changes staged?", "git:undo-commit"),
    ("What's `git rebase -i` for and when is it dangerous?", "git:rebase-i"),
    ("What does `go mod tidy` do?", "tool:go:mod-tidy"),
    ("What's the difference between `pip install -e .` and `pip install .`?", "tool:python:pip-e"),
    # Perf / complexity
    ("What's the time complexity of a Python set lookup?", "perf:python:set-lookup"),
    ("Why is `std::vec::Vec::push` amortized O(1) in Rust?", "perf:rust:vec-push"),
    # Patterns
    ("What is the builder pattern and when is it worth the boilerplate?", "pattern:builder"),
    ("Explain what a 'context manager' is in Python.", "pattern:python:context-manager"),
    # Regex / small mechanics
    ("In the regex `/\\b\\w+\\b/`, what does `\\b` mean?", "regex:word-boundary"),
    ("Why does `0.1 + 0.2 != 0.3` in most languages?", "mechanics:float"),
    # Stack trace reads
    ("I see `panic: runtime error: index out of range [5] with length 3` in Go — where do I look?", "debug:go:oob"),
    ("What does a Node.js `EADDRINUSE` error mean?", "debug:node:eaddrinuse"),
]


def build_anchor_pool() -> list[dict]:
    out: list[dict] = []
    for prompt, source in ANCHOR_CONCEPT_QS:
        out.append({"prompt": prompt, "source": source})
    # Source 2 — "explain the algorithm" HumanEval reframings. Different
    # from slice 2 — we don't ask for code, we ask for prose description.
    if HUMAN_EVAL.exists():
        for i, line in enumerate(HUMAN_EVAL.open()):
            if i >= 80: break
            line = line.strip()
            if not line: continue
            try: row = json.loads(line)
            except Exception: continue
            import re
            m = re.search(r'"""(.*?)"""', row.get("prompt", ""), re.DOTALL)
            if not m: continue
            doc = m.group(1).strip()
            doc = re.sub(r"^\s*>>> .*$", "", doc, flags=re.MULTILINE).strip()
            if not doc: continue
            name = row.get("entry_point", "the_function")
            prompt = f"Describe in prose (not code) how you'd implement `{name}`: {doc[:300]}"
            out.append({"prompt": prompt, "source": f"humaneval-prose:{row.get('task_id')}"})
    # Dedup.
    seen = set()
    dedup = []
    for r in out:
        if r["prompt"] in seen: continue
        seen.add(r["prompt"])
        dedup.append(r)
    return dedup


def main() -> None:
    inst = build_instruction_pool()
    with INSTRUCTION_OUT.open("w") as f:
        for r in inst:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"instruction_prompts: {len(inst)} rows → {INSTRUCTION_OUT}")

    anchor = build_anchor_pool()
    with ANCHOR_OUT.open("w") as f:
        for r in anchor:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"anchor_prompts:      {len(anchor)} rows → {ANCHOR_OUT}")


if __name__ == "__main__":
    main()
