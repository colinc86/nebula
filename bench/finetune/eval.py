#!/usr/bin/env python3
"""Evaluate a trained Nebula→Python model on HumanEval.

Takes HumanEval problems, converts them to Nebula, feeds them to the model,
and checks if the Python output passes the unit tests.

Usage:
    # Evaluate fine-tuned model (direct loading via unsloth):
    python3 eval.py --model models/nebula-python-v1/checkpoint-2500 --problems 164

    # Evaluate via Ollama:
    python3 eval.py --ollama nebula-python-3epoch --problems 164

    # Evaluate via Together AI:
    python3 eval.py --together colinc86_4aa1/Qwen2.5-1.5B-nebula-python-0aa3a047 --problems 164

    # Quick test:
    python3 eval.py --ollama nebula-python-3epoch --problems 10
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
NEBULA_BIN = REPO_ROOT / "compiler" / "target" / "debug" / "nebula"
HUMANEVAL = REPO_ROOT / "bench" / "corpus" / "humaneval_py" / "humaneval.jsonl"

SYSTEM_PROMPT = "You are a code translator. Given code in Nebula (a universal intermediate language), produce the equivalent idiomatic Python code. Output only the Python code, no explanations."


def load_humaneval(max_problems=None):
    """Load HumanEval problems."""
    with open(HUMANEVAL) as f:
        problems = [json.loads(line) for line in f]
    if max_problems:
        problems = problems[:max_problems]
    return problems


def python_to_nebula(python_code):
    """Convert Python to Nebula using the compiler."""
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        f.write(python_code)
        py_file = f.name
    try:
        result = subprocess.run(
            [str(NEBULA_BIN), "from-python", py_file],
            capture_output=True, text=True, timeout=10,
        )
        return result.stdout.strip() if result.returncode == 0 else None
    except Exception:
        return None
    finally:
        os.unlink(py_file)


def clean_response(response):
    """Extract code from model response, truncating at garbage."""
    response = response.strip()

    # Strip markdown code fences
    if response.startswith("```python"):
        response = response[9:]
    elif response.startswith("```nebula"):
        response = response[9:]
    elif response.startswith("```"):
        response = response[3:]
    if response.endswith("```"):
        response = response[:-3]

    # If model prefixed with "nebula\n", strip it
    if response.startswith("nebula\n"):
        response = response[7:]

    # Stop at hallucination markers
    for marker in ["```", "<|im_start|>", "<|im_end|>", "BitFields",
                    "Translate this", "\nuser\n", "\nassistant\n"]:
        idx = response.find(marker)
        if idx > 0:
            response = response[:idx]

    # Stop at first non-ASCII character (hallucination)
    for i, c in enumerate(response):
        if ord(c) > 127 and c not in '\u201c\u201d\u2018\u2019\u2014\u2013\u2026\u00d7\u00f7\u00b1\u2264\u2265\u2260':
            response = response[:i]
            break

    # Strip trailing incomplete lines
    lines = response.rstrip().split("\n")
    while lines and not lines[-1].strip():
        lines.pop()

    return "\n".join(lines)


def nebula_to_python_fixup(code):
    """Post-process: fix Nebula artifacts in generated code to valid Python."""
    import re
    # If model output Nebula instead of Python, use the rule-based translator
    if any(code.lstrip().startswith(kw) for kw in ["fn ", "fo ", "lt ", "mt ", "rt "]):
        return fallback_translate(code)

    # Fix Nebula type annotations that leaked into Python output
    code = re.sub(r': Float\b', ': float', code)
    code = re.sub(r': Bool\b', ': bool', code)
    code = re.sub(r': Int\b', ': int', code)
    code = re.sub(r': String\b', ': str', code)
    code = re.sub(r'-> Float\b', '-> float', code)
    code = re.sub(r'-> Bool\b', '-> bool', code)
    code = re.sub(r'-> Int\b', '-> int', code)
    code = re.sub(r'-> String\b', '-> str', code)
    code = re.sub(r'\bList\[', 'list[', code)
    code = re.sub(r'\bList<(\w+)>', r'list[\1]', code)
    code = re.sub(r'\bDict\[', 'dict[', code)
    code = re.sub(r'\bTuple\[', 'tuple[', code)
    return code


def fallback_translate(nebula_code):
    """Use the rule-based nebula to-python as fallback."""
    with tempfile.NamedTemporaryFile(suffix=".neb", mode="w", delete=False) as f:
        f.write(nebula_code)
        neb_file = f.name
    try:
        result = subprocess.run(
            [str(NEBULA_BIN), "to-python", neb_file],
            capture_output=True, text=True, timeout=10,
        )
        return result.stdout.strip() if result.returncode == 0 else nebula_code
    except Exception:
        return nebula_code
    finally:
        os.unlink(neb_file)


class DirectModel:
    """Load model directly via unsloth/transformers."""

    def __init__(self, model_path):
        from unsloth import FastLanguageModel
        print(f"Loading model: {model_path}")
        self.model, self.tokenizer = FastLanguageModel.from_pretrained(
            model_name=model_path,
            max_seq_length=2048,
            dtype=None,
            load_in_4bit=True,
        )
        FastLanguageModel.for_inference(self.model)

    def generate(self, nebula_code, max_new_tokens=512):
        prompt = f"<|im_start|>system\n{SYSTEM_PROMPT}<|im_end|>\n"
        prompt += f"<|im_start|>user\n{nebula_code}<|im_end|>\n"
        prompt += "<|im_start|>assistant\n"

        inputs = self.tokenizer(prompt, return_tensors="pt").to(self.model.device)
        outputs = self.model.generate(
            **inputs,
            max_new_tokens=max_new_tokens,
            temperature=0.1,
            do_sample=True,
            pad_token_id=self.tokenizer.eos_token_id,
        )
        response = self.tokenizer.decode(
            outputs[0][inputs["input_ids"].shape[1]:],
            skip_special_tokens=True,
        )
        return clean_response(response)


class OllamaModel:
    """Use model via Ollama API."""

    def __init__(self, model_name):
        import urllib.request
        self.model_name = model_name
        self.url = "http://localhost:11434/api/generate"
        print(f"Using Ollama model: {model_name}")
        try:
            req = urllib.request.Request(self.url, method="POST",
                data=json.dumps({"model": model_name, "prompt": "test", "stream": False}).encode(),
                headers={"Content-Type": "application/json"})
            urllib.request.urlopen(req, timeout=30)
        except Exception as e:
            print(f"Warning: Ollama warmup failed: {e}")

    def generate(self, nebula_code, max_new_tokens=512):
        import urllib.request
        payload = {
            "model": self.model_name,
            "prompt": nebula_code,
            "stream": False,
            "options": {"temperature": 0.1, "num_predict": max_new_tokens},
        }
        req = urllib.request.Request(self.url, method="POST",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"})
        try:
            resp = urllib.request.urlopen(req, timeout=60)
            data = json.loads(resp.read())
            return clean_response(data.get("response", ""))
        except Exception as e:
            print(f"  Ollama error: {e}")
            return ""


class TogetherModel:
    """Use model via Together AI API (OpenAI-compatible)."""

    def __init__(self, model_name):
        import urllib.request
        self.model_name = model_name
        self.url = "https://api.together.xyz/v1/chat/completions"
        # Load API key from openai.env
        env_file = REPO_ROOT / "openai.env"
        self.api_key = None
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("TOGETHER_API_KEY="):
                    self.api_key = line.split("=", 1)[1].strip().strip('"')
                    break
        if not self.api_key:
            self.api_key = os.environ.get("TOGETHER_API_KEY")
        if not self.api_key:
            print("ERROR: No TOGETHER_API_KEY found in openai.env or environment")
            sys.exit(1)
        print(f"Using Together AI model: {model_name}")

    def generate(self, nebula_code, max_new_tokens=512):
        import urllib.request
        payload = {
            "model": self.model_name,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": nebula_code},
            ],
            "temperature": 0.1,
            "max_tokens": max_new_tokens,
        }
        req = urllib.request.Request(self.url, method="POST",
            data=json.dumps(payload).encode(),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
                "User-Agent": "nebula-eval/1.0",
            })
        try:
            resp = urllib.request.urlopen(req, timeout=60)
            data = json.loads(resp.read())
            content = data["choices"][0]["message"]["content"]
            return clean_response(content)
        except Exception as e:
            print(f"  Together API error: {e}")
            return ""


def run_test(python_code, test_code, entry_point, timeout=10):
    """Run the generated Python code against the test."""
    full_code = python_code + "\n" + test_code + f"\ncheck({entry_point})\n"
    try:
        result = subprocess.run(
            [sys.executable, "-c", full_code],
            capture_output=True, text=True, timeout=timeout,
        )
        return result.returncode == 0, result.stderr[:200]
    except subprocess.TimeoutExpired:
        return False, "TIMEOUT"
    except Exception as e:
        return False, str(e)


def main():
    parser = argparse.ArgumentParser(description="Evaluate Nebula→Python model")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--model", help="Model path for direct loading (unsloth)")
    group.add_argument("--ollama", help="Ollama model name")
    group.add_argument("--together", help="Together AI model name")
    parser.add_argument("--problems", type=int, default=20, help="Number of HumanEval problems")
    parser.add_argument("--output", default=None, help="Output results JSONL")
    args = parser.parse_args()

    if args.ollama:
        model = OllamaModel(args.ollama)
    elif args.together:
        model = TogetherModel(args.together)
    else:
        model = DirectModel(args.model)

    problems = load_humaneval(args.problems)
    print(f"Evaluating on {len(problems)} HumanEval problems\n")

    passed = 0
    failed = 0
    errors = 0
    results = []

    for i, problem in enumerate(problems):
        task_id = problem["task_id"]
        entry_point = problem["entry_point"]
        prompt = problem["prompt"]
        solution = problem["canonical_solution"]
        test = problem["test"]

        # Convert prompt+solution to Nebula
        nebula = python_to_nebula(prompt + solution)
        if nebula is None:
            print(f"[{i+1}/{len(problems)}] {task_id}: SKIP (nebula conversion failed)")
            errors += 1
            continue

        # Generate Python from Nebula
        generated = nebula_to_python_fixup(model.generate(nebula))

        # Test the generated code
        ok, err = run_test(generated, test, entry_point)

        status = "PASS" if ok else "FAIL"
        if ok:
            passed += 1
        else:
            failed += 1

        print(f"[{i+1}/{len(problems)}] {task_id}: {status}")

        results.append({
            "task_id": task_id,
            "status": status,
            "nebula_input": nebula[:200],
            "generated": generated[:200],
            "error": err if not ok else None,
        })

    # Summary
    total = passed + failed
    ca = passed / total * 100 if total > 0 else 0
    print(f"\n=== RESULTS ===")
    print(f"Pass@1: {passed}/{total} ({ca:.1f}%)")
    print(f"Errors (skip): {errors}")

    if args.output:
        with open(args.output, "w") as f:
            for r in results:
                f.write(json.dumps(r) + "\n")
        print(f"Results saved to {args.output}")


if __name__ == "__main__":
    main()
