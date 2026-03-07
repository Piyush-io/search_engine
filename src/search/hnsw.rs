use std::fs;

use hnsw_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{ChunkId, EmbeddingVec};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    chunk_id: ChunkId,
    vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedIndex {
    dim: usize,
    m: usize,
    ef_construction: usize,
    ef_search: usize,
    max_elements: usize,
    entries: Vec<Entry>,
}

/// Real ANN index backed by hnsw_rs.
pub struct HnswIndex {
    dim: usize,
    m: usize,
    ef_construction: usize,
    ef_search: usize,
    max_elements: usize,
    hnsw: Hnsw<'static, f32, DistCosine>,
    entries: Vec<Entry>,
}

impl HnswIndex {
    pub fn new(dim: usize) -> Self {
        Self::with_params(dim, 16, 200, 80, 100_000)
    }

    pub fn with_params(
        dim: usize,
        m: usize,
        ef_construction: usize,
        ef_search: usize,
        max_elements: usize,
    ) -> Self {
        let hnsw = make_hnsw(m, ef_construction, max_elements);
        Self {
            dim,
            m,
            ef_construction,
            ef_search,
            max_elements,
            hnsw,
            entries: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn set_ef_search(&mut self, ef_search: usize) {
        self.ef_search = ef_search.max(1);
    }

    pub fn insert(&mut self, chunk_id: ChunkId, vector: EmbeddingVec) {
        if vector.len() != self.dim {
            return;
        }

        let idx = self.entries.len();
        self.hnsw.insert((vector.as_slice(), idx));
        self.entries.push(Entry { chunk_id, vector });
    }

    pub fn search(&self, query: &EmbeddingVec, k: usize) -> Vec<(ChunkId, f32)> {
        if k == 0 || self.entries.is_empty() || query.len() != self.dim {
            return Vec::new();
        }

        let ef = self.ef_search.max(k);
        let neighbours = self.hnsw.search(query.as_slice(), k, ef);

        let mut out = Vec::with_capacity(neighbours.len());
        for n in neighbours {
            let idx = n.d_id;
            if let Some(e) = self.entries.get(idx) {
                let sim = (1.0_f32 - n.distance).clamp(-1.0, 1.0);
                out.push((e.chunk_id.clone(), sim));
            }
        }

        out
    }

    pub fn save_to_path(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let persisted = PersistedIndex {
            dim: self.dim,
            m: self.m,
            ef_construction: self.ef_construction,
            ef_search: self.ef_search,
            max_elements: self.max_elements,
            entries: self.entries.clone(),
        };

        let bytes = bincode::serialize(&persisted)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load_from_path(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = fs::read(path)?;
        let persisted: PersistedIndex = bincode::deserialize(&bytes)?;

        let mut idx = Self::with_params(
            persisted.dim,
            persisted.m,
            persisted.ef_construction,
            persisted.ef_search,
            persisted.max_elements.max(persisted.entries.len()),
        );

        for e in persisted.entries {
            idx.insert(e.chunk_id, e.vector);
        }

        Ok(idx)
    }
}

fn make_hnsw(
    m: usize,
    ef_construction: usize,
    max_elements: usize,
) -> Hnsw<'static, f32, DistCosine> {
    let max_nb_connection = m.max(4);
    let nb_elem = max_elements.max(1);
    let nb_layer = 16.min((nb_elem as f32).ln().trunc().max(1.0) as usize);

    Hnsw::<f32, DistCosine>::new(
        max_nb_connection,
        nb_elem,
        nb_layer,
        ef_construction.max(max_nb_connection),
        DistCosine {},
    )
}
