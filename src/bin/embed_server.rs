use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use search_engine::{
    config,
    embeddings::{
        EmbedRequest, EmbedResponse, RerankRequest, RerankResponse,
        server::{EmbedServer, RerankerServer},
    },
};

struct AppState {
    embed: Arc<EmbedServer>,
    rerank: Arc<RerankerServer>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    println!("[embed-server] config={}", config::config_path());
    println!("[embed-server] loading embedding model on accelerator...");

    let embed = Arc::new(EmbedServer::new(
        &cfg.embedding.model,
        cfg.embedding.dim,
        cfg.embedding.max_length.unwrap_or(512),
    )?);

    println!(
        "[embed-server] embed: {} dim={} max_length={} ready",
        embed.model_name,
        embed.dim,
        cfg.embedding.max_length.unwrap_or(512)
    );

    let rerank_model =
        std::env::var("RERANKER_MODEL").unwrap_or_else(|_| "bge-reranker-base".to_string());
    let rerank_max_len = std::env::var("RERANKER_MAX_LENGTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);

    println!(
        "[embed-server] loading reranker model '{}'...",
        rerank_model
    );
    let rerank = Arc::new(RerankerServer::new(&rerank_model, rerank_max_len)?);
    println!("[embed-server] rerank: {} ready", rerank.model_name);

    let state = Arc::new(AppState { embed, rerank });

    let app = Router::new()
        .route("/embed", post(embed_handler))
        .route("/rerank", post(rerank_handler))
        .route("/health", get(health_handler))
        .with_state(state);

    let port = std::env::var("EMBED_SERVER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[embed-server] listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn embed_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, String)> {
    let vectors = state
        .embed
        .embed(req.texts, req.is_query)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(EmbedResponse {
        vectors,
        dim: state.embed.dim,
        model: state.embed.model_name.clone(),
    }))
}

async fn rerank_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RerankRequest>,
) -> Result<Json<RerankResponse>, (StatusCode, String)> {
    let scores = state
        .rerank
        .rerank(&req.query, &req.documents)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let top_k = req.top_k.min(scores.len());
    let mut indexed: Vec<(usize, f32)> = scores.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| b.1.total_cmp(&a.1));
    indexed.truncate(top_k);

    Ok(Json(RerankResponse {
        scores: indexed.iter().map(|(_, s)| *s).collect(),
        indices: indexed.iter().map(|(i, _)| *i).collect(),
    }))
}

async fn health_handler() -> &'static str {
    "ok"
}
