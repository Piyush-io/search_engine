use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use reqwest::Client;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{sleep, timeout};
use tracing_subscriber::EnvFilter;

use rocksdb::DB;
use search_engine::{
    config,
    crawler::{
        fetch, parse, persist, recover, robots, scheduler::CrawlScheduler,
        types::{RejectReason, UrlTask, FetchResult},
        persist::PersistCommand,
    },
    storage,
};

const SEED_URLS: &[&str] = &[
    "https://doc.rust-lang.org/",
    "https://docs.python.org/3/",
    "https://developer.mozilla.org/en-US/docs/Web/JavaScript",
    "https://en.wikipedia.org/wiki/Computer_science",
    "https://stackoverflow.com/questions/tagged/algorithms",
    "https://blog.rust-lang.org/",
    "https://blog.cloudflare.com/",
    "https://jvns.ca/",
    "https://martinfowler.com/",
    "https://blog.acolyer.org/",
    "https://eng.uber.com/",
    "https://netflixtechblog.com/",
    "https://research.google/blog/",
    "https://cppreference.com/",
    "https://docs.rs/tokio/latest/tokio/",
    "https://go.dev/doc/",
    "https://nodejs.org/en/docs/",
    "https://kubernetes.io/docs/home/",
    "https://docs.docker.com/",
    "https://learn.microsoft.com/en-us/dotnet/",
    "https://docs.julialang.org/en/v1/",
    "https://news.ycombinator.com/",
    "https://dev.to/",
    "https://medium.com/tag/programming",
    "https://www.infoq.com/",
    "https://thenewstack.io/",
    "https://highscalability.com/",
    "https://engineering.fb.com/",
    "https://aws.amazon.com/blogs/architecture/",
    "https://developers.googleblog.com/",
    "https://techcrunch.com/category/artificial-intelligence/",
    "https://simonwillison.net/",
    "https://www.joelonsoftware.com/",
    "https://danluu.com/",
    "https://rachelbythebay.com/w/",
    "https://without.boats/blog/",
    "https://matklad.github.io/",
    "https://arxiv.org/list/cs.LG/recent",
    "https://arxiv.org/list/cs.DS/recent",
    "https://arxiv.org/list/cs.DC/recent",
    "https://paperswithcode.com/",
    "https://distill.pub/",
    "https://cacm.acm.org/",
    "https://people.csail.mit.edu/",
    "https://cs.stanford.edu/",
    "https://ocw.mit.edu/courses/electrical-engineering-and-computer-science/",
    "https://en.wikipedia.org/wiki/Algorithm",
    "https://en.wikipedia.org/wiki/Data_structure",
    "https://en.wikipedia.org/wiki/Machine_learning",
    "https://en.wikipedia.org/wiki/Operating_system",
    "https://en.wikipedia.org/wiki/Distributed_computing",
    "https://en.wikipedia.org/wiki/Programming_language",
    "https://en.wikipedia.org/wiki/Database",
    "https://en.wikipedia.org/wiki/Computer_network",
    "https://en.wikipedia.org/wiki/Compiler",
    "https://ask.ubuntu.com/",
    "https://unix.stackexchange.com/",
    "https://doc.rust-lang.org/reference/",
    "https://doc.rust-lang.org/nomicon/",
    "https://doc.rust-lang.org/rust-by-example/",
    "https://docs.swift.org/swift-book/",
    "https://kotlinlang.org/docs/",
    "https://docs.scala-lang.org/",
    "https://clojure.org/reference/",
    "https://elixir-lang.org/docs.html",
    "https://hexdocs.pm/elixir/",
    "https://www.haskell.org/documentation/",
    "https://wiki.haskell.org/",
    "https://ziglang.org/documentation/",
    "https://docs.oracle.com/en/java/javase/21/docs/api/",
    "https://slack.engineering/",
    "https://dropbox.tech/",
    "https://shopify.engineering/",
    "https://www.databricks.com/blog/engineering",
    "https://grafana.com/blog/",
    "https://www.elastic.co/blog/",
    "https://systemdesign.one/",
    "https://www.oreilly.com/radar/",
    "https://microservices.io/patterns/",
    "https://12factor.net/",
    "https://sre.google/sre-book/",
    "https://sre.google/workbook/",
    "https://owasp.org/www-project-top-ten/",
    "https://cheatsheetseries.owasp.org/",
    "https://portswigger.net/web-security",
    "https://www.schneier.com/blog/",
    "https://www.terraform.io/docs/",
    "https://docs.ansible.com/",
    "https://prometheus.io/docs/",
    "https://opentelemetry.io/docs/",
    "https://kafka.apache.org/documentation/",
    "https://redis.io/docs/",
    "https://www.postgresql.org/docs/",
    "https://sqlite.org/docs.html",
    "https://cp-algorithms.com",
    "https://cstheory.stackexchange.com/questions",
    "https://norvig.com",
    "https://www.kernel.org/doc/html/latest/",
    "https://www.brendangregg.com/blog/index.html",
    "https://pages.cs.wisc.edu/~remzi/OSTEP/",
    "https://pdos.csail.mit.edu/6.828/2023/schedule.html",
    "https://os.phil-opp.com",
    "https://www.linuxfromscratch.org/lfs/view/stable/",
    "https://lwn.net/Kernel/Index/",
    "https://www.agner.org/optimize/",
    "https://uops.info/table.html",
    "https://sandpile.org",
    "https://book.rvemu.app",
    "https://llvm.org/docs/",
    "https://clang.llvm.org/docs/",
    "https://gcc.gnu.org/onlinedocs/gcc/",
    "https://craftinginterpreters.com/contents.html",
    "https://eli.thegreenplace.net",
    "https://www.cs.cornell.edu/courses/cs6120/2020fa/blog/",
    "https://swtch.com/~rsc/regexp/",
    "https://softwarefoundations.cis.upenn.edu",
    "https://homotopytypetheory.org/book/",
    "https://xavierleroy.org",
    "https://faultlore.com/blah/",
    "https://www.scattered-thoughts.net",
    "https://www.interdb.jp/pg/",
    "https://15445.courses.cs.cmu.edu/fall2024/",
    "https://15721.courses.cs.cmu.edu/spring2024/",
    "https://www.cockroachlabs.com/blog/",
    "https://jepsen.io/analyses",
    "https://martin.kleppmann.com",
    "https://muratbuffalo.blogspot.com",
    "https://fly.io/blog/",
    "https://explained.ai",
    "https://cs231n.github.io",
    "https://nlp.stanford.edu/IR-book/html/htmledition/",
    "https://learnopengl.com",
    "https://vulkan-tutorial.com",
    "https://raytracing.github.io",
    "https://beej.us/guide/bgnet/html/",
    "https://hacks.mozilla.org",
    "https://webassembly.org",
    "https://cryptopals.com",
    "https://blog.cryptographyengineering.com",
    "https://www.usenix.org/publications/proceedings/",
    "https://isocpp.org/faq",
    "https://abseil.io/docs/cpp/",
    "https://beej.us/guide/bgc/html/",
    "https://paulgraham.com/articles.html",
    "https://thume.ca",
    "https://lamport.azurewebsites.net/tla/tla.html",
    "https://coq.inria.fr/documentation",
    "https://www.cl.cam.ac.uk/~pes20/weakmemory/",
    "https://interrupt.memfault.com/blog",
    "https://quantum.country/qcvc",
    "https://learn.qiskit.org",
    "https://missing.csail.mit.edu",
    "https://staffeng.com/guides/",
    "https://www.nand2tetris.org/course",
];

const PROCESS_ONE_TIMEOUT: Duration = Duration::from_secs(45);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Ok(directive) = "html5ever::tree_builder=off".parse() {
        env_filter = env_filter.add_directive(directive);
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();

    let cfg = Arc::new(config::load()?);
    let db = Arc::new(storage::open_db_with_cache(
        &cfg.paths.db_path,
        cfg.rocksdb.block_cache_mb,
    )?);

    let per_domain_counts = recover::load_existing_content_counts(&db)?;
    let existing_pages: usize = per_domain_counts.values().sum();
    let per_domain_processed: Arc<DashMap<String, usize>> = Arc::new(DashMap::new());
    for (host, count) in per_domain_counts {
        per_domain_processed.insert(host, count);
    }

    println!(
        "[crawl] start existing_pages={} target_max_pages={}",
        existing_pages, cfg.crawl.max_pages
    );

    if existing_pages >= cfg.crawl.max_pages {
        println!("[crawl] target already reached ({} >= {}), nothing to do", existing_pages, cfg.crawl.max_pages);
        return Ok(());
    }

    let scheduler = Arc::new(CrawlScheduler::new(cfg.crawl.rate_limit_ms));
    let robots_cache = robots::new_cache();
    let (frontier_loaded, frontier_purged) =
        recover::load_frontier_into_scheduler(&db, &scheduler, &per_domain_processed, &robots_cache).await?;
    println!("[crawl] frontier recovered: {} live, {} purged", frontier_loaded, frontier_purged);

    let seeded = recover::seed_frontier(SEED_URLS, &db, &scheduler, &per_domain_processed, &robots_cache).await?;
    println!("[crawl] seeded {} curated URLs", seeded);

    let client = Arc::new(
        Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .read_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent("search-engine-crawler/0.2")
            .danger_accept_invalid_certs(true)
            .build()?,
    );

    let cpu_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2);

    let (parse_tx, parse_rx) = mpsc::channel::<FetchResult>(cfg.crawl.concurrency.max(1) * 2);
    let (persist_tx, persist_rx) = mpsc::channel::<PersistCommand>(cfg.crawl.concurrency.max(1) * 4);

    let dns_ok_cache: Arc<DashMap<String, bool>> = Arc::new(DashMap::new());
    let processed_pages = Arc::new(AtomicUsize::new(0));
    let fetch_inflight = Arc::new(AtomicUsize::new(0));
    let persist_pending = Arc::new(AtomicUsize::new(0));

    let writer_handle = tokio::spawn(writer_loop(
        Arc::clone(&db),
        Arc::clone(&scheduler),
        Arc::clone(&per_domain_processed),
        Arc::clone(&robots_cache),
        Arc::clone(&processed_pages),
        Arc::clone(&persist_pending),
        existing_pages,
        persist_rx,
    ));

    let mut parse_handles = Vec::new();
    let parse_worker_count = cpu_workers.min(8);
    let parse_rx = Arc::new(Mutex::new(parse_rx));
    for _ in 0..parse_worker_count {
        let persist_tx = persist_tx.clone();
        let persist_pending = Arc::clone(&persist_pending);
        let parse_rx = Arc::clone(&parse_rx);
        parse_handles.push(tokio::spawn(async move {
            loop {
                let next = {
                    let mut guard = parse_rx.lock().await;
                    guard.recv().await
                };
                let Some(payload) = next else { break; };

                let depth = payload.task.depth;
                let command = match tokio::task::spawn_blocking(move || parse::parse_result(payload)).await {
                    Ok(Ok(page)) => {
                        let aliases = vec![page.page_record.url.clone()];
                        PersistCommand::Accept { page, aliases, depth }
                    },
                    Ok(Err(reason)) => {
                        PersistCommand::Reject { 
                            url: String::new(), 
                            host: String::new(), 
                            aliases: Vec::new(), 
                            outlinks: Vec::new(), 
                            reason,
                            depth,
                        }
                    },
                    Err(_) => PersistCommand::Reject { 
                        url: String::new(), 
                        host: String::new(), 
                        aliases: Vec::new(), 
                        outlinks: Vec::new(), 
                        reason: RejectReason::ParsePanic,
                        depth,
                    },
                };

                persist_pending.fetch_add(1, Ordering::SeqCst);
                if persist_tx.send(command).await.is_err() {
                    persist_pending.fetch_sub(1, Ordering::SeqCst);
                    break;
                }
            }
        }));
    }

    let mut fetch_handles = Vec::new();
    for _ in 0..cfg.crawl.concurrency.max(1) {
        let db = Arc::clone(&db);
        let scheduler = Arc::clone(&scheduler);
        let client = Arc::clone(&client);
        let robots_cache = Arc::clone(&robots_cache);
        let dns_ok_cache = Arc::clone(&dns_ok_cache);
        let parse_tx = parse_tx.clone();
        let persist_tx = persist_tx.clone();
        let persist_pending = Arc::clone(&persist_pending);
        let fetch_inflight = Arc::clone(&fetch_inflight);

        fetch_handles.push(tokio::spawn(async move {
            while let Some(task) = scheduler.next_task().await {
                let host = task.host.clone();
                fetch_inflight.fetch_add(1, Ordering::SeqCst);
                
                let result = timeout(
                    PROCESS_ONE_TIMEOUT,
                    fetch::fetch_task(&db, &client, &robots_cache, &dns_ok_cache, task.clone()),
                ).await;

                match result {
                    Ok(Ok(payload)) => {
                        if parse_tx.send(payload).await.is_err() {
                            break;
                        }
                    }
                    Ok(Err(reason)) => {
                        persist_pending.fetch_add(1, Ordering::SeqCst);
                        let cmd = PersistCommand::Reject {
                            url: task.url.clone(),
                            host: task.host.clone(),
                            aliases: vec![task.url.clone()],
                            outlinks: Vec::new(),
                            reason,
                            depth: task.depth,
                        };
                        if persist_tx.send(cmd).await.is_err() {
                            persist_pending.fetch_sub(1, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(_) => {
                        persist_pending.fetch_add(1, Ordering::SeqCst);
                        let cmd = PersistCommand::Reject {
                            url: task.url.clone(),
                            host: task.host.clone(),
                            aliases: vec![task.url.clone()],
                            outlinks: Vec::new(),
                            reason: RejectReason::Timeout,
                            depth: task.depth,
                        };
                        if persist_tx.send(cmd).await.is_err() {
                            persist_pending.fetch_sub(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }

                scheduler.complete_host(&host).await;
                fetch_inflight.fetch_sub(1, Ordering::SeqCst);
            }
        }));
    }

    let target_new_pages = cfg.crawl.max_pages.saturating_sub(existing_pages);
    let mut last_status = Instant::now();
    loop {
        let processed = processed_pages.load(Ordering::SeqCst);
        let (pending_urls, inflight_hosts, tracked_hosts) = scheduler.stats().await;
        
        if processed >= target_new_pages {
            scheduler.close().await;
            break;
        }

        if pending_urls == 0 && inflight_hosts == 0 && fetch_inflight.load(Ordering::SeqCst) == 0 {
             scheduler.close().await;
             break;
        }

        if last_status.elapsed() >= Duration::from_secs(5) {
            println!(
                "[crawl] status pending_urls={} inflight_hosts={} tracked_hosts={} processed={}",
                pending_urls, inflight_hosts, tracked_hosts, processed
            );
            last_status = Instant::now();
        }
        sleep(Duration::from_secs(1)).await;
    }

    for h in fetch_handles { let _ = h.await; }
    drop(parse_tx);
    for h in parse_handles { let _ = h.await; }
    drop(persist_tx);
    let _ = writer_handle.await;

    println!("[crawl] finished. total processed this run: {}", processed_pages.load(Ordering::SeqCst));
    Ok(())
}

async fn writer_loop(
    db: Arc<DB>,
    scheduler: Arc<CrawlScheduler>,
    per_domain_processed: Arc<DashMap<String, usize>>,
    robots_cache: robots::RobotsCache,
    processed_pages: Arc<AtomicUsize>,
    persist_pending: Arc<AtomicUsize>,
    _existing_pages: usize,
    mut persist_rx: mpsc::Receiver<PersistCommand>,
) {
    while let Some(command) = persist_rx.recv().await {
        let is_accept = matches!(command, PersistCommand::Accept { .. });
        if let Err(err) = persist::persist_command(
            &db,
            &scheduler,
            &per_domain_processed,
            &robots_cache,
            command,
        ).await {
            eprintln!("[crawl] persist error: {err}");
        } else if is_accept {
            processed_pages.fetch_add(1, Ordering::SeqCst);
        }
        persist_pending.fetch_sub(1, Ordering::SeqCst);
    }
}
