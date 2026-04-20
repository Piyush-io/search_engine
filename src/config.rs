use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub crawl: CrawlConfig,
    pub embedding: EmbeddingConfig,
    pub hnsw: HnswConfig,
    pub chunking: ChunkingConfig,
    pub ranking: RankingConfig,
    pub rocksdb: RocksDbConfig,
    pub server: ServerConfig,
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrawlConfig {
    pub max_pages: usize,
    pub concurrency: usize,
    pub rate_limit_ms: u64,
    #[serde(default = "default_recrawl_days")]
    pub recrawl_days: u64,
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
fn default_recrawl_days() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankingConfig {
    #[serde(default = "default_short_vec_weight")]
    pub short_vec_weight: f32,
    #[serde(default = "default_short_lex_weight")]
    pub short_lex_weight: f32,
    #[serde(default = "default_short_title_weight")]
    pub short_title_weight: f32,
    #[serde(default = "default_short_heading_weight")]
    pub short_heading_weight: f32,
    #[serde(default = "default_short_body_weight")]
    pub short_body_weight: f32,
    #[serde(default = "default_long_vec_weight")]
    pub long_vec_weight: f32,
    #[serde(default = "default_long_lex_weight")]
    pub long_lex_weight: f32,
    #[serde(default = "default_long_title_weight")]
    pub long_title_weight: f32,
    #[serde(default = "default_long_heading_weight")]
    pub long_heading_weight: f32,
    #[serde(default = "default_long_body_weight")]
    pub long_body_weight: f32,
    #[serde(default = "default_exact_heading_boost")]
    pub exact_heading_boost: f32,
    #[serde(default = "default_exact_body_boost")]
    pub exact_body_boost: f32,
    #[serde(default = "default_no_heading_penalty")]
    pub no_heading_penalty: f32,
    #[serde(default = "default_weak_heading_penalty")]
    pub weak_heading_penalty: f32,
    #[serde(default = "default_authority_bonus")]
    pub authority_bonus: f32,
    #[serde(default = "default_rrf_k")]
    pub rrf_k: f32,
    #[serde(default = "default_short_rrf_vec_weight")]
    pub short_rrf_vec_weight: f32,
    #[serde(default = "default_short_rrf_lex_weight")]
    pub short_rrf_lex_weight: f32,
    #[serde(default = "default_long_rrf_vec_weight")]
    pub long_rrf_vec_weight: f32,
    #[serde(default = "default_long_rrf_lex_weight")]
    pub long_rrf_lex_weight: f32,
    #[serde(default = "default_score_floor_fraction")]
    pub score_floor_fraction: f32,
    #[serde(default = "default_score_floor_min")]
    pub score_floor_min: f32,
    #[serde(default = "default_host_cap_divisor")]
    pub host_cap_divisor: usize,
    #[serde(default = "default_host_cap_min")]
    pub host_cap_min: usize,
    #[serde(default = "default_host_cap_max")]
    pub host_cap_max: usize,
    #[serde(default = "default_authority_min_lexical_score")]
    pub authority_min_lexical_score: f32,
    #[serde(default = "default_authority_min_structural_overlap")]
    pub authority_min_structural_overlap: f32,
    #[serde(default = "default_lexical_only_fallback_enabled")]
    pub lexical_only_fallback_enabled: bool,
    #[serde(default = "default_lexical_only_pool_mult")]
    pub lexical_only_pool_mult: usize,
    #[serde(default = "default_lexical_only_pool_cap")]
    pub lexical_only_pool_cap: usize,
    #[serde(default = "default_lexical_relaxed_fallback_enabled")]
    pub lexical_relaxed_fallback_enabled: bool,
    #[serde(default = "default_lexical_relaxed_min_hits")]
    pub lexical_relaxed_min_hits: usize,
    #[serde(default = "default_lexical_relaxed_extra_k")]
    pub lexical_relaxed_extra_k: usize,
}

fn default_short_vec_weight() -> f32 {
    0.25
}
fn default_short_lex_weight() -> f32 {
    0.35
}
fn default_short_title_weight() -> f32 {
    0.22
}
fn default_short_heading_weight() -> f32 {
    0.12
}
fn default_short_body_weight() -> f32 {
    0.06
}
fn default_long_vec_weight() -> f32 {
    0.35
}
fn default_long_lex_weight() -> f32 {
    0.20
}
fn default_long_title_weight() -> f32 {
    0.20
}
fn default_long_heading_weight() -> f32 {
    0.15
}
fn default_long_body_weight() -> f32 {
    0.10
}
fn default_exact_heading_boost() -> f32 {
    0.25
}
fn default_exact_body_boost() -> f32 {
    0.10
}
fn default_no_heading_penalty() -> f32 {
    0.55
}
fn default_weak_heading_penalty() -> f32 {
    0.78
}
fn default_authority_bonus() -> f32 {
    0.08
}
fn default_rrf_k() -> f32 {
    60.0
}
fn default_short_rrf_vec_weight() -> f32 {
    0.6
}
fn default_short_rrf_lex_weight() -> f32 {
    1.8
}
fn default_long_rrf_vec_weight() -> f32 {
    1.0
}
fn default_long_rrf_lex_weight() -> f32 {
    1.0
}
fn default_score_floor_fraction() -> f32 {
    0.15
}
fn default_score_floor_min() -> f32 {
    0.12
}
fn default_host_cap_divisor() -> usize {
    4
}
fn default_host_cap_min() -> usize {
    2
}
fn default_host_cap_max() -> usize {
    3
}
fn default_authority_min_lexical_score() -> f32 {
    0.15
}
fn default_authority_min_structural_overlap() -> f32 {
    0.20
}
fn default_lexical_only_fallback_enabled() -> bool {
    true
}
fn default_lexical_only_pool_mult() -> usize {
    20
}
fn default_lexical_only_pool_cap() -> usize {
    1_000
}
fn default_lexical_relaxed_fallback_enabled() -> bool {
    true
}
fn default_lexical_relaxed_min_hits() -> usize {
    4
}
fn default_lexical_relaxed_extra_k() -> usize {
    200
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
    #[serde(default = "default_vector_delta_path")]
    pub vector_delta_path: String,
    #[serde(default = "default_seeds_path")]
    pub seeds_path: String,
}

fn default_vector_delta_path() -> String {
    "./hnsw_delta.bin".to_string()
}

fn default_seeds_path() -> String {
    "./seeds.md".to_string()
}

pub fn load() -> Result<Config, Box<dyn std::error::Error>> {
    let config_path =
        std::env::var("SEARCH_ENGINE_CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let text = std::fs::read_to_string(config_path)?;
    Ok(toml::from_str(&text)?)
}
