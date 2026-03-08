use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use search_engine::{
    config,
    embeddings::client,
    knowledge::panel,
    search::{
        bruteforce::BruteForceIndex, hnsw::HnswIndex, lexical::LexicalIndex, query,
        vector_index::VectorIndex,
    },
    storage,
    web::{serp, tracking},
};

#[derive(Clone)]
struct AppState {
    db: Arc<rocksdb::DB>,
    index: Arc<dyn VectorIndex>,
    lexical: Option<Arc<LexicalIndex>>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClickParams {
    d: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    println!("[server] {}", client::backend_info()?);
    let db = Arc::new(storage::open_db(&cfg.paths.db_path)?);

    let index_backend = cfg.hnsw.backend.to_ascii_lowercase();
    println!("[server] loading vector index from {} (this may take a minute)...", cfg.paths.index_path);
    let t0 = std::time::Instant::now();
    let index: Arc<dyn VectorIndex> = if index_backend == "bruteforce" {
        let idx = match BruteForceIndex::load_from_path(&cfg.paths.index_path) {
            Ok(idx) => idx,
            Err(_) => BruteForceIndex::new(cfg.embedding.dim),
        };
        Arc::new(idx)
    } else {
        let idx = match HnswIndex::load_from_path(&cfg.paths.index_path) {
            Ok(idx) => idx,
            Err(_) => HnswIndex::with_params(
                cfg.embedding.dim,
                cfg.hnsw.m,
                cfg.hnsw.ef_construction,
                cfg.hnsw.ef_search,
                cfg.hnsw.max_elements,
            ),
        };
        Arc::new(idx)
    };

    println!(
        "[server] vector backend={} entries={} loaded in {:.1}s",
        index_backend,
        index.len(),
        t0.elapsed().as_secs_f64(),    );

    let lexical = LexicalIndex::open(&cfg.paths.lexical_index_path)
        .ok()
        .map(Arc::new);

    let state = AppState { db, index, lexical };

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/search", get(search_handler))
        .route("/act", get(act_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cfg.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("[server] listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn home_handler() -> Html<String> {
    Html(serp::render_home_page())
}

async fn search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let query_text = params.q.unwrap_or_default();

    if query_text.trim().is_empty() {
        return Html(serp::render_home_page());
    }

    let t0 = std::time::Instant::now();

    let results = query::run_query(
        &state.db,
        state.index.as_ref(),
        state.lexical.as_deref(),
        &query_text,
        10,
    );
    let panel = panel::build_panel(&state.db, &query_text);

    let elapsed_ms = t0.elapsed().as_millis();

    Html(serp::render_results_page(
        &query_text,
        &results,
        panel.as_ref(),
        elapsed_ms,
    ))
}

async fn act_handler(
    State(state): State<AppState>,
    Query(params): Query<ClickParams>,
) -> impl IntoResponse {
    if let Some(payload) = tracking::decode_click_payload(&params.d) {
        let clicks_cf = match storage::cf(&state.db, storage::CF_CLICKS) {
            Ok(cf) => cf,
            Err(_) => return Redirect::temporary(&payload.target_url),
        };

        let key = click_key(&payload.query, payload.position, &payload.target_url);
        let target_url = payload.target_url.clone();

        let value = json!({
            "query": payload.query,
            "position": payload.position,
            "target_url": target_url,
            "timestamp_ms": payload.timestamp_ms,
        });

        let _ = state
            .db
            .put_cf(clicks_cf, key.as_bytes(), value.to_string().as_bytes());

        return Redirect::temporary(&payload.target_url);
    }

    Redirect::temporary("/")
}

fn click_key(query: &str, position: usize, target: &str) -> String {
    let mut h = Sha256::new();
    h.update(query.as_bytes());
    h.update(position.to_string().as_bytes());
    h.update(target.as_bytes());
    h.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    format!("{:x}", h.finalize())
}
