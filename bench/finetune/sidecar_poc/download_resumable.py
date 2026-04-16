#!/usr/bin/env python3
"""Resumable downloader for Together fine-tune checkpoints.

Together's /v1/finetune/download endpoint returns a 302 redirect to a
pre-signed Cloudflare R2 URL. R2 supports HTTP Range requests natively,
so we can resume partial downloads — which the vanilla together SDK
streaming path cannot do (any network blip restarts the 47 GB transfer
from zero).

Strategy:
  1. GET the /v1/finetune/download endpoint with Authorization, capture
     the 302 Location header (the R2 pre-signed URL).
  2. Download with httpx, using Range: bytes=<current_size>- to resume
     from whatever the previous attempt wrote. Retry on partial-body
     errors up to N times, re-fetching a fresh R2 URL each retry since
     pre-signed URLs have a short TTL (~1 hour).
"""

from __future__ import annotations
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).parent.resolve()
REPO = HERE.parent.parent.parent
ENV_FILE = REPO / ".env"
OUT_DIR = HERE / "downloads"
OUT_DIR.mkdir(parents=True, exist_ok=True)

MAX_RETRIES = 10


def load_api_key() -> str:
    if "TOGETHER_API_KEY" in os.environ:
        return os.environ["TOGETHER_API_KEY"]
    for line in ENV_FILE.read_text().splitlines():
        if line.startswith("TOGETHER_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    raise RuntimeError("TOGETHER_API_KEY not set")


def fetch_r2_url(api_key: str, ft_id: str, checkpoint: str) -> str:
    import httpx
    with httpx.Client(follow_redirects=False, timeout=30.0) as c:
        r = c.get(
            "https://api.together.xyz/v1/finetune/download",
            params={"ft_id": ft_id, "checkpoint": checkpoint},
            headers={"Authorization": f"Bearer {api_key}",
                     "Accept": "application/octet-stream"},
        )
        if r.status_code != 302:
            raise RuntimeError(f"expected 302 from /finetune/download, got {r.status_code}: {r.text[:300]}")
        loc = r.headers.get("location")
        if not loc:
            raise RuntimeError("302 response has no Location header")
        return loc


def download_with_resume(r2_url: str, out_path: Path, expected_size: int | None = None) -> None:
    import httpx
    t0 = time.time()
    start_size = out_path.stat().st_size if out_path.exists() else 0
    mode = "ab" if start_size > 0 else "wb"
    print(f"  resuming from {start_size / 1024 / 1024:.1f} MB", flush=True)

    headers: dict[str, str] = {}
    if start_size > 0:
        headers["Range"] = f"bytes={start_size}-"

    # httpx Client with a long read timeout — R2 is usually fast, but the
    # streaming body can pause between chunks.
    timeout = httpx.Timeout(connect=30.0, read=120.0, write=30.0, pool=30.0)
    with httpx.Client(timeout=timeout, follow_redirects=True) as c:
        with c.stream("GET", r2_url, headers=headers) as r:
            if r.status_code not in (200, 206):
                raise RuntimeError(f"R2 returned status {r.status_code}: {r.text[:300]}")
            # Expected total size from Content-Length (or Content-Range if partial)
            if r.status_code == 206:
                cr = r.headers.get("content-range", "")
                # bytes 1234-5678/90000
                if "/" in cr:
                    try:
                        expected_size = int(cr.rsplit("/", 1)[1])
                    except Exception:
                        pass
            elif r.status_code == 200:
                try:
                    expected_size = int(r.headers.get("content-length", "0")) + start_size
                except Exception:
                    pass
            if expected_size:
                print(f"  target size: {expected_size / 1024 / 1024:.0f} MB", flush=True)

            written_this_attempt = 0
            last_log = 0
            with out_path.open(mode) as fh:
                for chunk in r.iter_bytes(chunk_size=8 * 1024 * 1024):
                    fh.write(chunk)
                    written_this_attempt += len(chunk)
                    total = start_size + written_this_attempt
                    if total - last_log >= 256 * 1024 * 1024:
                        mb = total / 1024 / 1024
                        rate = written_this_attempt / 1024 / 1024 / max(1, time.time() - t0)
                        pct = (100 * total / expected_size) if expected_size else 0
                        print(f"  {mb:7.0f} MB  {pct:5.1f}%  ({rate:5.1f} MB/s)", flush=True)
                        last_log = total

    final_size = out_path.stat().st_size
    print(f"  attempt complete: {final_size / 1024 / 1024:.0f} MB on disk")
    if expected_size and final_size < expected_size:
        raise RuntimeError(f"incomplete: {final_size} < {expected_size}")


def main() -> None:
    ft_id = os.environ.get("NEB_JOB_ID", "")
    checkpoint = sys.argv[1] if len(sys.argv) > 1 else "merged"
    if not ft_id:
        print("Set NEB_JOB_ID env var", file=sys.stderr)
        sys.exit(1)

    api_key = load_api_key()
    out_path = OUT_DIR / f"{ft_id}_{checkpoint}.tar.zst"
    expected = None

    for attempt in range(1, MAX_RETRIES + 1):
        print(f"\n[attempt {attempt}/{MAX_RETRIES}] fetching fresh R2 URL...", flush=True)
        r2_url = fetch_r2_url(api_key, ft_id, checkpoint)
        try:
            download_with_resume(r2_url, out_path, expected)
            print(f"\nDone. {out_path}")
            return
        except Exception as e:
            print(f"  attempt {attempt} failed: {type(e).__name__}: {e}", flush=True)
            if attempt < MAX_RETRIES:
                backoff = min(30, 2 ** attempt)
                print(f"  sleeping {backoff}s before retry", flush=True)
                time.sleep(backoff)

    print("All retries exhausted", file=sys.stderr)
    sys.exit(1)


if __name__ == "__main__":
    main()
