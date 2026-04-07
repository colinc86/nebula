# Nebula Token Compression Benchmark

**Date:** 2026-04-07 17:16
**Tokenizer:** cl100k_base (tiktoken)
**Corpus:** Task Manager (~100 lines per language, equivalent logic)
**Nebula binary:** `compiler/target/debug/nebula`

## Results

| Language   | Src Tokens | Neb Tokens | Token +/- | Src Bytes | Neb Bytes | Byte +/- | Src Lines | Neb Lines |
|------------|-----------|-----------|----------|----------|----------|---------|----------|----------|
| Python     |       870 |       868 | -  0.2%  |    3,232 |    2,605 | - 19.4%  |      135 |      135 |
| JavaScript |       933 |       863 | -  7.5%  |    3,564 |    2,612 | - 26.7%  |      155 |      135 |
| TypeScript |      1140 |       978 | - 14.2%  |    4,368 |    3,043 | - 30.3%  |      177 |      153 |
| Go         |      1024 |       889 | - 13.2%  |    3,297 |    2,945 | - 10.7%  |      175 |      142 |
| Swift      |       967 |       847 | - 12.4%  |    3,489 |    2,629 | - 24.6%  |      148 |      124 |
| Kotlin     |       922 |       803 | - 12.9%  |    3,338 |    2,523 | - 24.4%  |      143 |      121 |
| Rust       |      1071 |       975 | -  9.0%  |    3,797 |    2,955 | - 22.2%  |      158 |      133 |
| C          |      1258 |      1173 | -  6.8%  |    4,131 |    3,427 | - 17.0%  |      174 |      155 |

## Summary

- **Overall token compression:** +9.6% (8185 -> 7396 tokens)
- **Mean token compression:** +9.5%
- **Mean byte compression:** +21.9%
- **Best compression:** TypeScript (+14.2% tokens)
- **Least compression:** Python (+0.2% tokens)

### Interpretation

Positive percentages = Nebula is smaller (compression).
Negative percentages = Nebula is larger (expansion).

These results represent the **pre-fine-tune baseline**. A model fine-tuned on Nebula
would eliminate the need for a syntax reference card (~500 tokens overhead), making
the effective compression even better for multi-file sessions.
