use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub crawl: CrawlConfig,
    pub embedding: EmbeddingConfig,
    pub hnsw: HnswConfig,
    pub chunking: ChunkingConfig,
    pub rocksdb: RocksDbConfig,
    pub server: ServerConfig,
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrawlConfig {
    pub max_pages: usize,
    pub concurrency: usize,
    pub rate_limit_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    pub backend: String,
    pub model: String,
    pub dim: usize,
    pub batch_size: usize,
    pub max_length: Option<usize>,
    /// Number of parallel ORT sessions used during bulk embedding (embed binary only).
    /// Default: 2. Each session uses `bulk_intra_threads` intra-op threads.
    #[serde(default = "default_bulk_workers")]
    pub bulk_workers: usize,
    /// Intra-op thread count per bulk session. Default: 4.
    #[serde(default = "default_bulk_intra_threads")]
    pub bulk_intra_threads: usize,
}

fn default_bulk_workers() -> usize {
    2
}
fn default_bulk_intra_threads() -> usize {
    4
}

fn default_window_size() -> usize {
    3
}
fn default_window_overlap() -> usize {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct HnswConfig {
    pub backend: String,
    pub shards: usize,
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    pub max_elements: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkingConfig {
    pub context_depth: usize,
    #[serde(default = "default_window_size")]
    pub window_size: usize,
    #[serde(default = "default_window_overlap")]
    pub window_overlap: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RocksDbConfig {
    pub block_cache_mb: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    pub db_path: String,
    pub index_path: String,
    pub lexical_index_path: String,
    pub wiki_index_path: String,
}

pub fn load() -> Result<Config, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string("config.toml")?;
    Ok(toml::from_str(&text)?)
}
