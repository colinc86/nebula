"""Stage a single GAIA task into a run directory.

Writes question.txt (the task prompt) and copies any attached files into the
run dir. Reads the task record from the given JSONL dataset.
"""
import json
import shutil
import sys
from pathlib import Path


def main():
    if len(sys.argv) != 4:
        print("usage: stage_task.py <task_id> <run_dir> <dataset_jsonl>", file=sys.stderr)
        sys.exit(2)
    task_id, run_dir, dataset = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
    run_dir.mkdir(parents=True, exist_ok=True)

    record = None
    with dataset.open() as f:
        for line in f:
            r = json.loads(line)
            if r.get("task_id") == task_id:
                record = r
                break
    if record is None:
        print(f"task {task_id} not found in {dataset}", file=sys.stderr)
        sys.exit(1)

    question = record["Question"]
    (run_dir / "question.txt").write_text(question + "\n\nRespond with ONLY the final answer as a single line — no explanation.\n")
    (run_dir / "expected.txt").write_text(str(record.get("Final answer", "")) + "\n")

    # Copy any attached files — hf download stores them under files/2023/validation/<file>
    file_name = record.get("file_name")
    if file_name:
        candidates = [
            dataset.parent / "files" / "2023" / "validation" / file_name,
            dataset.parent / "files" / "2023" / "test" / file_name,
            dataset.parent / "files" / file_name,
        ]
        found = next((p for p in candidates if p.exists()), None)
        if found is not None:
            shutil.copy2(found, run_dir / file_name)
        else:
            print(f"warning: attached file {file_name} not found in any of {candidates}", file=sys.stderr)

    print(f"staged {task_id} -> {run_dir}")


if __name__ == "__main__":
    main()
