use std::{collections::HashMap, io, sync::OnceLock};

use rocksdb::{DB, IteratorMode};

use crate::{
    config, embeddings::client, knowledge::wikipedia::WikiRecord, search::hnsw::HnswIndex, storage,
};

struct WikiPanelState {
    index: HnswIndex,
    records: HashMap<String, WikiRecord>,
}

static WIKI_PANEL_STATE: OnceLock<Result<WikiPanelState, String>> = OnceLock::new();

fn load_panel_state(db: &DB) -> Result<WikiPanelState, Box<dyn std::error::Error>> {
    let cfg = config::load()?;

    let index = HnswIndex::load_from_path(&cfg.paths.wiki_index_path)?;

    let wiki_cf = storage::cf(db, storage::CF_WIKI)?;
    let mut records = HashMap::new();
    for item in db.iterator_cf(wiki_cf, IteratorMode::Start) {
        let (_, value) = item?;
        let rec: WikiRecord = serde_json::from_slice(&value)?;
        records.insert(rec.title.to_ascii_lowercase(), rec);
    }

    Ok(WikiPanelState { index, records })
}

fn state(db: &DB) -> Result<&'static WikiPanelState, Box<dyn std::error::Error>> {
    let res = WIKI_PANEL_STATE
        .get_or_init(|| load_panel_state(db).map_err(|e| format!("wiki panel init failed: {e}")));

    match res {
        Ok(s) => Ok(s),
        Err(msg) => Err(io::Error::other(msg.clone()).into()),
    }
}

/// ANN-backed wiki panel matcher.
pub fn build_panel(db: &DB, query: &str) -> Option<WikiRecord> {
    let query_vec = client::embed_query(query).ok()?;
    let st = state(db).ok()?;

    let hits = st.index.search(&query_vec, 3);
    for (wiki_key, score) in hits {
        if score < 0.35 {
            continue;
        }

        if let Some(rec) = st.records.get(&wiki_key.to_ascii_lowercase()) {
            return Some(rec.clone());
        }
    }

    None
}
