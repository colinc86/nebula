These are notes from the user that they were thinking about as Claude Code sessions were working through prompts.

1. The MCP should have modes like translation, refactor, auto(? maybe), and general
  - Depending on which mode is set dictates whether we use the MCP for different tasks to avoid degredations in token usage
  - We showed that the model will kick ass at refactoring large codebases but not do well in small general coding
  - If the MoE model fixes this then we won't need these restrictions
2. Together.ai only caps the epocs on qwen coder MoE at 2 for validation - I can choose up to 20 in toe FE UI for finetuning