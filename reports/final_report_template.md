# Neural Search Engine for Computer Science

## 1. Abstract
- Problem statement
- Approach summary
- Key results

## 2. Introduction
- Why keyword search falls short for complex CS queries
- Why neural sentence-level retrieval helps

## 3. Related Work
- Wilson Lin architecture and production scale

## 4. System Architecture
- Crawl -> Normalize -> Chunk -> Embed -> Index -> Serve

## 5. Implementation
### 5.1 Crawler
### 5.2 Extraction
### 5.3 Chunking + Statement Chaining
### 5.4 Embeddings
### 5.5 Vector Search
### 5.6 Web SERP + Tracking
### 5.7 Knowledge Panel

## 6. Configurability and Scale
- Config table
- Local scale vs production scale mapping

## 7. Evaluation
- Throughput metrics
- Latency metrics
- Recall metrics
- Query quality comparisons

## 8. Limitations
- Current fallbacks and simplifications

## 9. Future Work
- ONNX runtime inference in Rust
- Real HNSW backend integration
- Better statement chaining model at runtime

## 10. Conclusion
