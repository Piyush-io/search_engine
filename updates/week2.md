# Week 2 Update (Implemented)

## Scope
- Sentence chunking + context attachment + statement chaining.

## Implemented
- `src/chunking/sentencizer.rs`
  - fallback sentence splitter (`. ! ? \n`) with whitespace normalization
- `src/chunking/context.rs`
  - context attachment utility
  - configurable depth helper (`with_context_depth`)
- `src/chunking/chaining.rs`
  - heuristic chaining fallback (pronoun/reference cue handling)
- `src/bin/label.rs`
  - interactive labeling CLI
  - JSONL output to `training/labels.jsonl`
  - supports 0/1/2/3 labels and quit flow
- `training/train_chaining.py`
  - DistilBERT fine-tuning pipeline for pair classification
- `training/export_onnx.py`
  - ONNX export wrapper via `optimum-cli`

## Notes
- Rust-side ONNX inference for chaining remains pending (currently heuristic fallback).
