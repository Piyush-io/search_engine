use crate::{ChunkId, EmbeddingVec};

use super::{bruteforce::BruteForceIndex, hnsw::HnswIndex};

pub trait VectorIndex: Send + Sync {
    fn search(&self, query: &EmbeddingVec, k: usize) -> Vec<(ChunkId, f32)>;
    fn len(&self) -> usize;
}

impl VectorIndex for HnswIndex {
    fn search(&self, query: &EmbeddingVec, k: usize) -> Vec<(ChunkId, f32)> {
        self.search(query, k)
    }

    fn len(&self) -> usize {
        self.len()
    }
}

impl VectorIndex for BruteForceIndex {
    fn search(&self, query: &EmbeddingVec, k: usize) -> Vec<(ChunkId, f32)> {
        self.search(query, k)
    }

    fn len(&self) -> usize {
        self.len()
    }
}
