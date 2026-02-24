use rocksdb::{DB, IteratorMode};

use crate::{knowledge::wikipedia::WikiRecord, storage};

/// Very lightweight lexical panel matcher over stored wiki records.
pub fn build_panel(db: &DB, query: &str) -> Option<WikiRecord> {
    let wiki_cf = storage::cf(db, storage::CF_WIKI).ok()?;
    let q = query.to_ascii_lowercase();

    let mut best: Option<(i32, WikiRecord)> = None;

    for item in db.iterator_cf(wiki_cf, IteratorMode::Start) {
        let (_, value) = item.ok()?;
        let rec: WikiRecord = serde_json::from_slice(&value).ok()?;

        let mut score = 0;
        let title_l = rec.title.to_ascii_lowercase();
        let summary_l = rec.summary.to_ascii_lowercase();

        if title_l == q {
            score += 100;
        }
        if title_l.contains(&q) {
            score += 40;
        }
        if summary_l.contains(&q) {
            score += 10;
        }

        if score > 0 {
            match &best {
                Some((best_score, _)) if *best_score >= score => {}
                _ => best = Some((score, rec)),
            }
        }
    }

    best.map(|(_, rec)| rec)
}
