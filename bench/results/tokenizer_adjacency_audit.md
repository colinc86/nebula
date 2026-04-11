# cl100k_base Adjacency Audit for Nebula 0.3

Measures how `tiktoken.get_encoding('cl100k_base')` tokenizes every
syntax-relevant pattern we might use in a Nebula redesign. Run via
`bench/benchmarks/tokenizer_adjacency_audit.py`.

Numbers in **bold** are token counts. The `Pieces` column shows how the
tokenizer split the text — watch for single-token wins and for
adjacency merges that could inform grammar choices.
## 1. Single chars and operators (no surrounding context)

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| open brace | `{` | **1** | `{` |
| close brace | `}` | **1** | `}` |
| open paren | `(` | **1** | `(` |
| close paren | `)` | **1** | `)` |
| open bracket | `[` | **1** | `[` |
| close bracket | `]` | **1** | `]` |
| colon | `:` | **1** | `:` |
| semicolon | `;` | **1** | `;` |
| comma | `,` | **1** | `,` |
| pipe | `\|` | **1** | `\|` |
| arrow | `->` | **1** | `->` |
| fat arrow | `=>` | **1** | `=>` |
| colon colon | `::` | **1** | `::` |
| pipe arrow | `\|>` | **2** | `\| | >` |
| nil coalesce | `??` | **1** | `??` |
| dot dot | `..` | **1** | `..` |
| dot dot eq | `..=` | **2** | `.. | =` |
| eq | `=` | **1** | `=` |
| eq eq | `==` | **1** | `==` |
| walrus | `:=` | **1** | `:=` |
| plus eq | `+=` | **1** | `+=` |
| minus eq | `-=` | **1** | `-=` |

## 2. Brace adjacency — packed vs spaced

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| paren-brace packed | `){` | **1** | `){` |
| paren-brace spaced | `) {` | **2** | `) |  {` |
| close brace newline | `}\n` | **1** | `}\n` |
| close brace space | `} ` | **2** | `} |  ` |
| newline close brace | `\n}` | **2** | `\n | }` |
| close then else (packed) | `}else{` | **2** | `}else | {` |
| close then else (spaced) | `} else {` | **3** | `} |  else |  {` |
| close then else: | `} else:` | **3** | `} |  else | :` |
| paren colon (Python) | `):` | **1** | `):` |
| paren colon newline indent | `):\n    ` | **2** | `):\n |     ` |
| close brace newline indent | `}\n    ` | **2** | `}\n |     ` |
| paren brace newline | `){\n` | **1** | `){\n` |
| paren brace newline indent | `){\n    ` | **2** | `){\n |     ` |
| close brace newline close brace | `}\n}` | **2** | `}\n | }` |

## 3. Indentation costs (newline + leading whitespace)

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| bare newline | `\n` | **1** | `\n` |
| newline + 2 spaces | `\n  ` | **2** | `\n |   ` |
| newline + 4 spaces | `\n    ` | **2** | `\n |     ` |
| newline + 8 spaces | `\n        ` | **2** | `\n |         ` |
| newline + 12 spaces | `\n            ` | **2** | `\n |             ` |
| newline + 16 spaces | `\n                ` | **2** | `\n |                 ` |
| double newline + 4 spaces | `\n\n    ` | **2** | `\n\n |     ` |
| newline + tab | `\n\t` | **2** | `\n | \t` |
| newline + 2 tabs | `\n\t\t` | **2** | `\n | \t\t` |

## 4. Full block openers — Python vs Nebula vs hypothetical

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| Python def block | `def f():\n    ` | **4** | `def |  f | ():\n |     ` |
| Python if block | `if x:\n    ` | **4** | `if |  x | :\n |     ` |
| Python for block | `for x in y:\n    ` | **6** | `for |  x |  in |  y | :\n |     ` |
| Python while block | `while x:\n    ` | **4** | `while |  x | :\n |     ` |
| Nebula fn brace | `fn f() {\n    ` | **5** | `fn |  f | () |  {\n |     ` |
| Nebula fn packed | `fn f(){\n    ` | **4** | `fn |  f | (){\n |     ` |
| Nebula if brace | `if x {\n    ` | **4** | `if |  x |  {\n |     ` |
| Nebula if packed | `if x{\n    ` | **4** | `if |  x | {\n |     ` |
| Nebula fo brace | `fo x in y {\n    ` | **6** | `fo |  x |  in |  y |  {\n |     ` |
| Nebula fo packed | `fo x in y{\n    ` | **6** | `fo |  x |  in |  y | {\n |     ` |
| Nebula wl brace | `wl x {\n    ` | **4** | `wl |  x |  {\n |     ` |
| Nebula match brace | `mc x {\n    ` | **4** | `mc |  x |  {\n |     ` |
| hypothetical fn colon | `fn f():\n    ` | **4** | `fn |  f | ():\n |     ` |
| hypothetical fo colon | `fo x in y:\n    ` | **6** | `fo |  x |  in |  y | :\n |     ` |

## 5. Keyword comparison (leading space, no trailing punctuation)

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| def | ` def` | **1** | ` def` |
| fn | ` fn` | **1** | ` fn` |
| return | ` return` | **1** | ` return` |
| rt | ` rt` | **1** | ` rt` |
| let | ` let` | **1** | ` let` |
| lt | ` lt` | **1** | ` lt` |
| mt | ` mt` | **1** | ` mt` |
| const | ` const` | **1** | ` const` |
| if | ` if` | **1** | ` if` |
| else | ` else` | **1** | ` else` |
| el | ` el` | **1** | ` el` |
| elif | ` elif` | **1** | ` elif` |
| for | ` for` | **1** | ` for` |
| fo | ` fo` | **1** | ` fo` |
| while | ` while` | **1** | ` while` |
| wl | ` wl` | **1** | ` wl` |
| loop | ` loop` | **1** | ` loop` |
| lp | ` lp` | **1** | ` lp` |
| match | ` match` | **1** | ` match` |
| mc | ` mc` | **1** | ` mc` |
| class | ` class` | **1** | ` class` |
| st | ` st` | **1** | ` st` |
| enum | ` enum` | **1** | ` enum` |
| en | ` en` | **1** | ` en` |
| impl | ` impl` | **1** | ` impl` |
| im | ` im` | **1** | ` im` |
| trait | ` trait` | **1** | ` trait` |
| tr | ` tr` | **1** | ` tr` |
| True | ` True` | **1** | ` True` |
| tt | ` tt` | **1** | ` tt` |
| False | ` False` | **1** | ` False` |
| ff | ` ff` | **1** | ` ff` |
| None | ` None` | **1** | ` None` |
| nil | ` nil` | **1** | ` nil` |
| self | ` self` | **1** | ` self` |
| sf | ` sf` | **1** | ` sf` |
| lambda | ` lambda` | **1** | ` lambda` |
| import | ` import` | **1** | ` import` |
| from | ` from` | **1** | ` from` |
| as | ` as` | **1** | ` as` |
| in | ` in` | **1** | ` in` |
| pass | ` pass` | **1** | ` pass` |
| break | ` break` | **1** | ` break` |
| bk | ` bk` | **1** | ` bk` |
| continue | ` continue` | **1** | ` continue` |
| ct | ` ct` | **1** | ` ct` |
| try | ` try` | **1** | ` try` |
| except | ` except` | **1** | ` except` |
| raise | ` raise` | **1** | ` raise` |
| with | ` with` | **1** | ` with` |
| yield | ` yield` | **1** | ` yield` |

## 6. Assignment forms

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| bare Python assign | `x = 0` | **4** | `x |  = |   | 0` |
| Nebula lt assign | `lt x = 0` | **5** | `lt |  x |  = |   | 0` |
| Nebula mt assign | `mt x = 0` | **5** | `mt |  x |  = |   | 0` |
| Python with type | `x: int = 0` | **6** | `x | : |  int |  = |   | 0` |
| Nebula with type | `lt x: Int = 0` | **7** | `lt |  x | : |  Int |  = |   | 0` |
| aug add | `x += 1` | **4** | `x |  += |   | 1` |
| aug sub | `x -= 1` | **4** | `x |  -= |   | 1` |
| tuple swap Py | `a, b = b, a` | **7** | `a | , |  b |  = |  b | , |  a` |
| walrus | `(x := 0)` | **5** | `(x |  := |   | 0 | )` |

## 7. Type annotations

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| colon int | `: int` | **2** | `: |  int` |
| colon str | `: str` | **2** | `: |  str` |
| colon bool | `: bool` | **2** | `: |  bool` |
| colon List int | `: List[int]` | **4** | `: |  List | [int | ]` |
| colon list int (lower) | `: list[int]` | **4** | `: |  list | [int | ]` |
| colon Tuple | `: Tuple[int, int]` | **6** | `: |  Tuple | [int | , |  int | ]` |
| colon Optional | `: Optional[int]` | **4** | `: |  Optional | [int | ]` |
| colon Dict | `: Dict[str, int]` | **6** | `: |  Dict | [str | , |  int | ]` |
| arrow int | ` -> int` | **2** | ` -> |  int` |
| arrow List int | ` -> List[int]` | **4** | ` -> |  List | [int | ]` |
| arrow Optional | ` -> Optional[int]` | **4** | ` -> |  Optional | [int | ]` |
| Nebula Int | `: Int` | **2** | `: |  Int` |
| Nebula Float | `: Float` | **2** | `: |  Float` |
| Nebula List int | `: List[Int]` | **4** | `: |  List | [Int | ]` |

## 8. Method call and builtin patterns

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| dot append paren | `.append(` | **2** | `.append | (` |
| dot join paren | `.join(` | **2** | `.join | (` |
| dot split paren | `.split(` | **2** | `.split | (` |
| dot map paren | `.map(` | **2** | `.map | (` |
| dot filter paren | `.filter(` | **2** | `.filter | (` |
| dot strip paren | `.strip(` | **2** | `.strip | (` |
| dot upper paren | `.upper(` | **2** | `.upper | (` |
| len paren | `len(` | **2** | `len | (` |
| range paren | `range(` | **2** | `range | (` |
| enumerate paren | `enumerate(` | **2** | `enumerate | (` |
| zip paren | `zip(` | **2** | `zip | (` |
| sorted paren | `sorted(` | **2** | `sorted | (` |
| print paren | `print(` | **2** | `print | (` |
| Nebula prt paren | `prt(` | **2** | `prt | (` |
| Nebula ln paren | `ln(` | **2** | `ln | (` |
| Nebula rng paren | `rng(` | **2** | `rng | (` |
| Nebula enu paren | `enu(` | **2** | `enu | (` |
| Nebula srt paren | `srt(` | **3** | `s | rt | (` |
| Nebula mp paren | `.mp(` | **2** | `.mp | (` |
| Nebula fl paren | `.fl(` | **2** | `.fl | (` |
| Nebula jn paren | `.jn(` | **3** | `.j | n | (` |
| Nebula psh paren | `.psh(` | **3** | `.p | sh | (` |

## 9. Complete statement comparisons (the real test)

| Label | Text | Tokens | Pieces |
|---|---|---|---|
| Py simple def | `def add(a, b):\n    return a + b\n` | **12** | `def |  add | (a | , |  b | ):\n |     |  return |  a |  + |  b | \n` |
| Neb braces | `fn add(a, b) {\n    rt a + b\n}\n` | **14** | `fn |  add | (a | , |  b | ) |  {\n |     |  rt |  a |  + |  b | \n | }\n` |
| Neb braces packed | `fn add(a,b){\n    rt a+b\n}\n` | **11** | `fn |  add | (a | ,b | ){\n |     |  rt |  a | +b | \n | }\n` |
| Neb short-form | `fn add(a, b) = a + b\n` | **11** | `fn |  add | (a | , |  b | ) |  = |  a |  + |  b | \n` |
| Neb indent (proposed) | `fn add(a, b):\n    rt a + b\n` | **12** | `fn |  add | (a | , |  b | ):\n |     |  rt |  a |  + |  b | \n` |
| Py if | `if x > 0:\n    return x\n` | **10** | `if |  x |  > |   | 0 | :\n |     |  return |  x | \n` |
| Neb if braces | `if x > 0 {\n    rt x\n}\n` | **11** | `if |  x |  > |   | 0 |  {\n |     |  rt |  x | \n | }\n` |
| Neb if indent | `if x > 0:\n    rt x\n` | **10** | `if |  x |  > |   | 0 | :\n |     |  rt |  x | \n` |
| Py for append | `for i in range(n):\n    out.append(i)\n` | **11** | `for |  i |  in |  range | (n | ):\n |     |  out | .append | (i | )\n` |
| Neb for braces | `fo i in rng(n) {\n    out.psh(i)\n}\n` | **14** | `fo |  i |  in |  rng | (n | ) |  {\n |     |  out | .p | sh | (i | )\n | }\n` |
| Neb for indent | `fo i in rng(n):\n    out.psh(i)\n` | **12** | `fo |  i |  in |  rng | (n | ):\n |     |  out | .p | sh | (i | )\n` |
| Py nested | `def f(n):\n    for i in range(n):\n        if i % 2 == 0:\n            print(i)\n` | **25** | `def |  f | (n | ):\n |     |  for |  i |  in |  range | (n | ):\n |         |  if |  i |  % |   | 2 |  == |   | 0 | :\n |             |  print | (i | )\n` |
| Neb braces nested | `fn f(n) {\n    fo i in rng(n) {\n        if i % 2 == 0 {\n            prt(i)\n        }\n    }\n}\n` | **32** | `fn |  f | (n | ) |  {\n |     |  fo |  i |  in |  rng | (n | ) |  {\n |         |  if |  i |  % |   | 2 |  == |   | 0 |  {\n |             |  prt | (i | )\n |         |  }\n |     |  }\n | }\n` |
| Neb indent nested | `fn f(n):\n    fo i in rng(n):\n        if i % 2 == 0:\n            prt(i)\n` | **25** | `fn |  f | (n | ):\n |     |  fo |  i |  in |  rng | (n | ):\n |         |  if |  i |  % |   | 2 |  == |   | 0 | :\n |             |  prt | (i | )\n` |

