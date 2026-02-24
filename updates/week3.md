# Week 3 Update (Implemented)

## Scope
- Embedding pipeline.

## Implemented
- `src/embeddings/client.rs`
  - deterministic 768-d fallback embedder
  - batch embedding API
  - cosine similarity utility
- `src/bin/embed.rs`
  - scans `chunks` CF
  - resumable behavior (skips existing embeddings)
  - batch embedding + writes to `embeddings` CF via `bincode`
  - progress logging

## Notes
- ORT + tokenizer ONNX runtime integration is planned next iteration.
- Current fallback keeps pipeline fully runnable end-to-end.
