#!/usr/bin/env python3
"""Poll a Together fine-tune job and exit when it terminates.

Writes status updates to monitor.log every poll. Exits 0 on success,
non-zero on failure. Designed to run in the background while we work on
parallel tasks.
"""

from __future__ import annotations
import json
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
ENV_FILE = REPO / ".env"


def load_key() -> str:
    if "TOGETHER_API_KEY" in os.environ:
        return os.environ["TOGETHER_API_KEY"]
    for line in ENV_FILE.read_text().splitlines():
        if line.startswith("TOGETHER_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    raise RuntimeError("TOGETHER_API_KEY not set")


def main() -> None:
    job_id = sys.argv[1] if len(sys.argv) > 1 else None
    if not job_id:
        info = json.loads((HERE / "finetune_job.json").read_text())
        job_id = info["job_id"]
    print(f"Monitoring {job_id}", flush=True)

    sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
    from together import Together  # type: ignore

    client = Together(api_key=load_key())
    log_path = HERE / "monitor.log"
    log = log_path.open("a")

    terminal = {"completed", "error", "cancelled", "failed"}
    last_status = ""
    poll = 0
    started = time.time()
    while True:
        poll += 1
        try:
            r = client.fine_tuning.retrieve(id=job_id)
        except Exception as e:
            log.write(f"{time.strftime('%H:%M:%S')} poll{poll} ERROR: {e}\n"); log.flush()
            time.sleep(30)
            continue
        status = str(getattr(r, "status", "unknown"))
        elapsed = int(time.time() - started)
        if status != last_status:
            line = f"{time.strftime('%H:%M:%S')} poll{poll:3d} elapsed={elapsed}s status={status}"
            output = getattr(r, "output_name", None)
            if output:
                line += f" output={output}"
            log.write(line + "\n"); log.flush()
            print(line, flush=True)
            last_status = status
        if status.lower() in terminal:
            output = getattr(r, "output_name", None)
            log.write(f"{time.strftime('%H:%M:%S')} TERMINAL  status={status}  output={output}\n"); log.flush()
            print(f"\nDONE  status={status}  output={output}", flush=True)
            (HERE / "finetune_final.json").write_text(json.dumps({
                "status": status,
                "output_name": output,
                "elapsed_seconds": elapsed,
                "job_id": job_id,
            }, indent=2))
            sys.exit(0 if status.lower() == "completed" else 1)
        time.sleep(30)


if __name__ == "__main__":
    main()
