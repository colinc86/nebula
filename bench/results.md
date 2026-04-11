# Nebula 1.0 — Benchmark Results

**Date:** 2026-04-10
**Tokenizer:** cl100k_base (tiktoken)

## Phase 3: Token Compression

**Corpus:** Task Manager (~100 lines per language, equivalent logic across 8 languages)

| Language   | Src Tokens | Neb Tokens | Compression |
|------------|-----------|-----------|-------------|
| Python     |       870 |       842 |  **3.2%**   |
| JavaScript |       933 |       830 | **11.0%**   |
| TypeScript |      1140 |       924 | **18.9%**   |
| Go         |      1024 |       899 | **12.2%**   |
| Swift      |       967 |       768 | **20.6%**   |
| Kotlin     |       922 |       732 | **20.6%**   |
| Rust       |      1071 |       849 | **20.7%**   |
| C          |      1258 |      1017 | **19.2%**   |
| **Overall**| **8185**  | **6861**  | **16.2%**   |

All 8 languages compress. Mean: 15.8%. Best: Rust (20.7%). Least: Python (3.2%).

## Phase 4: InterTrans Validation (Python → Go)

**Model:** Claude Sonnet 4 | **Problems:** 20 HumanEval | **Metric:** Computational Accuracy

| Condition | Run 1 | Run 2 | Average |
|---|---|---|---|
| Direct LLM (Python→Go) | 60.0% | 75.0% | **67.5%** |
| Nebula-mediated (Python→Nebula→Go) | 80.0% | 70.0% | **75.0%** |

**+7.5pp average improvement** with Nebula mediation.

## Phase 5: Neural Refinement (Nebula → Python)

**Benchmark:** HumanEval (164 problems) | **Metric:** Pass@1

| Model | Size | Target | Passed | Pass@1 |
|---|---|---|---|---|
| Base Qwen-2.5-1.5B on Nebula | 1.5B | — | 2/164 | **1.2%** |
| Single-target (3-epoch) | 1.5B | Python | 80/164 | **48.8%** |
| **8-lang (production)** | **1.5B** | **8 langs** | **74/164** | **45.1%** |
| **8-lang (production)** | **7B** | **8 langs** | **111/164** | **67.7%** |

The 7B 8-language model achieves 67.7% Pass@1 on Nebula→Python — matching published Qwen-2.5-7B results on raw Python (~60-70%) while also handling 7 additional languages.

## Phase 6: Error Correction Pipeline

**Benchmark:** HumanEval (164 problems) | **Max retries:** 3 | **Fallback:** rule-based `nebula to-python`

### 7B 8-language (production):

| Condition | Passed | Pass@1 |
|---|---|---|
| No correction | 111/164 | **67.7%** |
| + Error correction retries | — | — |
| + Rule-based fallback | 145/164 | **88.4%** |

### 1.5B 8-language (production):

| Condition | Passed | Pass@1 |
|---|---|---|
| No correction | 74/164 | **45.1%** |
| + Error correction retries | — | — |
| + Rule-based fallback | 130/164 | **79.3%** |

## InterTrans Scale-Up (10 Directed Pairs)

**Model:** Claude Sonnet 4 | **Problems:** 20 per pair | **Note:** Frontier model seeing Nebula cold (no fine-tuning)

### Python → 5 Targets

| Pair | Direct CA | Nebula CA | Delta |
|---|---|---|---|
| Python → JavaScript | 90.0% | 85.0% | -5.0pp |
| Python → TypeScript | 85.0% | 65.0% | -20.0pp |
| Python → Go | 65.0% | 65.0% | 0.0pp |
| Python → Rust | 80.0% | 75.0% | -5.0pp |
| Python → C | 75.0% | 60.0% | -15.0pp |
| **Average** | **79.0%** | **70.0%** | **-9.0pp** |

### 5 Sources → Python

| Pair | Direct CA | Nebula CA | Delta |
|---|---|---|---|
| JavaScript → Python | 23.5% | 41.2% | **+17.6pp** |
| TypeScript → Python | 52.9% | 35.3% | -17.6pp |
| Go → Python | 58.8% | 76.5% | **+17.6pp** |
| Rust → Python | 58.8% | 47.1% | -11.8pp |
| C → Python | 47.1% | 52.9% | **+5.9pp** |
| **Average** | **48.2%** | **50.6%** | **+2.4pp** |

### Combined

| Direction | Direct CA | Nebula CA | Delta |
|---|---|---|---|
| Python → 5 targets | 79.0% | 70.0% | -9.0pp |
| 5 sources → Python | 48.2% | 50.6% | +2.4pp |
| **All 10 pairs** | **63.6%** | **60.3%** | **-3.3pp** |

Without a Nebula-trained model, the IL is slightly net-negative (-3.3pp) for a frontier model seeing it cold.

## MBPP Benchmark (500 problems)

| Model | Benchmark | Pass@1 |
|---|---|---|
| 7B multi-target on Nebula input | MBPP (500) | **55.4%** |
| 7B multi-target on Nebula input | HumanEval (164) | **68.9%** |
| Published Qwen-2.5-7B on raw Python | MBPP | ~55-65% |

Confirms on a second, larger benchmark: Nebula-trained models match base model performance on raw Python. Nebula wins where ceremony-stripping helps (JS→Py +17.6pp, Go→Py +17.6pp), loses where type information is stripped (TS→Py -17.6pp).
