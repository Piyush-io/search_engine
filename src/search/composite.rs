use std::{collections::HashMap, sync::Arc};

use crate::{ChunkId, EmbeddingVec};

use super::{bruteforce::BruteForceIndex, vector_index::VectorIndex};

pub struct CompositeVectorIndex {
    base: Arc<dyn VectorIndex>,
    delta: Option<BruteForceIndex>,
    tombstones: std::collections::HashSet<ChunkId>,
}

impl CompositeVectorIndex {
    pub fn new(
        base: Arc<dyn VectorIndex>,
        delta: Option<BruteForceIndex>,
        tombstones: std::collections::HashSet<ChunkId>,
    ) -> Self {
        Self {
            base,
            delta,
            tombstones,
        }
    }
}

impl VectorIndex for CompositeVectorIndex {
    fn search(&self, query: &EmbeddingVec, k: usize) -> Vec<(ChunkId, f32)> {
        if k == 0 {
            return Vec::new();
        }

        let oversample = k
            .saturating_mul(2)
            .max(k + self.tombstones.len().min(k * 8));
        let mut merged = HashMap::<ChunkId, f32>::new();

        for (id, score) in self.base.search(query, oversample) {
            if self.tombstones.contains(&id) {
                continue;
            }
            merged
                .entry(id)
                .and_modify(|existing| *existing = existing.max(score))
                .or_insert(score);
        }

        if let Some(delta) = &self.delta {
            for (id, score) in delta.search(query, oversample) {
                if self.tombstones.contains(&id) {
                    continue;
                }
                merged
                    .entry(id)
                    .and_modify(|existing| *existing = existing.max(score))
                    .or_insert(score);
            }
        }

        let mut ranked: Vec<(ChunkId, f32)> = merged.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked.truncate(k);
        ranked
    }

    fn len(&self) -> usize {
        let delta_len = self.delta.as_ref().map(|idx| idx.len()).unwrap_or(0);
        self.base.len() + delta_len
    }
}
