#!/usr/bin/env python3
"""Download the merged checkpoint of the Phase 20 sidecar fine-tune.

Usage:
    .venv/bin/python3 download_finetune.py            # merged
    .venv/bin/python3 download_finetune.py adapter    # LoRA adapter only
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
OUT_DIR = HERE / "downloads"
OUT_DIR.mkdir(parents=True, exist_ok=True)

JOB_ID = os.environ.get("NEB_JOB_ID", "ft-897ce224-c6a5")  # default: combined fine-tune


def main() -> None:
    checkpoint = sys.argv[1] if len(sys.argv) > 1 else "merged"
    assert checkpoint in ("merged", "adapter", "model_output_path")

    api_key = os.environ.get("TOGETHER_API_KEY")
    if not api_key:
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("TOGETHER_API_KEY="):
                api_key = line.split("=", 1)[1].strip().strip('"').strip("'")
                break
    assert api_key, "TOGETHER_API_KEY not set"

    sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
    from together import Together  # type: ignore

    client = Together(api_key=api_key)
    print(f"Requesting {checkpoint} checkpoint for {JOB_ID}", flush=True)
    t0 = time.time()
    out_path = OUT_DIR / f"{JOB_ID}_{checkpoint}.tar.zst"
    # Use the streaming response context manager — the non-streaming variant
    # buffers the entire body into memory before iter_bytes() yields anything,
    # which OOMs for a 14GB tarball.
    with client.fine_tuning.with_streaming_response.content(
        ft_id=JOB_ID, checkpoint=checkpoint
    ) as resp:
        written = 0
        last_log = 0
        with out_path.open("wb") as fh:
            for chunk in resp.iter_bytes(chunk_size=8 * 1024 * 1024):
                fh.write(chunk)
                written += len(chunk)
                if written - last_log >= 64 * 1024 * 1024:
                    mb = written / 1024 / 1024
                    rate = mb / max(0.1, (time.time() - t0))
                    print(f"  {mb:7.1f} MB  ({rate:5.1f} MB/s)", flush=True)
                    last_log = written
    elapsed = time.time() - t0
    print(f"Done. {written/1024/1024:.1f} MB in {elapsed:.1f}s ({written/1024/1024/elapsed:.1f} MB/s)")
    print(f"Output: {out_path}")


if __name__ == "__main__":
    main()
