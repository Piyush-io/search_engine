/// link_harvest — community-curated link discovery pre-pass
///
/// Queries HN Algolia, lobste.rs JSON API, and tildes.net to extract URLs of
/// high-scoring articles, then enqueues them into the RocksDB crawl frontier.
/// Run this BEFORE `crawl` to seed the frontier with community-vetted content.
///
/// Usage:
///   cargo run --release --bin link_harvest
///
/// Sources:
///   HN Algolia  — stories with score >= HN_MIN_SCORE, paginated via date windows
///   lobste.rs   — /hottest.json, /newest.json (score >= LOBSTERS_MIN_SCORE)
///   tildes.net  — ~comp, ~programming, ~science (robots.txt explicitly allows crawlers)
use std::time::Duration;

use reqwest::Client;
use rocksdb::WriteBatch;
use search_engine::{config, crawler::canon, storage};
use url::Url;

// ── Thresholds ────────────────────────────────────────────────────────────────
const HN_MIN_SCORE: u32 = 100;
const HN_MAX_PAGES: usize = 500; // 1000 hits/page × 500 pages = 500k stories max
const LOBSTERS_MIN_SCORE: i64 = 10;
const LOBSTERS_MAX_PAGES: usize = 200;

// ── Domain allowlist for harvested links ─────────────────────────────────────
// We only enqueue URLs from domains that are either:
//   a) already in the crawl allowlist (crawl.rs will accept them), OR
//   b) in the extended harvest allowlist below (new domains discovered via HN/lobste.rs)
//
// This prevents harvesting links to paywalled/noisy sites while still getting
// value from community curation for domains we DO want to crawl.
fn harvest_domain_allowed(host: &str) -> bool {
    // Strip www. prefix for matching
    let h = host.strip_prefix("www.").unwrap_or(host);

    // Domains already in crawl.rs allowlist (abbreviated — crawl.rs does the full check)
    const EXISTING: &[&str] = &[
        "doc.rust-lang.org",
        "docs.python.org",
        "developer.mozilla.org",
        "en.wikipedia.org",
        "stackoverflow.com",
        "stackoverflow.blog",
        "cppreference.com",
        "en.cppreference.com",
        "blog.rust-lang.org",
        "blog.cloudflare.com",
        "jvns.ca",
        "martinfowler.com",
        "blog.acolyer.org",
        "eng.uber.com",
        "netflixtechblog.com",
        "research.google",
        "docs.rs",
        "go.dev",
        "nodejs.org",
        "kubernetes.io",
        "docs.docker.com",
        "learn.microsoft.com",
        "docs.julialang.org",
        "dev.to",
        "medium.com",
        "infoq.com",
        "highscalability.com",
        "engineering.fb.com",
        "aws.amazon.com",
        "arxiv.org",
        "paperswithcode.com",
        "distill.pub",
        "cacm.acm.org",
        "cs.stanford.edu",
        "ocw.mit.edu",
        "ask.ubuntu.com",
        "unix.stackexchange.com",
        "docs.swift.org",
        "kotlinlang.org",
        "docs.scala-lang.org",
        "elixir-lang.org",
        "hexdocs.pm",
        "wiki.haskell.org",
        "ziglang.org",
        "docs.oracle.com",
        "slack.engineering",
        "dropbox.tech",
        "databricks.com",
        "grafana.com",
        "elastic.co",
        "shopify.engineering",
        "engineering.linkedin.com",
        "microservices.io",
        "sre.google",
        "owasp.org",
        "portswigger.net",
        "schneier.com",
        "terraform.io",
        "docs.ansible.com",
        "prometheus.io",
        "opentelemetry.io",
        "kafka.apache.org",
        "redis.io",
        "postgresql.org",
        "sqlite.org",
        "thenewstack.io",
        "systemdesign.one",
        "simonwillison.net",
        "danluu.com",
        "matklad.github.io",
        "without.boats",
        "joelonsoftware.com",
        "rachelbythebay.com",
        "oreilly.com",
        // new seeds from seeds.md (crawl.rs will be updated to accept these)
        "cp-algorithms.com",
        "cstheory.stackexchange.com",
        "norvig.com",
        "kernel.org",
        "brendangregg.com",
        "pages.cs.wisc.edu",
        "pdos.csail.mit.edu",
        "os.phil-opp.com",
        "linuxfromscratch.org",
        "lwn.net",
        "agner.org",
        "uops.info",
        "sandpile.org",
        "book.rvemu.app",
        "llvm.org",
        "clang.llvm.org",
        "gcc.gnu.org",
        "craftinginterpreters.com",
        "eli.thegreenplace.net",
        "cs.cornell.edu",
        "swtch.com",
        "softwarefoundations.cis.upenn.edu",
        "cs.cmu.edu",
        "homotopytypetheory.org",
        "xavierleroy.org",
        "faultlore.com",
        "scattered-thoughts.net",
        "interdb.jp",
        "15445.courses.cs.cmu.edu",
        "15721.courses.cs.cmu.edu",
        "cockroachlabs.com",
        "jepsen.io",
        "martin.kleppmann.com",
        "muratbuffalo.blogspot.com",
        "fly.io",
        "explained.ai",
        "cs231n.github.io",
        "nlp.stanford.edu",
        "learnopengl.com",
        "vulkan-tutorial.com",
        "raytracing.github.io",
        "beej.us",
        "hacks.mozilla.org",
        "webassembly.org",
        "cryptopals.com",
        "blog.cryptographyengineering.com",
        "usenix.org",
        "isocpp.org",
        "abseil.io",
        "paulgraham.com",
        "thume.ca",
        "lamport.azurewebsites.net",
        "coq.inria.fr",
        "cl.cam.ac.uk",
        "interrupt.memfault.com",
        "quantum.country",
        "missing.csail.mit.edu",
        "staffeng.com",
        "nand2tetris.org",
        // Popular high-signal domains commonly linked from HN/lobste.rs
        "apenwarr.ca",
        "aphyr.com",
        "acm.org",
        "bitbashing.io",
        "blog.golang.org",
        "blog.regehr.org",
        "blog.jessfraz.com",
        "borretti.me",
        "browserengineering.book",
        "cacm.acm.org",
        "codewithoutrules.com",
        "commandcenter.blogspot.com",
        "corecursive.com",
        "cs.nyu.edu",
        "cppstories.com",
        "fasterthanli.me",
        "ferrous-systems.com",
        "fgiesen.wordpress.com",
        "gamasutra.com",
        "gankra.github.io",
        "github.com",
        "golangweekly.com",
        "gregoryszorc.com",
        "hn.algolia.com",
        "hugotunius.se",
        "jakewharton.com",
        "jameshfisher.com",
        "jasonlaster.github.io",
        "jeremykun.com",
        "jhjourdan.mketjh.fr",
        "johnnysswlab.com",
        "justine.lol",
        "kristerw.blogspot.com",
        "lobste.rs",
        "log.annulled.net",
        "luca.ntop.org",
        "lxr.sourceware.org",
        "maskray.me",
        "mcyoung.xyz",
        "mort.coffee",
        "mrale.ph",
        "mulle-kybernetik.com",
        "nee.lv",
        "neugierig.org",
        "nickdesaulniers.github.io",
        "nullprogram.com",
        "okmij.org",
        "overreacted.io",
        "oxfeeefeee.github.io",
        "pling.app",
        "preshing.com",
        "pvk.ca",
        "ralfj.de",
        "ridiculousfish.com",
        "robert.ocallahan.org",
        "rxweb.io",
        "smallcultfollowing.com",
        "snoyman.com",
        "sunfishcode.online",
        "swlaschin.gitbooks.io",
        "tenderlovemaking.com",
        "thecodelesscode.com",
        "thorstenball.com",
        "timharris.uk",
        "timlrx.com",
        "tollef.no",
        "tonsky.me",
        "tratt.net",
        "univalent.foundations",
        "verdagon.dev",
        "wingolog.org",
        "words.filippo.io",
        "wordsandbuttons.online",
        "write.as",
        "www.aosabook.org",
        "www.cs.virginia.edu",
        "www.cs.yale.edu",
        "www.destroyallsoftware.com",
        "www.hillelwayne.com",
        "www.hyrumslaw.com",
        "www.pathsensitive.com",
        "www.pvk.ca",
        "www.tedinski.com",
        "www.theregister.com",
        "www.tweag.io",
        "www.usenix.org",
        "zig.news",
        "ziglang.org",
        "zserge.com",
    ];

    // Check exact match or subdomain match
    for allowed in EXISTING {
        if h == *allowed || h.ends_with(&format!(".{allowed}")) {
            return true;
        }
    }
    false
}

// ── Path filters ──────────────────────────────────────────────────────────────
fn path_looks_binary(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [
        ".pdf", ".png", ".jpg", ".jpeg", ".gif", ".zip", ".tar", ".gz", ".mp4", ".mp3", ".woff",
        ".woff2", ".ttf", ".svg",
    ]
    .iter()
    .any(|ext| p.ends_with(ext))
}

fn url_worth_harvesting(url_str: &str) -> bool {
    let Ok(u) = Url::parse(url_str) else {
        return false;
    };
    let Some(host) = u.host_str() else {
        return false;
    };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    if path_looks_binary(u.path()) {
        return false;
    }
    harvest_domain_allowed(host)
}

// ── RocksDB helpers ───────────────────────────────────────────────────────────
fn enqueue_urls(
    db: &rocksdb::DB,
    urls: &[String],
    enqueued: &mut usize,
    skipped: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let seen_cf = storage::cf(db, storage::CF_SEEN)?;
    let mut wb = WriteBatch::default();

    for url_str in urls {
        let Some(canon) = canon::canonicalize(url_str) else {
            *skipped += 1;
            continue;
        };
        let bytes = canon.as_str().as_bytes();
        // Skip if already seen
        if db.key_may_exist_cf(seen_cf, bytes) && db.get_cf(seen_cf, bytes)?.is_some() {
            *skipped += 1;
            continue;
        }
        // Skip if already queued
        if db.key_may_exist_cf(to_crawl_cf, bytes) && db.get_cf(to_crawl_cf, bytes)?.is_some() {
            *skipped += 1;
            continue;
        }
        wb.put_cf(to_crawl_cf, bytes, []);
        *enqueued += 1;
    }

    if *enqueued > 0 {
        db.write(wb)?;
    }
    Ok(())
}

// ── HN Algolia harvest ────────────────────────────────────────────────────────
async fn harvest_hn(client: Client) -> Vec<String> {
    println!("[hn] harvesting HN Algolia stories (score >= {HN_MIN_SCORE})…");

    let mut all_urls: Vec<String> = Vec::new();
    let mut page = 0usize;
    let mut cursor_before: Option<u64> = None;
    let mut total_fetched = 0usize;

    'outer: loop {
        if page >= HN_MAX_PAGES {
            break;
        }

        let numeric_filters = if let Some(ts) = cursor_before {
            format!("points>{HN_MIN_SCORE},created_at_i<{ts}")
        } else {
            format!("points>{HN_MIN_SCORE}")
        };

        let api_url = format!(
            "https://hn.algolia.com/api/v1/search_by_date?tags=story&numericFilters={}&hitsPerPage=1000&attributesToRetrieve=url,title,points,created_at_i",
            urlencoding::encode(&numeric_filters)
        );

        let resp = match client.get(&api_url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                eprintln!("[hn] API error: {}", r.status());
                break;
            }
            Err(e) => {
                eprintln!("[hn] request error: {e}");
                break;
            }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[hn] json error: {e}");
                break;
            }
        };
        let hits = match body["hits"].as_array() {
            Some(a) => a.clone(),
            None => break,
        };

        if hits.is_empty() {
            break;
        }

        let mut batch_count = 0usize;
        let mut min_ts: Option<u64> = None;

        for hit in &hits {
            let url = hit["url"].as_str().unwrap_or("").to_string();
            let ts = hit["created_at_i"].as_u64().unwrap_or(0);

            if !url.is_empty() && url_worth_harvesting(&url) {
                all_urls.push(url);
                batch_count += 1;
            }

            if ts > 0 {
                min_ts = Some(match min_ts {
                    None => ts,
                    Some(m) => m.min(ts),
                });
            }
        }

        total_fetched += hits.len();
        println!(
            "[hn] page={page} fetched={} accepted_batch={batch_count} total_accepted={} total_fetched={total_fetched}",
            hits.len(),
            all_urls.len()
        );

        match min_ts {
            Some(ts) if ts > 0 => cursor_before = Some(ts),
            _ => break,
        }
        page += 1;

        tokio::time::sleep(Duration::from_millis(250)).await;

        if hits.len() < 1000 {
            break 'outer;
        }
    }

    println!(
        "[hn] done. total_fetched={total_fetched} accepted={}",
        all_urls.len()
    );
    all_urls
}

// ── lobste.rs harvest ─────────────────────────────────────────────────────────
async fn harvest_lobsters(client: Client) -> Vec<String> {
    println!("[lobste.rs] harvesting lobste.rs hottest + newest pages…");

    let mut all_urls: Vec<String> = Vec::new();

    let endpoints = [
        ("hottest", HN_MAX_PAGES.min(LOBSTERS_MAX_PAGES)),
        ("newest", LOBSTERS_MAX_PAGES),
    ];

    for (kind, max_pages) in endpoints {
        let mut page = 1usize;
        loop {
            if page > max_pages {
                break;
            }

            let url = format!("https://lobste.rs/{kind}.json?page={page}");
            let resp = match client.get(&url).send().await {
                Ok(r) if r.status().is_success() => r,
                Ok(r) => {
                    eprintln!("[lobste.rs] {url} → {}", r.status());
                    break;
                }
                Err(e) => {
                    eprintln!("[lobste.rs] {url} → {e}");
                    break;
                }
            };

            let items: Vec<serde_json::Value> = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[lobste.rs] json parse error: {e}");
                    break;
                }
            };

            if items.is_empty() {
                break;
            }

            let mut batch_count = 0usize;
            for item in &items {
                let score = item["score"].as_i64().unwrap_or(0);
                if score < LOBSTERS_MIN_SCORE {
                    continue;
                }
                let url = item["url"].as_str().unwrap_or("").to_string();
                if !url.is_empty() && url_worth_harvesting(&url) {
                    all_urls.push(url);
                    batch_count += 1;
                }
            }

            println!(
                "[lobste.rs] {kind} page={page} items={} accepted_batch={batch_count} total={}",
                items.len(),
                all_urls.len()
            );

            if items.len() < 25 {
                break;
            }
            page += 1;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    println!("[lobste.rs] done. accepted={}", all_urls.len());
    all_urls
}

// ── tildes.net harvest ────────────────────────────────────────────────────────
async fn harvest_tildes(client: Client) -> Vec<String> {
    println!("[tildes] harvesting tildes.net groups…");

    let mut all_urls: Vec<String> = Vec::new();
    let groups = [
        "~comp",
        "~programming",
        "~science",
        "~tech",
        "~devops",
        "~math",
    ];
    let re = regex::Regex::new(r#"href="(https?://[^"]+)""#).unwrap();

    for group in groups {
        for page in 1..=20usize {
            let url = if page == 1 {
                format!("https://tildes.net/{group}?order=votes&period=all")
            } else {
                format!("https://tildes.net/{group}?order=votes&period=all&page={page}")
            };

            let resp = match client.get(&url).header("Accept", "text/html").send().await {
                Ok(r) if r.status().is_success() => r,
                Ok(r) => {
                    eprintln!("[tildes] {url} → {}", r.status());
                    break;
                }
                Err(e) => {
                    eprintln!("[tildes] {url} → {e}");
                    break;
                }
            };

            let body = resp.text().await.unwrap_or_default();

            let mut batch_count = 0usize;
            for cap in re.captures_iter(&body) {
                let url_str = &cap[1];
                if !url_str.contains("tildes.net") && url_worth_harvesting(url_str) {
                    all_urls.push(url_str.to_string());
                    batch_count += 1;
                }
            }

            println!(
                "[tildes] {group} page={page} accepted_batch={batch_count} total={}",
                all_urls.len()
            );

            if body.contains("No topics") || batch_count == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    println!("[tildes] done. accepted={}", all_urls.len());
    all_urls
}

// ── main ──────────────────────────────────────────────────────────────────────
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("search-engine-crawler/0.1 (CS search engine; contact via GitHub)")
        .build()?;

    // Run all three sources concurrently — each collects into its own Vec,
    // no shared state during network I/O.
    println!("[link_harvest] starting HN, lobste.rs and tildes in parallel…");
    let (hn_urls, lobsters_urls, tildes_urls) = tokio::join!(
        harvest_hn(client.clone()),
        harvest_lobsters(client.clone()),
        harvest_tildes(client.clone()),
    );

    // Merge and deduplicate
    let mut all_urls = hn_urls;
    all_urls.extend(lobsters_urls);
    all_urls.extend(tildes_urls);
    all_urls.sort_unstable();
    all_urls.dedup();

    println!(
        "[link_harvest] collected {} unique candidate URLs, writing to DB…",
        all_urls.len()
    );

    let mut enqueued = 0usize;
    let mut skipped = 0usize;
    enqueue_urls(&db, &all_urls, &mut enqueued, &mut skipped)?;

    let _ = db.flush_wal(true);

    println!("\n[link_harvest] complete.");
    println!("  enqueued: {enqueued} new URLs into crawl frontier");
    println!("  skipped:  {skipped} (already seen/queued or domain not allowed)");
    println!("\nNext: cargo run --release --bin crawl");

    Ok(())
}
