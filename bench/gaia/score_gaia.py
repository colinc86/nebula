"""Score GAIA runs and append rows to summary.csv.

For each runs/gaia_<task_id>_<config>/ directory, parses result.json,
normalizes the predicted answer, compares to expected.txt, and writes a row
to summary.csv.
"""
import csv
import json
import re
import sys
from pathlib import Path


def normalize(s: str) -> str:
    s = s.strip().lower()
    s = re.sub(r"\s+", " ", s)
    s = re.sub(r"[.?!]+$", "", s)
    return s


def score_run(run_dir: Path):
    result_path = run_dir / "result.json"
    if not result_path.exists() or result_path.stat().st_size == 0:
        return None
    data = json.loads(result_path.read_text())
    expected = (run_dir / "expected.txt").read_text().strip() if (run_dir / "expected.txt").exists() else ""
    predicted = (data.get("result") or "").strip()
    correct = normalize(predicted) == normalize(expected)
    u = data.get("usage", {}) or {}
    return {
        "cost": data.get("total_cost_usd", 0),
        "inp": u.get("input_tokens", 0),
        "cache_read": u.get("cache_read_input_tokens", 0),
        "out": u.get("output_tokens", 0),
        "turns": data.get("num_turns", 0),
        "dur": data.get("duration_ms", 0),
        "predicted": predicted[:120],
        "expected": expected[:120],
        "correct": correct,
    }


def main():
    base = Path(__file__).parent
    runs_root = base / "runs"
    out_csv = base / "summary.csv"
    rows = []
    for run_dir in sorted(runs_root.glob("gaia_*_*")):
        name = run_dir.name
        parts = name.split("_")
        if len(parts) < 3:
            continue
        config = parts[-1]
        task_id = "_".join(parts[1:-1])
        scored = score_run(run_dir)
        if scored is None:
            continue
        rows.append([
            task_id,
            config,
            scored["cost"],
            scored["inp"],
            scored["cache_read"],
            scored["out"],
            scored["turns"],
            scored["dur"],
            scored["predicted"],
            scored["expected"],
            "true" if scored["correct"] else "false",
        ])
    with out_csv.open("w") as f:
        w = csv.writer(f)
        w.writerow(["task_id", "config", "total_cost_usd", "input_tokens", "cache_read_tokens", "output_tokens", "num_turns", "duration_ms", "predicted", "expected", "correct"])
        w.writerows(rows)
    print(f"wrote {len(rows)} rows to {out_csv}")

    # Aggregate per config
    agg = {}
    for r in rows:
        cfg = r[1]
        agg.setdefault(cfg, {"n": 0, "correct": 0, "cost": 0.0, "inp": 0, "out": 0})
        agg[cfg]["n"] += 1
        agg[cfg]["correct"] += 1 if r[10] == "true" else 0
        agg[cfg]["cost"] += float(r[2] or 0)
        agg[cfg]["inp"] += int(r[3] or 0)
        agg[cfg]["out"] += int(r[5] or 0)
    print("\nAggregate:")
    for cfg, a in agg.items():
        print(f"  {cfg}: n={a['n']} acc={a['correct']}/{a['n']} cost=${a['cost']:.3f} input={a['inp']} output={a['out']}")


if __name__ == "__main__":
    main()
