#!/usr/bin/env python3
"""Evaluate candidate fine-tunes via Together dedicated endpoints.

For each candidate:
  1. Find the model name in Together's registry
  2. Spin up a dedicated endpoint (2×H100)
  3. Wait for STARTED
  4. Run 8-shape × N=1 benchmark via the phase20_smoke harness
  5. Tear down the endpoint
  6. Report results

This avoids the 44 GB download + convert cycle for each candidate.
The winner gets downloaded and deployed locally for N=3 paper-grade numbers.
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

SHAPES = [
    ("add_feature",     "add_feature_template",     "add_feature_prompt.txt",     "go test ./..."),
    ("bugfix",          "bugfix_template",          "bugfix_prompt.txt",          "go test ./..."),
    ("extract_helper",  "extract_helper_template",  "extract_helper_prompt.txt",  "go test ./..."),
    ("port_language",   "port_language_template",   "port_language_prompt.txt",   "go test ./..."),
    ("write_from_spec", "write_from_spec_template", "write_from_spec_prompt.txt", "go test ./..."),
    ("refactor",        "refactor_template",        "refactor_prompt.txt",        "go test ./..."),
    ("docstring_pass",  "docstring_pass_template",  "docstring_pass_prompt.txt",  "bash verify_docstrings.sh"),
    ("large_file",      "large_file_template",      "large_file_prompt.txt",      "go test ./..."),
]


def load_api_key() -> str:
    if "TOGETHER_API_KEY" in os.environ:
        return os.environ["TOGETHER_API_KEY"]
    for line in ENV_FILE.read_text().splitlines():
        if line.startswith("TOGETHER_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"').strip("'")
    raise RuntimeError("TOGETHER_API_KEY not set")


def find_model_name(client, job_id: str) -> str:
    """Search Together's model list for a model created by this job."""
    import requests
    r = requests.get("https://api.together.xyz/v1/models?filter=fine-tunable",
                     headers={"Authorization": f"Bearer {client.api_key}"})
    # The job suffix maps to the model name suffix
    job = client.fine_tuning.retrieve(id=job_id)
    # Look for the model in the list
    suffix = getattr(job, "suffix", "") or ""
    for m in r.json():
        if isinstance(m, dict) and suffix in m.get("id", "") and job_id[:12] not in m.get("id", ""):
            pass  # not it
    # Simpler: just check model_extra for adapter_output_path
    model_extra = getattr(job, "model_extra", {}) or {}
    # The model name follows the pattern colinc86_4aa1/BaseModel-suffix-hash
    for m in r.json():
        if isinstance(m, dict):
            mid = m.get("id", "")
            if suffix and suffix in mid and mid.startswith("colinc86"):
                # Check it's from this job by matching the hash
                return mid
    raise RuntimeError(f"Could not find model for job {job_id} with suffix {suffix}")


def wait_for_jobs(client, job_ids: dict[str, str], timeout: int = 1800) -> dict[str, str]:
    """Wait for all fine-tune jobs to complete. Returns {name: status}."""
    pending = dict(job_ids)
    statuses = {}
    t0 = time.time()
    while pending and time.time() - t0 < timeout:
        for name, jid in list(pending.items()):
            job = client.fine_tuning.retrieve(id=jid)
            status = str(job.status).lower()
            if status in ("completed", "error", "cancelled", "failed"):
                statuses[name] = status
                del pending[name]
                print(f"  {name} ({jid}): {status}", flush=True)
        if pending:
            time.sleep(30)
    for name in pending:
        statuses[name] = "timeout"
    return statuses


def spin_up_endpoint(client, model_name: str, display_name: str) -> str:
    """Create a dedicated endpoint, wait for STARTED, return endpoint ID."""
    ep = client.endpoints.create(
        model=model_name,
        hardware="2x_nvidia_h100_80gb_sxm",
        autoscaling={"min_replicas": 1, "max_replicas": 1},
        display_name=display_name,
        inactive_timeout=15,
    )
    print(f"  endpoint {ep.id} state={ep.state}", flush=True)
    for _ in range(60):
        ep = client.endpoints.retrieve(ep.id)
        if str(ep.state) == "STARTED":
            # The endpoint model name has a suffix
            ep_model = getattr(ep, "name", model_name)
            return ep.id, ep_model
        time.sleep(10)
    raise RuntimeError(f"Endpoint {ep.id} never reached STARTED (state={ep.state})")


def run_benchmark(model_name: str, ollama_url: str = None) -> list[dict]:
    """Run 8 shapes × N=1 via the phase20_smoke harness. Returns list of result dicts."""
    sys.path.insert(0, str(REPO / "bench" / "mcp_manual_test"))
    # Set env for the smoke harness
    os.environ["NEBULA_DELEGATE_MODEL"] = model_name
    if ollama_url:
        os.environ["NEBULA_DELEGATE_OLLAMA_URL"] = ollama_url
    os.environ["NEBULA_DELEGATE_NUM_CTX"] = "32768"

    from phase20_smoke import run_shape, NEBULA

    results = []
    for shape, template, prompt, verify in SHAPES:
        print(f"    {shape}...", end=" ", flush=True)
        try:
            p = run_shape(shape, template, prompt, verify, trial="eval")
            vp = p.get("verify_passed")
            turns = p.get("turns_used")
            wall = p.get("_harness_wall_seconds")
            print(f"verify={vp} turns={turns} wall={wall}s")
            results.append({"shape": shape, "verify_passed": vp, "turns_used": turns, "wall_s": wall})
        except Exception as e:
            print(f"ERROR: {e}")
            results.append({"shape": shape, "verify_passed": None, "error": str(e)[:200]})
    return results


def main():
    api_key = load_api_key()
    sys.path.insert(0, str(REPO / ".venv" / "lib" / "python3.13" / "site-packages"))
    from together import Together
    client = Together(api_key=api_key)

    # Load job IDs
    jobs = json.loads((HERE / "candidate_jobs.json").read_text())["candidates"]
    print(f"Candidates: {json.dumps(jobs)}\n")

    # Wait for all 3 to complete
    print("Waiting for fine-tunes to complete...")
    statuses = wait_for_jobs(client, jobs)
    print(f"\nStatuses: {statuses}\n")

    failed = [n for n, s in statuses.items() if s != "completed"]
    if failed:
        print(f"WARNING: {failed} did not complete. Evaluating only completed candidates.")

    # Find model names
    import requests
    r = requests.get("https://api.together.xyz/v1/models?filter=fine-tunable",
                     headers={"Authorization": f"Bearer {api_key}"})
    all_models = r.json()

    # For each completed candidate: find model, spin up endpoint, eval, tear down
    all_results = {}
    for name in sorted(jobs.keys()):
        if statuses.get(name) != "completed":
            continue
        jid = jobs[name]
        # Find the model by matching the suffix
        job_obj = client.fine_tuning.retrieve(id=jid)
        suffix = f"neb{name}"
        candidates = [m.get("id") for m in all_models if isinstance(m, dict) and suffix in m.get("id", "")]
        if not candidates:
            print(f"\n{name}: model not found in registry (suffix={suffix})")
            continue
        model_name = candidates[-1]  # latest
        print(f"\n=== {name}: {model_name} ===")

        try:
            ep_id, ep_model = spin_up_endpoint(client, model_name, f"eval-{name}")
            print(f"  endpoint ready: {ep_id}, model={ep_model}")
        except Exception as e:
            print(f"  endpoint spin-up failed: {e}")
            continue

        try:
            results = run_benchmark(ep_model, ollama_url=None)
            all_results[name] = results
        finally:
            print(f"  tearing down endpoint {ep_id}...")
            try:
                client.endpoints.delete(ep_id)
            except Exception as e:
                print(f"  delete failed: {e}")

    # Summary
    print("\n" + "=" * 70)
    print("CANDIDATE COMPARISON (8 shapes × N=1)")
    print("=" * 70)
    print(f"{'candidate':10s}  {'pass':>6s}  {'mean_wall':>10s}  {'shapes_passed':>30s}")
    for name, results in sorted(all_results.items()):
        passed = [r for r in results if r.get("verify_passed")]
        walls = [r["wall_s"] for r in passed if r.get("wall_s")]
        mean_wall = f"{sum(walls)/len(walls):.1f}s" if walls else "n/a"
        shapes = ", ".join(r["shape"] for r in passed)
        print(f"{name:10s}  {len(passed):>4d}/8  {mean_wall:>10s}  {shapes}")

    # Save detailed results
    out = HERE / "candidate_eval_results.json"
    out.write_text(json.dumps(all_results, indent=2))
    print(f"\nDetailed results → {out}")


if __name__ == "__main__":
    main()
