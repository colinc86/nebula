---
name: run-neb
description: Run, check, lint, or format a Nebula (.neb) file
user_invocable: true
---

# Run a Nebula File

Run a `.neb` file using the Nebula compiler. All commands run from the `compiler/` directory.

## Usage

The user may provide arguments like:
- `/run-neb examples/hello.neb` — run the file
- `/run-neb check examples/hello.neb` — parse-check only
- `/run-neb lint examples/hello.neb` — run linter
- `/run-neb fmt examples/hello.neb` — format to canonical style
- `/run-neb to-python examples/hello.neb` — translate to Python
- `/run-neb` (no args) — smoke-test all example files

## Behavior

1. If a specific file and command are given, run: `cargo run -- <command> <file>` from `compiler/`
2. If only a file is given, default command is `run`
3. If no arguments are given, run all examples as a smoke test:
   ```
   for f in examples/*.neb; do echo "=== $f ===" && cargo run -- "$f" && echo; done
   ```
4. If the command fails, read the error output and the relevant `.neb` source to diagnose the issue. Check the compiler source if needed to understand the error.
