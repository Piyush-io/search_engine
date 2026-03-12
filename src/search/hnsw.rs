use std::fs;
use std::path::Path;

use hnsw_rs::api::AnnT;
use hnsw_rs::hnswio::*;
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
    _io: Option<HnswIo>,
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
            _io: None,
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
        self.entries.push(Entry {
            chunk_id,
            vector: Vec::new(),
        });
    }

    /// Parallel bulk insert using Rayon — call `push_chunk_id` for each entry afterward.
    /// `data` is `&[(&Vec<f32>, usize)]` where the usize is the numeric index (0-based, contiguous).
    pub fn parallel_insert_slice(&mut self, data: &[(&Vec<f32>, usize)]) {
        self.hnsw.parallel_insert(data);
    }

    /// Register a chunk_id for a numeric index produced by `parallel_insert_slice`.
    /// Must be called in the same order as the indices passed to `parallel_insert_slice`.
    pub fn push_chunk_id(&mut self, chunk_id: ChunkId) {
        self.entries.push(Entry {
            chunk_id,
            vector: Vec::new(),
        });
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
        let p = Path::new(path);
        let parent = p.parent().unwrap_or(Path::new("."));
        let basename = p.file_name().unwrap_or_default().to_string_lossy();

        // Dump graph and data via hnswio
        self.hnsw.file_dump(parent, &basename)?;

        let persisted = PersistedIndex {
            dim: self.dim,
            m: self.m,
            ef_construction: self.ef_construction,
            ef_search: self.ef_search,
            max_elements: self.max_elements,
            entries: self.entries.clone(),
        };

        // Dump metadata
        let bytes = bincode::serialize(&persisted)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load_from_path(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = fs::read(path)?;
        let persisted: PersistedIndex = bincode::deserialize(&bytes)?;

        let p = Path::new(path);
        let parent = p.parent().unwrap_or(Path::new("."));
        let basename = p.file_name().unwrap_or_default().to_string_lossy();
        let graph_path = parent.join(format!("{}.hnsw.graph", basename));

        let mut idx = Self::with_params(
            persisted.dim,
            persisted.m,
            persisted.ef_construction,
            persisted.ef_search,
            persisted.max_elements.max(persisted.entries.len()),
        );

        if graph_path.exists() {
            tracing::info!("Found HNSW graph dump, loading instantly...");
            let io = Box::leak(Box::new(HnswIo::new(parent, &basename)));
            idx.hnsw = io.load_hnsw()?;
            idx.entries = persisted.entries;
            // Clear vectors from memory to save RAM, since they are inside the memory mapped graph
            for e in &mut idx.entries {
                e.vector.clear();
                e.vector.shrink_to_fit();
            }
        } else {
            tracing::info!("No HNSW graph dump found, rebuilding from entries...");
            for mut e in persisted.entries {
                let vec = std::mem::take(&mut e.vector);
                idx.insert(e.chunk_id.clone(), vec);
            }

            // Clear vectors from memory after insert
            for e in &mut idx.entries {
                e.vector.clear();
                e.vector.shrink_to_fit();
            }

            tracing::info!("Rebuild complete, saving fast dump...");
            let _ = idx.save_to_path(path);
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
    // hnsw_rs requires nb_layer = 16 to dump successfully
    let nb_layer = 16;

    Hnsw::<f32, DistCosine>::new(
        max_nb_connection,
        nb_elem,
        nb_layer,
        ef_construction.max(max_nb_connection),
        DistCosine {},
    )
}
