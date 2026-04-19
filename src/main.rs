use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use search_engine::{
    config,
    embeddings::client,
    knowledge::panel,
    search::{bootstrap, lexical::LexicalIndex, query, vector_index::VectorIndex},
    storage,
    web::{debug, serp, tracking},
};

#[derive(Clone)]
struct AppState {
    db: Arc<rocksdb::DB>,
    index: Arc<dyn VectorIndex>,
    lexical: Option<Arc<LexicalIndex>>,
    ranking: Arc<config::RankingConfig>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DebugSearchParams {
    q: Option<String>,
    k: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DebugEvalParams {
    qrels: Option<String>,
    queries: Option<String>,
    k: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClickParams {
    d: String,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    println!("[server] {}", client::backend_info()?);
    let stack = bootstrap::load_search_stack()?;
    let ranking = Arc::new(cfg.ranking.clone());

    let state = AppState {
        db: stack.db,
        index: stack.index,
        lexical: stack.lexical,
        ranking,
    };

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/search", get(search_handler))
        .route("/debug/search", get(debug_handler))
        .route("/debug/api/search", get(debug_api_search_handler))
        .route("/debug/api/eval", get(debug_api_eval_handler))
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

    let db = state.db.clone();
    let index = state.index.clone();
    let lexical = state.lexical.clone();
    let ranking = state.ranking.clone();
    let query_text_clone = query_text.clone();
    let results = tokio::task::spawn_blocking(move || {
        query::run_query(
            &db,
            index.as_ref(),
            lexical.as_deref(),
            &query_text_clone,
            10,
            &ranking,
        )
    })
    .await
    .unwrap_or_default();

    let panel = panel::build_panel(&state.db, &query_text);
    let elapsed_ms = t0.elapsed().as_millis();

    let search_results: Vec<_> = results.iter().map(|r| r.to_search_result()).collect();
    Html(serp::render_results_page(
        &query_text,
        &search_results,
        panel.as_ref(),
        elapsed_ms,
    ))
}

async fn debug_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let query_text = params.q.unwrap_or_default();

    if query_text.trim().is_empty() {
        return Html(serp::render_home_page());
    }

    let t0 = std::time::Instant::now();

    let db = state.db.clone();
    let index = state.index.clone();
    let lexical = state.lexical.clone();
    let ranking = state.ranking.clone();
    let query_text_clone = query_text.clone();
    let results = tokio::task::spawn_blocking(move || {
        query::run_query(
            &db,
            index.as_ref(),
            lexical.as_deref(),
            &query_text_clone,
            10,
            &ranking,
        )
    })
    .await
    .unwrap_or_default();

    let elapsed_ms = t0.elapsed().as_millis();

    Html(debug::render_debug_page(&query_text, &results, elapsed_ms))
}

async fn debug_api_search_handler(
    State(state): State<AppState>,
    Query(params): Query<DebugSearchParams>,
) -> impl IntoResponse {
    let query_text = params.q.unwrap_or_default();
    let top_k = params.k.unwrap_or(10).clamp(1, 25);

    if query_text.trim().is_empty() {
        return Json(debug::build_debug_search_response(
            "",
            &[],
            0,
            state.ranking.as_ref(),
        ));
    }

    let t0 = std::time::Instant::now();

    let db = state.db.clone();
    let index = state.index.clone();
    let lexical = state.lexical.clone();
    let ranking = state.ranking.clone();
    let query_text_clone = query_text.clone();
    let results = tokio::task::spawn_blocking(move || {
        query::run_query(
            &db,
            index.as_ref(),
            lexical.as_deref(),
            &query_text_clone,
            top_k,
            &ranking,
        )
    })
    .await
    .unwrap_or_default();

    Json(debug::build_debug_search_response(
        &query_text,
        &results,
        t0.elapsed().as_millis(),
        state.ranking.as_ref(),
    ))
}

async fn debug_api_eval_handler(
    State(state): State<AppState>,
    Query(params): Query<DebugEvalParams>,
) -> Result<Json<debug::DebugEvalResponse>, (StatusCode, Json<ApiError>)> {
    let qrels_path = params.qrels.unwrap_or_default();
    let queries_path = params.queries.unwrap_or_default();

    if qrels_path.trim().is_empty() || queries_path.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Both qrels and queries parameters are required".to_string(),
            }),
        ));
    }

    let k_values = debug::parse_k_values(params.k.as_deref());

    let db = state.db.clone();
    let index = state.index.clone();
    let lexical = state.lexical.clone();
    let ranking = state.ranking.clone();

    let report = tokio::task::spawn_blocking(move || {
        debug::run_debug_evaluation(
            &db,
            index.as_ref(),
            lexical.as_deref(),
            &ranking,
            &qrels_path,
            &queries_path,
            &k_values,
        )
    })
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("evaluation task failed: {err}"),
            }),
        )
    })?
    .map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: err.to_string(),
            }),
        )
    })?;

    Ok(Json(report))
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
