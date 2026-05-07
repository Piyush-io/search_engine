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
    /// Optional URL of a remote embed-server (e.g. http://192.168.1.100:3001).
    /// When set, all embedding inference is forwarded over HTTP instead of
    /// running a local ONNX session.
    #[serde(default)]
    pub remote_url: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Default)]
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

    // Lexical field boost configuration (for tantivy field weights)
    #[serde(default = "default_lexical_field_boost_title")]
    pub lexical_field_boost_title: f32,
    #[serde(default = "default_lexical_field_boost_section")]
    pub lexical_field_boost_section: f32,
    #[serde(default = "default_lexical_field_boost_heading")]
    pub lexical_field_boost_heading: f32,
    #[serde(default = "default_lexical_field_boost_text")]
    pub lexical_field_boost_text: f32,

    // Lexical query boost configuration
    #[serde(default = "default_lexical_short_query_phrase_boost")]
    pub lexical_short_query_phrase_boost: f32,

    // HNSW ef_search configuration per query class (adaptive search exploration)
    #[serde(default = "default_ef_search_short")]
    pub ef_search_short: usize,
    #[serde(default = "default_ef_search_long")]
    pub ef_search_long: usize,
    #[serde(default = "default_ef_search_identifier")]
    pub ef_search_identifier: usize,

    // Host policy configuration for niche corpus cleanup
    /// Multiplier for trusted (canonical) hosts. 1.0 = neutral, >1.0 boosts, <1.0 penalizes.
    /// Default: 1.0 (backward compatible)
    #[serde(default = "default_host_allowlist_boost")]
    pub host_allowlist_boost: f32,
    /// Score penalty multiplier for noisy/low-quality hosts. 1.0 = neutral, <1.0 reduces score.
    /// Default: 1.0 (backward compatible)
    #[serde(default = "default_host_soft_penalty")]
    pub host_soft_penalty: f32,
    /// Hard cap on results per host in top-k. None = unlimited (backward compatible).
    /// Recommended: 3-5 for niche profiles to prevent host dominance.
    #[serde(default = "default_host_hard_cap")]
    pub host_hard_cap: Option<usize>,
    /// List of canonical/trusted hosts to apply allowlist boost to.
    /// Default: empty (backward compatible, no special handling)
    #[serde(default = "default_host_allowlist")]
    pub host_allowlist: Vec<String>,
    /// List of noisy hosts to apply soft penalty to.
    /// Default: empty (backward compatible, no special handling)
    #[serde(default = "default_host_penalty_list")]
    pub host_penalty_list: Vec<String>,

    // Cross-encoder reranking configuration
    /// Enable cross-encoder reranking via remote embed_server. Default: false.
    #[serde(default = "default_rerank_enabled")]
    pub rerank_enabled: bool,
    /// Number of top heuristic candidates to send to reranker. Default: 20.
    #[serde(default = "default_rerank_pool_size")]
    pub rerank_pool_size: usize,
    /// Blend weight for reranker score (0.0 = pure heuristic, 1.0 = pure reranker). Default: 0.35.
    #[serde(default = "default_rerank_blend_weight")]
    pub rerank_blend_weight: f32,
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

// Lexical field boost defaults
fn default_lexical_field_boost_title() -> f32 {
    4.0
}
fn default_lexical_field_boost_section() -> f32 {
    3.0
}
fn default_lexical_field_boost_heading() -> f32 {
    2.5
}
fn default_lexical_field_boost_text() -> f32 {
    1.0
}
fn default_lexical_short_query_phrase_boost() -> f32 {
    2.5
}

// HNSW ef_search defaults per query class
fn default_ef_search_short() -> usize {
    120
}
fn default_ef_search_long() -> usize {
    80
}
fn default_ef_search_identifier() -> usize {
    150
}

// Host policy defaults (all neutral for backward compatibility)
fn default_host_allowlist_boost() -> f32 {
    1.0
}
fn default_host_soft_penalty() -> f32 {
    1.0
}
fn default_host_hard_cap() -> Option<usize> {
    None
}
fn default_host_allowlist() -> Vec<String> {
    Vec::new()
}
fn default_host_penalty_list() -> Vec<String> {
    Vec::new()
}

// Reranker defaults
fn default_rerank_enabled() -> bool {
    false
}
fn default_rerank_pool_size() -> usize {
    20
}
fn default_rerank_blend_weight() -> f32 {
    0.35
}

#[derive(Debug, Clone, Deserialize)]
pub struct HnswConfig {
    pub backend: String,
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

pub fn config_path() -> String {
    std::env::var("SEARCH_ENGINE_CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string())
}

pub fn load() -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = config_path();
    let text = std::fs::read_to_string(config_path)?;
    Ok(toml::from_str(&text)?)
}
