use search_engine::{
    config,
    embeddings::client,
    search::{bootstrap, query},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let top_k = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);
    let queries_arg = args.next().unwrap_or_else(|| {
        "what is a B-tree||tcp three-way handshake||rust lifetime elision rules".to_string()
    });
    let queries: Vec<String> = queries_arg
        .split("||")
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(ToString::to_string)
        .collect();

    let cfg = config::load()?;
    println!("[query_suite] {}", client::backend_info()?);
    let stack = bootstrap::load_search_stack()?;

    for query_text in queries {
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
            "\n=== query: {:?} hits={} elapsed_ms={} ===",
            query_text,
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
                .take(24)
                .collect::<Vec<_>>()
                .join(" ");

            println!(
                "{rank}. score={score:.3} vec={vec:.3} lex={lex:.3} title={title:.3} heading={heading_overlap:.3} body={body:.3} auth={auth:.3}\n   url={url}\n   heading={heading}\n   text={preview}",
                rank = idx + 1,
                score = result.final_score,
                vec = result.vector_score,
                lex = result.lexical_score,
                title = result.title_overlap,
                heading_overlap = result.heading_overlap,
                body = result.body_overlap,
                auth = result.authority_bonus,
                url = result.source_url,
            );
        }
    }

    Ok(())
}
