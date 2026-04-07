# Nebula Token Compression Benchmark

**Date:** 2026-04-07 16:47
**Tokenizer:** cl100k_base (tiktoken)
**Corpus:** Task Manager (~100 lines per language, equivalent logic)
**Nebula binary:** `/Users/colin/Local Documents/Programming/nebula/bench/../compiler/target/debug/nebula`

## Results

| Language   | Src Tokens | Neb Tokens | Token +/- | Src Bytes | Neb Bytes | Byte +/- | Src Lines | Neb Lines |
|------------|-----------|-----------|----------|----------|----------|---------|----------|----------|
| Python     |       870 |       931 | +  7.0%  |    3,232 |    3,159 | -  2.3%  |      135 |      135 |
| JavaScript |       933 |       936 | +  0.3%  |    3,564 |    3,146 | - 11.7%  |      155 |      132 |
| TypeScript |      1140 |      1068 | -  6.3%  |    4,368 |    3,627 | - 17.0%  |      177 |      150 |
| Go         |      1024 |      1001 | -  2.2%  |    3,297 |    3,146 | -  4.6%  |      175 |      149 |
| Swift      |       967 |       853 | - 11.8%  |    3,489 |    2,681 | - 23.2%  |      148 |      147 |
| Kotlin     |       922 |       807 | - 12.5%  |    3,338 |    2,570 | - 23.0%  |      143 |      142 |
| Rust       |      1071 |      1052 | -  1.8%  |    3,797 |    3,510 | -  7.6%  |      158 |      134 |
| C          |      1258 |      1279 | +  1.7%  |    4,131 |    3,999 | -  3.2%  |      174 |      156 |

## Summary

- **Overall token compression:** +3.2% (8185 -> 7927 tokens)
- **Mean token compression:** +3.2%
- **Mean byte compression:** +11.6%
- **Best compression:** Kotlin (+12.5% tokens)
- **Least compression:** Python (-7.0% tokens)

### Interpretation

Positive percentages = Nebula is smaller (compression).
Negative percentages = Nebula is larger (expansion).

These results represent the **pre-fine-tune baseline**. A model fine-tuned on Nebula
would eliminate the need for a syntax reference card (~500 tokens overhead), making
the effective compression even better for multi-file sessions.
