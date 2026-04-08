# Nebula Token Compression Benchmark

**Date:** 2026-04-07 21:27
**Tokenizer:** cl100k_base (tiktoken)
**Corpus:** Task Manager (~100 lines per language, equivalent logic)
**Nebula binary:** `/Users/colin/Local Documents/Programming/nebula/bench/../compiler/target/debug/nebula`

## Results

| Language   | Src Tokens | Neb Tokens | Token +/- | Src Bytes | Neb Bytes | Byte +/- | Src Lines | Neb Lines |
|------------|-----------|-----------|----------|----------|----------|---------|----------|----------|
| Python     |       870 |       944 | +  8.5%  |    3,232 |    2,694 | - 16.6%  |      135 |      134 |
| JavaScript |       933 |       939 | +  0.6%  |    3,564 |    2,704 | - 24.1%  |      155 |      135 |
| TypeScript |      1140 |      1030 | -  9.6%  |    4,368 |    3,014 | - 31.0%  |      177 |      147 |
| Go         |      1024 |       969 | -  5.4%  |    3,297 |    2,866 | - 13.1%  |      175 |      142 |
| Swift      |       967 |       827 | - 14.5%  |    3,489 |    2,611 | - 25.2%  |      148 |      128 |
| Kotlin     |       922 |       785 | - 14.9%  |    3,338 |    2,473 | - 25.9%  |      143 |      126 |
| Rust       |      1071 |       965 | -  9.9%  |    3,797 |    2,901 | - 23.6%  |      158 |      133 |
| C          |      1258 |      1123 | - 10.7%  |    4,131 |    3,202 | - 22.5%  |      174 |      155 |

## Summary

- **Overall token compression:** +7.4% (8185 -> 7582 tokens)
- **Mean token compression:** +7.0%
- **Mean byte compression:** +22.8%
- **Best compression:** Kotlin (+14.9% tokens)
- **Least compression:** Python (-8.5% tokens)

### Interpretation

Positive percentages = Nebula is smaller (compression).
Negative percentages = Nebula is larger (expansion).

These results represent the **pre-fine-tune baseline**. A model fine-tuned on Nebula
would eliminate the need for a syntax reference card (~500 tokens overhead), making
the effective compression even better for multi-file sessions.
