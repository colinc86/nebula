# Nebula Token Compression Benchmark

**Date:** 2026-04-07 16:59
**Tokenizer:** cl100k_base (tiktoken)
**Corpus:** Task Manager (~100 lines per language, equivalent logic)
**Nebula binary:** `/Users/colin/Local Documents/Programming/nebula/bench/../compiler/target/debug/nebula`

## Results

| Language   | Src Tokens | Neb Tokens | Token +/- | Src Bytes | Neb Bytes | Byte +/- | Src Lines | Neb Lines |
|------------|-----------|-----------|----------|----------|----------|---------|----------|----------|
| Python     |       870 |       849 | -  2.4%  |    3,232 |    2,578 | - 20.2%  |      135 |      133 |
| JavaScript |       933 |       857 | -  8.1%  |    3,564 |    2,592 | - 27.3%  |      155 |      131 |
| TypeScript |      1140 |       974 | - 14.6%  |    4,368 |    3,031 | - 30.6%  |      177 |      149 |
| Go         |      1024 |       896 | - 12.5%  |    3,297 |    2,980 | -  9.6%  |      175 |      142 |
| Swift      |       967 |       843 | - 12.8%  |    3,489 |    2,614 | - 25.1%  |      148 |      124 |
| Kotlin     |       922 |       796 | - 13.7%  |    3,338 |    2,504 | - 25.0%  |      143 |      121 |
| Rust       |      1071 |       974 | -  9.1%  |    3,797 |    2,962 | - 22.0%  |      158 |      133 |
| C          |      1258 |      1183 | -  6.0%  |    4,131 |    3,439 | - 16.8%  |      174 |      155 |

## Summary

- **Overall token compression:** +9.9% (8185 -> 7372 tokens)
- **Mean token compression:** +9.9%
- **Mean byte compression:** +22.1%
- **Best compression:** TypeScript (+14.6% tokens)
- **Least compression:** Python (+2.4% tokens)

### Interpretation

Positive percentages = Nebula is smaller (compression).
Negative percentages = Nebula is larger (expansion).

These results represent the **pre-fine-tune baseline**. A model fine-tuned on Nebula
would eliminate the need for a syntax reference card (~500 tokens overhead), making
the effective compression even better for multi-file sessions.
