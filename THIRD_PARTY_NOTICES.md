Nebula — Third-Party Notices

This file lists third-party software, datasets, and models used by this
project, along with their respective licenses.

================================================================================
COMPILER DEPENDENCIES (Rust)
================================================================================

tree-sitter (0.26)
  License: MIT
  https://github.com/tree-sitter/tree-sitter

tree-sitter-python (0.25)
  License: MIT
  https://github.com/tree-sitter/tree-sitter-python

tree-sitter-javascript (0.25)
  License: MIT
  https://github.com/tree-sitter/tree-sitter-javascript

tree-sitter-typescript (0.23)
  License: MIT
  https://github.com/tree-sitter/tree-sitter-typescript

tree-sitter-go (0.25)
  License: MIT
  https://github.com/tree-sitter/tree-sitter-go

tree-sitter-swift (0.7)
  License: MIT
  https://github.com/alex-pinkus/tree-sitter-swift

tree-sitter-rust (0.24)
  License: MIT
  https://github.com/tree-sitter/tree-sitter-rust

tree-sitter-c (0.24)
  License: MIT
  https://github.com/tree-sitter/tree-sitter-c

serde (1.x)
  License: MIT OR Apache-2.0
  https://github.com/serde-rs/serde

serde_json (1.x)
  License: MIT OR Apache-2.0
  https://github.com/serde-rs/json

ureq (2.x)
  License: MIT OR Apache-2.0
  https://github.com/algesten/ureq

================================================================================
TRAINING DATA
================================================================================

StarCoderData
  License: OpenRAIL-M (Open Responsible AI License — Modified)
  https://huggingface.co/datasets/bigcode/starcoderdata
  Used for: Fine-tuning training data (Python, JS, TS, Go, Rust, C, Kotlin)

  NOTE: The OpenRAIL-M license permits use, modification, and redistribution
  but includes use-based restrictions. Any model weights derived from this
  data must be distributed with the same use-based restrictions attached.
  Full license: https://huggingface.co/spaces/bigcode/bigcode-model-license-agreement

The Stack
  License: OpenRAIL-M (Open Responsible AI License — Modified)
  https://huggingface.co/datasets/bigcode/the-stack
  Used for: Fine-tuning training data (Swift, as fallback)

  Same OpenRAIL-M terms as StarCoderData above.

================================================================================
BASE MODELS (fine-tuned via Together AI)
================================================================================

Qwen-2.5 (1.5B, 7B, 14B)
  License: Apache-2.0
  https://huggingface.co/Qwen
  Copyright: Alibaba Cloud
  Used for: Base models fine-tuned into nebula-8lang-* variants

================================================================================
BENCHMARK DATASETS
================================================================================

HumanEval
  License: MIT
  https://github.com/openai/human-eval
  Copyright: OpenAI
  Used for: Pass@1 evaluation of code generation

MBPP (Mostly Basic Programming Problems)
  License: Apache-2.0 (CC-BY-4.0 for the dataset)
  https://github.com/google-research/google-research/tree/master/mbpp
  Copyright: Google LLC
  Used for: Multi-problem benchmark evaluation

================================================================================
ACADEMIC WORK (included in bench/external/)
================================================================================

SimPy
  License: Apache-2.0
  https://github.com/v587su/SimPy
  Paper: "Towards Effective and Efficient Token Efficiency of Code LLMs
         through Grammar Redesign" (ISSTA 2024, arXiv:2404.16333)
  Used for: Baseline comparison

Token Sugar
  License: Apache-2.0
  https://github.com/v587su/TokenSugar
  Paper: "Token Sugar: Semantic-Level Pattern Substitution for Token
         Efficiency" (ASE 2025, arXiv:2512.08266)
  Used for: Baseline comparison

BigCode Evaluation Harness
  License: Apache-2.0
  https://github.com/bigcode-project/bigcode-evaluation-harness
  Used for: Standardized HumanEval/MBPP evaluation (included within
  SimPy and Token Sugar directories)

================================================================================
PYTHON DEPENDENCIES (bench/)
================================================================================

tiktoken
  License: MIT
  https://github.com/openai/tiktoken
  Copyright: OpenAI

datasets (Hugging Face)
  License: Apache-2.0
  https://github.com/huggingface/datasets

together (Together AI SDK)
  License: Apache-2.0
  https://github.com/togethercomputer/together-python
