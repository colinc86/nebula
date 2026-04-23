"""Quality-control gates for fine-tune training rows.

Each gate returns `(passed: bool, reason: str)`. Gates run in order; the
first failure short-circuits the row.
"""
from __future__ import annotations
import hashlib
import json
import re
from pathlib import Path
from typing import Optional

from config import (
    MIN_NEBULA_LEN, MAX_NEBULA_LEN, MAX_MESSAGE_LEN,
    MAX_RESPONSE_LEN, MAX_TOTAL_LEN,
)
import nebula_cli


TEMPLATE_LEAK_PATTERNS = [
    r"\bI can't\b",
    r"\bI cannot\b",
    r"\bI'm sorry, but\b",
    r"\bAs an AI (language model|assistant)\b",
    r"^Here's the code:",
    r"^Here is the code:",
]
_TEMPLATE_LEAK_RE = re.compile("|".join(TEMPLATE_LEAK_PATTERNS), re.IGNORECASE | re.MULTILINE)


def gate_schema(row: dict) -> tuple[bool, str]:
    """Validate the OpenAI chat-completion shape."""
    msgs = row.get("messages")
    if not isinstance(msgs, list) or len(msgs) < 2:
        return False, "messages missing or too short"
    valid_roles = {"system", "user", "assistant", "tool"}
    for i, m in enumerate(msgs):
        if not isinstance(m, dict):
            return False, f"messages[{i}] not an object"
        role = m.get("role")
        if role not in valid_roles:
            return False, f"messages[{i}].role '{role}' invalid"
        # assistant messages may have content or tool_calls (or both);
        # others must have content.
        if role != "assistant" and "content" not in m:
            return False, f"messages[{i}] ({role}) missing content"
    return True, "ok"


def gate_length(row: dict) -> tuple[bool, str]:
    """Bounded lengths on user + assistant content + total."""
    total = 0
    for m in row.get("messages", []):
        c = m.get("content") or ""
        if isinstance(c, list):
            c = "".join(part.get("text", "") for part in c if isinstance(part, dict))
        c = str(c)
        role = m.get("role")
        if role == "user" and len(c) > MAX_MESSAGE_LEN:
            return False, f"user message {len(c)} > {MAX_MESSAGE_LEN}"
        if role == "assistant" and len(c) > MAX_RESPONSE_LEN:
            return False, f"assistant message {len(c)} > {MAX_RESPONSE_LEN}"
        total += len(c)
    if total > MAX_TOTAL_LEN:
        return False, f"total chars {total} > {MAX_TOTAL_LEN}"
    return True, "ok"


def gate_template_leak(row: dict) -> tuple[bool, str]:
    """Reject assistant responses with refusal / over-helpful prefix patterns."""
    for m in row.get("messages", []):
        if m.get("role") != "assistant":
            continue
        c = m.get("content") or ""
        if isinstance(c, list):
            c = "".join(part.get("text", "") for part in c if isinstance(part, dict))
        if _TEMPLATE_LEAK_RE.search(str(c)):
            return False, "template-leak pattern matched"
    return True, "ok"


def extract_nebula_block(assistant_content: str) -> Optional[str]:
    """Pull out the first ```nebula ... ``` fenced block, if any."""
    m = re.search(r"```(?:nebula)?\s*\n(.*?)```", assistant_content, re.DOTALL)
    if m:
        return m.group(1).strip()
    return None


def gate_nebula_check(row: dict, slice_name: str) -> tuple[bool, str]:
    """For slices that emit Nebula (translation/instruction/toolcall), verify
    the assistant's Nebula block parses. Skipped for anchor slice."""
    if slice_name == "anchor":
        return True, "skipped (anchor slice)"

    for m in row.get("messages", []):
        if m.get("role") != "assistant":
            continue
        c = m.get("content") or ""
        if isinstance(c, list):
            c = "".join(part.get("text", "") for part in c if isinstance(part, dict))
        neb = extract_nebula_block(str(c))
        if neb is None:
            # Slice 1 (translation) and slice 2 (instruction) must have a
            # Nebula block; slice 3 may use tool_calls instead.
            if slice_name in ("translation", "instruction"):
                return False, "no Nebula block in assistant content"
            continue
        if len(neb) < MIN_NEBULA_LEN or len(neb) > MAX_NEBULA_LEN:
            return False, f"Nebula length {len(neb)} out of [{MIN_NEBULA_LEN},{MAX_NEBULA_LEN}]"
        if not nebula_cli.check(neb):
            return False, "nebula check failed"

        # For tool-call slices, also check every code arg in tool_calls.
    if slice_name == "toolcall":
        for m in row.get("messages", []):
            if m.get("role") != "assistant":
                continue
            for tc in m.get("tool_calls", []) or []:
                fn = (tc.get("function") or {})
                try:
                    args_json = json.loads(fn.get("arguments") or "{}")
                except Exception:
                    return False, f"tool_call arguments not JSON-parseable"
                for field in ("code", "content", "new_str"):
                    v = args_json.get(field)
                    if isinstance(v, str) and v.strip() and not nebula_cli.check(v):
                        return False, f"tool_call {fn.get('name')} {field} failed check"
    return True, "ok"


def row_id(row: dict) -> str:
    """Stable hash over the user message + slice label."""
    parts = []
    for m in row.get("messages", []):
        if m.get("role") == "user":
            c = m.get("content") or ""
            if isinstance(c, list):
                c = "".join(p.get("text", "") for p in c if isinstance(p, dict))
            parts.append(str(c))
            break
    parts.append(row.get("slice", ""))
    h = hashlib.sha1("\x00".join(parts).encode("utf-8")).hexdigest()
    return h[:16]


def run_all_gates(row: dict, slice_name: str, seen_ids: Optional[set] = None) -> tuple[bool, str]:
    """Run all gates. Returns (pass, reason-of-first-failure-or-"ok")."""
    for gate in (gate_schema, gate_length, gate_template_leak):
        ok, why = gate(row)
        if not ok:
            return False, why
    ok, why = gate_nebula_check(row, slice_name)
    if not ok:
        return False, why
    # Dedup (optional — slice builders pass a shared set).
    if seen_ids is not None:
        rid = row_id(row)
        if rid in seen_ids:
            return False, "duplicate row_id"
        seen_ids.add(rid)
    return True, "ok"


if __name__ == "__main__":
    # Smoke test.
    good = {
        "slice": "translation",
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "Convert this Python:"},
            {"role": "assistant", "content": "```nebula\ndef f(n):\n    return n+1\n```"},
        ],
    }
    print("good:", run_all_gates(good, "translation"))

    bad = {
        "slice": "translation",
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "Convert this Python:"},
            {"role": "assistant", "content": "I can't help with that."},
        ],
    }
    print("bad:", run_all_gates(bad, "translation"))
