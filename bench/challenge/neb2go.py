#!/usr/bin/env python3
"""Convert .neb compact notation back to Go source.

This is a simple text-level keyword expansion — the reverse of the ingester's
postprocessing. It does NOT parse Nebula; it just expands shortened keywords
back to their Go equivalents.

Usage:
    python3 neb2go.py main.neb > main.go
    python3 neb2go.py main.neb main_test.neb   # converts both, writes .go
"""

import re
import sys


def neb_to_go(source: str, package: str = "main") -> str:
    lines = source.split("\n")
    out = []
    imports = set()
    in_enum = False
    enum_type = ""
    enum_counter = 0

    for line in lines:
        stripped = line.strip()

        # Skip comment-only lines that are just "// package ..." or "// import ..."
        if stripped.startswith("// package ") or stripped.startswith("// import "):
            continue

        # Enum blocks: en TypeName { ... }
        if stripped.startswith("en "):
            m = re.match(r"en\s+(\w+)\s*\{", stripped)
            if m:
                enum_type = m.group(1)
                enum_counter = 0
                in_enum = True
                out.append("")  # blank line before const block
                out.append("const (")
                continue

        if in_enum:
            if stripped == "}":
                in_enum = False
                out.append(")")
                continue
            elif stripped:
                indent = "\t"
                if enum_counter == 0:
                    out.append(f"{indent}{stripped} {enum_type} = iota")
                else:
                    out.append(f"{indent}{stripped}")
                enum_counter += 1
                continue

        # Type alias: ty Name = type
        if stripped.startswith("ty "):
            m = re.match(r"ty\s+(\w+)\s*=\s*(.*)", stripped)
            if m:
                out.append(f"type {m.group(1)} {m.group(2)}")
                continue

        # Struct: st Name {
        if stripped.startswith("st "):
            m = re.match(r"st\s+(\w+)\s*\{", stripped)
            if m:
                out.append(f"type {m.group(1)} struct {{")
                continue

        # Trait: tr Name {
        if stripped.startswith("tr "):
            m = re.match(r"tr\s+(\w+)\s*\{", stripped)
            if m:
                out.append(f"type {m.group(1)} interface {{")
                continue

        # Implementation: im Name { (methods follow)
        if stripped.startswith("im "):
            m = re.match(r"im\s+(\w+)\s*\{", stripped)
            if m:
                # Just skip the im line, methods will follow with fn which becomes func
                continue

        # Match: mc expr {
        line = re.sub(r"(\s*)mc\s+(.+?)\s*\{", r"\1switch \2 {", line)

        # Match arms: value => body  →  case value: body
        m = re.match(r"(\s+)(\S.*?)\s+=>\s+(.*)", line)
        if m and "=>" in line and not line.strip().startswith("//"):
            indent, pattern, body = m.group(1), m.group(2), m.group(3)
            if pattern == "_":
                if body.strip() == "{":
                    out.append(f"{indent}default:")
                else:
                    out.append(f"{indent}default:")
                    out.append(f"{indent}\t{expand_keywords(body.strip())}")
                continue
            else:
                if body.strip() == "{":
                    out.append(f"{indent}case {pattern}:")
                else:
                    out.append(f"{indent}case {pattern}:")
                    out.append(f"{indent}\t{expand_keywords(body.strip())}")
                continue

        # Function/method expansion
        line = expand_keywords(line)
        out.append(line)

    # Prepend package and imports
    header = [f"package {package}", ""]

    # Detect needed imports from the output
    full = "\n".join(out)
    import_map = {
        "fmt.": '"fmt"',
        "math.": '"math"',
        "sort.": '"sort"',
        "strings.": '"strings"',
        "sync.": '"sync"',
        "time.": '"time"',
        "json.": '"encoding/json"',
        "encoding/json": '"encoding/json"',
    }
    for pattern, imp in import_map.items():
        if pattern in full:
            imports.add(imp)

    if imports:
        header.append("import (")
        for imp in sorted(imports):
            header.append(f"\t{imp}")
        header.append(")")
        header.append("")

    return "\n".join(header + out)


def expand_keywords(line: str) -> str:
    """Expand Nebula keywords back to Go equivalents."""
    # Function declarations: fn Name(sf, ...) -> RetType {
    # Method: fn Name(sf, params) → func (s *Type) Name(params)
    # We handle the simple keyword replacements here

    # fn → func
    line = re.sub(r"\bfn\b", "func", line)
    # -> RetType → RetType (remove the arrow)
    line = re.sub(r"\s*->\s*", " ", line)
    # Lambda/closure: |params| → func(params)
    line = re.sub(r"\|([^|]+)\|\s*\{", r"func(\1) {", line)
    # Tuple destructuring: mt (a, b) = func() → a, b := func()
    line = re.sub(r"\bmt\s+\(([^)]+)\)\s*=", r"\1 :=", line)
    # Ptr<Type> → *Type
    line = re.sub(r"Ptr<(\w+)>", r"*\1", line)
    # List<Type> → []Type
    line = re.sub(r"List<(\w+)>", r"[]\1", line)
    # Map<K,V> → map[K]V
    line = re.sub(r"Map<(\w+),\s*(\w+)>", r"map[\1]\2", line)
    # Type: FieldType → FieldType (parameter syntax - just remove the colon and type becomes the value)
    # Actually, this is tricky. For now, let's handle common cases
    # t: Ptr<TestingT> → t *TestingT (parameter syntax)
    line = re.sub(r"(\w+):\s*\*(\w+)", r"\1 *\2", line)
    # t: Type → t Type (parameter syntax, but NOT in struct literals)
    # Only convert if not inside { }
    if "{" not in line:
        line = re.sub(r"(\w+):\s+(\w+)\b", r"\1 \2", line)
    # Array/slice literals: handle string and int arrays
    # But don't convert array indexing like arr[0] or arr[i]
    # String arrays: ["string"] → []string{"string"}
    # Only convert if preceded by space or operator, not by identifier
    line = re.sub(r'([\s=\(,])\[\s*(".*?")\s*\]', r'\1[]string{\2}', line)
    line = re.sub(r'([\s=\(,])\[\s*(".*?",\s*(?:".*?,\s*)*".*?)\s*\]', r'\1[]string{\2}', line)
    # Integer/numeric arrays: [1, 2, 3] → []int{1, 2, 3} (if not already converted)
    # Only convert if preceded by space or operator, not by identifier (array indexing)
    line = re.sub(r'([\s=\(,])\[\s*(\d[\d,\s]*)\s*\](?!\{)', r'\1[]int{\2}', line)
    # rt → return
    line = re.sub(r"\brt\b", "return", line)
    # lt → (used for const/var - context dependent, but in Go it's usually var or :=)
    # mt → var/mutable
    line = re.sub(r"\bmt\s+(\w+)\s*=", r"\1 :=", line)
    line = re.sub(r"\blt\s+(\w+)\s*=", r"\1 :=", line)
    # Booleans
    line = re.sub(r"\btt\b", "true", line)
    line = re.sub(r"\bff\b", "false", line)
    # Logical operators
    line = re.sub(r"\band\b", "&&", line)
    line = re.sub(r"\bor\b", "||", line)
    line = re.sub(r"\bnot\s+", "!", line)
    # Control flow
    line = re.sub(r"\bel\b", "else", line)
    line = re.sub(r"\bbk\b", "break", line)
    line = re.sub(r"\bct\b", "continue", line)
    # Loops
    line = re.sub(r"\bfo\b", "for", line)
    line = re.sub(r"\bwl\b", "for", line)
    line = re.sub(r"\blp\b", "for", line)
    # for x in y → for _, x := range y  (simple case)
    line = re.sub(r"for\s+(\w+)\s+in\s+", r"for _, \1 := range ", line)
    line = re.sub(r"for\s+(\w+),\s*(\w+)\s+in\s+", r"for \1, \2 := range ", line)
    # len shorthand
    line = re.sub(r"\bln\(", "len(", line)
    # prt → fmt.Println
    line = re.sub(r"\bprt\(", "fmt.Println(", line)
    # sp → go
    line = re.sub(r"\bsp\s+", "go ", line)
    # sf → s (receiver) — this is approximate
    line = re.sub(r"\bsf\.", "s.", line)

    return line


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 neb2go.py <file.neb> [file2.neb ...]", file=sys.stderr)
        sys.exit(1)

    for path in sys.argv[1:]:
        with open(path) as f:
            source = f.read()

        go_source = neb_to_go(source)

        if len(sys.argv) == 2:
            # Single file: print to stdout
            print(go_source)
        else:
            # Multiple files: write .go alongside .neb
            out_path = path.replace(".neb", ".go")
            with open(out_path, "w") as f:
                f.write(go_source)
            print(f"  {path} → {out_path}")
