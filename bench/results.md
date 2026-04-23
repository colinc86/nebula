# Nebula Token Compression Benchmark

**Date:** 2026-04-22 20:23
**Tokenizer:** cl100k_base (tiktoken)
**Corpus:** Task Manager (~100 lines per language, equivalent logic)
**Nebula binary:** `/Users/colin/Local Documents/Programming/Misc/nebula/bench/../compiler/target/release/nebula`

## Results

| Language   | Src Tokens | Neb Tokens | Token +/- | Src Bytes | Neb Bytes | Byte +/- | Src Lines | Neb Lines |
|------------|-----------|-----------|----------|----------|----------|---------|----------|----------|
| Python     |       870 |       837 | -  3.8%  |    3,232 |    3,072 | -  5.0%  |      135 |      101 |
| JavaScript |       933 |       833 | - 10.7%  |    3,564 |    2,792 | - 21.7%  |      155 |      103 |
| TypeScript |      1140 |       983 | - 13.8%  |    4,368 |    3,304 | - 24.4%  |      177 |      121 |
| Go         |      1024 |       915 | - 10.6%  |    3,297 |    3,055 | -  7.3%  |      175 |      112 |
| Swift      |       967 |       870 | - 10.0%  |    3,489 |    2,900 | - 16.9%  |      148 |       98 |
| Kotlin     |       922 |       843 | -  8.6%  |    3,338 |    2,760 | - 17.3%  |      143 |       97 |
| Rust       |      1071 |       851 | - 20.5%  |    3,797 |    2,767 | - 27.1%  |      158 |      102 |
| C          |      1258 |      1177 | -  6.4%  |    4,131 |    3,557 | - 13.9%  |      174 |      136 |

## Summary

- **Overall token compression:** +10.7% (8185 -> 7309 tokens)
- **Mean token compression:** +10.6%
- **Mean byte compression:** +16.7%
- **Best compression:** Rust (+20.5% tokens)
- **Least compression:** Python (+3.8% tokens)

### Interpretation

Positive percentages = Nebula is smaller (compression).
Negative percentages = Nebula is larger (expansion).

These results represent the **pre-fine-tune baseline**. A model fine-tuned on Nebula
would eliminate the need for a syntax reference card (~500 tokens overhead), making
the effective compression even better for multi-file sessions.
