use search_engine::{
    config,
    embeddings::client,
    search::{bootstrap, query},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let query_text = args
        .next()
        .unwrap_or_else(|| "what is a B-tree".to_string());
    let top_k = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);

    let cfg = config::load()?;
    println!("[sample_query] {}", client::backend_info()?);

    let stack = bootstrap::load_search_stack()?;

    let started = std::time::Instant::now();
    let results = query::run_query(
        &stack.db,
        stack.index.as_ref(),
        stack.lexical.as_deref(),
        &query_text,
        top_k,
        &cfg.ranking,
    );
    println!(
        "[sample_query] query={query_text:?} hits={} elapsed_ms={}",
        results.len(),
        started.elapsed().as_millis()
    );

    for (idx, result) in results.iter().enumerate() {
        let heading = if result.heading_chain.is_empty() {
            "-".to_string()
        } else {
            result.heading_chain.join(" > ")
        };
        let preview = result
            .text
            .split_whitespace()
            .take(32)
            .collect::<Vec<_>>()
            .join(" ");

        println!(
            "{rank}. score={score:.3} url={url}\n   heading={heading}\n   text={preview}\n   vec_score={vec:.3} lex_score={lex:.3} title_overlap={title:.3} heading_overlap={head:.3} body_overlap={body:.3} auth_bonus={auth:.3}",
            rank = idx + 1,
            score = result.final_score,
            url = result.source_url,
            vec = result.vector_score,
            lex = result.lexical_score,
            title = result.title_overlap,
            head = result.heading_overlap,
            body = result.body_overlap,
            auth = result.authority_bonus,
        );
    }

    if results.is_empty() {
        return Err("sample query returned no results".into());
    }

    Ok(())
}
