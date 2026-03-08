use std::{cmp::Ordering, fs};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ChunkId, EmbeddingVec};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    chunk_id: ChunkId,
    vector: Vec<f32>,
}

/// Exact linear-scan baseline used for ANN recall benchmarking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BruteForceIndex {
    dim: usize,
    entries: Vec<Entry>,
}

impl BruteForceIndex {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            entries: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn insert(&mut self, chunk_id: ChunkId, vector: EmbeddingVec) {
        if vector.len() != self.dim {
            return;
        }

        self.entries.push(Entry { chunk_id, vector });
    }

    pub fn search(&self, query: &EmbeddingVec, k: usize) -> Vec<(ChunkId, f32)> {
        if k == 0 || self.entries.is_empty() || query.len() != self.dim {
            return Vec::new();
        }

        let chunk_size = 4096usize;
        let partial = self
            .entries
            .par_chunks(chunk_size)
            .map(|chunk| {
                let mut local_top = Vec::with_capacity(k);
                for e in chunk {
                    let score = cosine(query, &e.vector);
                    push_top_k(&mut local_top, (e.chunk_id.clone(), score), k);
                }
                local_top
            })
            .collect::<Vec<_>>();

        let mut top = Vec::with_capacity(k);
        for part in partial {
            for cand in part {
                push_top_k(&mut top, cand, k);
            }
        }

        top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        top
    }

    pub fn save_to_path(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let bytes = bincode::serialize(self)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load_from_path(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = fs::read(path)?;
        let idx: Self = bincode::deserialize(&bytes)?;
        Ok(idx)
    }
}

fn push_top_k(top: &mut Vec<(ChunkId, f32)>, cand: (ChunkId, f32), k: usize) {
    if top.len() < k {
        top.push(cand);
        return;
    }

    let mut min_idx = 0usize;
    let mut min_score = top[0].1;
    for (i, (_, s)) in top.iter().enumerate().skip(1) {
        if *s < min_score {
            min_score = *s;
            min_idx = i;
        }
    }

    if cand.1 > min_score {
        top[min_idx] = cand;
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if b.len() != a.len() {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut an = 0.0;
    let mut bn = 0.0;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        an += a[i] * a[i];
        bn += b[i] * b[i];
    }

    if an == 0.0 || bn == 0.0 {
        0.0
    } else {
        dot / (an.sqrt() * bn.sqrt())
    }
}
