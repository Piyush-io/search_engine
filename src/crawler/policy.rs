use url::Url;

const MAX_DISCOVERED_PER_PAGE: usize = 200;
const MAX_CHUNKS_PER_PAGE: usize = 220;

pub fn host_matches_rule(host: &str, rule: &str) -> bool {
    let host = host.strip_prefix("www.").unwrap_or(host);
    let rule = rule.strip_prefix("www.").unwrap_or(rule);
    host == rule || host.ends_with(&format!(".{rule}"))
}

fn host_matches_exact_rule(host: &str, rule: &str) -> bool {
    let host = host.strip_prefix("www.").unwrap_or(host);
    let rule = rule.strip_prefix("www.").unwrap_or(rule);
    host == rule
}

pub fn domain_rate_limit_ms(default_ms: u64, host: &str) -> u64 {
    if host_matches_rule(host, "wikipedia.org") {
        default_ms.max(1200)
    } else {
        default_ms
    }
}

pub fn host_is_high_quality(host: &str) -> bool {
    if host_matches_rule(host, "developer.mozilla.org")
        || host_matches_rule(host, "docs.python.org")
        || host_matches_rule(host, "doc.rust-lang.org")
        || host_matches_rule(host, "docs.rs")
        || host_matches_rule(host, "go.dev")
        || host_matches_rule(host, "pkg.go.dev")
        || host_matches_rule(host, "kubernetes.io")
        || host_matches_rule(host, "learn.microsoft.com")
        || host_matches_rule(host, "docs.oracle.com")
        || host_matches_rule(host, "docs.aws.amazon.com")
        || host_matches_rule(host, "docs.docker.com")
        || host_matches_rule(host, "docs.ansible.com")
        || host_matches_rule(host, "prometheus.io")
        || host_matches_rule(host, "opentelemetry.io")
        || host_matches_rule(host, "www.kernel.org")
        || host_matches_rule(host, "docs.kernel.org")
        || host_matches_rule(host, "llvm.org")
        || host_matches_rule(host, "clang.llvm.org")
        || host_matches_rule(host, "gcc.gnu.org")
        || host_matches_rule(host, "cppreference.com")
        || host_matches_rule(host, "sqlite.org")
        || host_matches_rule(host, "www.postgresql.org")
        || host_matches_rule(host, "cp-algorithms.com")
        || host_matches_rule(host, "norvig.com")
        || host_matches_rule(host, "pages.cs.wisc.edu")
        || host_matches_rule(host, "pdos.csail.mit.edu")
        || host_matches_rule(host, "www.cs.princeton.edu")
        || host_matches_rule(host, "www.cs.cornell.edu")
        || host_matches_rule(host, "15445.courses.cs.cmu.edu")
        || host_matches_rule(host, "15721.courses.cs.cmu.edu")
        || host_matches_rule(host, "softwarefoundations.cis.upenn.edu")
        || host_matches_rule(host, "homotopytypetheory.org")
        || host_matches_rule(host, "lamport.azurewebsites.net")
        || host_matches_rule(host, "coq.inria.fr")
        || host_matches_rule(host, "www.cl.cam.ac.uk")
        || host_matches_rule(host, "book.rvemu.app")
        || host_matches_rule(host, "craftinginterpreters.com")
        || host_matches_rule(host, "raytracing.github.io")
        || host_matches_rule(host, "quantum.country")
        || host_matches_rule(host, "learn.qiskit.org")
        || host_matches_rule(host, "missing.csail.mit.edu")
        || host_matches_rule(host, "os.phil-opp.com")
        || host_matches_rule(host, "lwn.net")
        || host_matches_rule(host, "www.brendangregg.com")
        || host_matches_rule(host, "www.linuxfromscratch.org")
        || host_matches_rule(host, "www.agner.org")
        || host_matches_rule(host, "uops.info")
        || host_matches_rule(host, "sandpile.org")
        || host_matches_rule(host, "eli.thegreenplace.net")
        || host_matches_rule(host, "swtch.com")
        || host_matches_rule(host, "xavierleroy.org")
        || host_matches_rule(host, "faultlore.com")
        || host_matches_rule(host, "www.interdb.jp")
        || host_matches_rule(host, "jepsen.io")
        || host_matches_rule(host, "martin.kleppmann.com")
        || host_matches_rule(host, "explained.ai")
        || host_matches_rule(host, "nlp.stanford.edu")
        || host_matches_rule(host, "learnopengl.com")
        || host_matches_rule(host, "vulkan-tutorial.com")
        || host_matches_rule(host, "beej.us")
        || host_matches_rule(host, "webassembly.org")
        || host_matches_rule(host, "cryptopals.com")
        || host_matches_rule(host, "blog.cryptographyengineering.com")
        || host_matches_rule(host, "www.usenix.org")
        || host_matches_rule(host, "isocpp.org")
        || host_matches_rule(host, "abseil.io")
        || host_matches_rule(host, "interrupt.memfault.com")
        || host_matches_rule(host, "blog.cloudflare.com")
        || host_matches_rule(host, "martinfowler.com")
        || host_matches_rule(host, "blog.acolyer.org")
        || host_matches_rule(host, "research.google")
        || host_matches_rule(host, "blog.rust-lang.org")
        || host_matches_rule(host, "simonwillison.net")
        || host_matches_rule(host, "danluu.com")
        || host_matches_rule(host, "rachelbythebay.com")
        || host_matches_rule(host, "without.boats")
        || host_matches_rule(host, "matklad.github.io")
        || host_matches_rule(host, "thume.ca")
        || host_matches_rule(host, "fly.io")
    {
        true
    } else {
        false
    }
}

pub fn recrawl_days_for_host(default_days: u64, host: &str) -> u64 {
    if !host_is_high_quality(host) {
        return u64::MAX;
    }

    if host_matches_rule(host, "developer.mozilla.org")
        || host_matches_rule(host, "docs.python.org")
        || host_matches_rule(host, "doc.rust-lang.org")
        || host_matches_rule(host, "docs.rs")
        || host_matches_rule(host, "go.dev")
        || host_matches_rule(host, "pkg.go.dev")
        || host_matches_rule(host, "kubernetes.io")
        || host_matches_rule(host, "learn.microsoft.com")
        || host_matches_rule(host, "docs.oracle.com")
        || host_matches_rule(host, "docs.aws.amazon.com")
        || host_matches_rule(host, "docs.docker.com")
        || host_matches_rule(host, "docs.ansible.com")
        || host_matches_rule(host, "prometheus.io")
        || host_matches_rule(host, "opentelemetry.io")
        || host_matches_rule(host, "www.kernel.org")
        || host_matches_rule(host, "docs.kernel.org")
        || host_matches_rule(host, "llvm.org")
        || host_matches_rule(host, "clang.llvm.org")
        || host_matches_rule(host, "gcc.gnu.org")
        || host_matches_rule(host, "cppreference.com")
        || host_matches_rule(host, "sqlite.org")
        || host_matches_rule(host, "www.postgresql.org")
    {
        default_days.clamp(7, 30)
    } else if host_matches_rule(host, "blog.cloudflare.com")
        || host_matches_rule(host, "martinfowler.com")
        || host_matches_rule(host, "blog.acolyer.org")
        || host_matches_rule(host, "research.google")
        || host_matches_rule(host, "blog.rust-lang.org")
        || host_matches_rule(host, "simonwillison.net")
        || host_matches_rule(host, "danluu.com")
        || host_matches_rule(host, "rachelbythebay.com")
        || host_matches_rule(host, "www.elastic.co")
        || host_matches_rule(host, "fly.io")
        || host_matches_rule(host, "www.cockroachlabs.com")
        || host_matches_rule(host, "jepsen.io")
        || host_matches_rule(host, "martin.kleppmann.com")
        || host_matches_rule(host, "muratbuffalo.blogspot.com")
        || host_matches_rule(host, "thume.ca")
        || host_matches_rule(host, "interrupt.memfault.com")
        || host_matches_rule(host, "blog.cryptographyengineering.com")
        || host_matches_rule(host, "without.boats")
        || host_matches_rule(host, "matklad.github.io")
        || host_matches_rule(host, "eli.thegreenplace.net")
        || host_matches_rule(host, "faultlore.com")
    {
        default_days.clamp(30, 90)
    } else {
        default_days.max(90)
    }
}

pub fn wiki_topic_allowed(title: &str) -> bool {
    const TOPIC_MARKERS: &[&str] = &[
        "computer",
        "software",
        "programming",
        "algorithm",
        "data_structure",
        "operating_system",
        "kernel",
        "compiler",
        "interpreter",
        "database",
        "distributed",
        "network",
        "protocol",
        "internet",
        "web_",
        "crypt",
        "machine_learning",
        "deep_learning",
        "artificial_intelligence",
        "neural_network",
        "computer_science",
        "linux",
        "unix",
        "open_source",
    ];

    TOPIC_MARKERS.iter().any(|m| title.contains(m))
}

pub fn discovery_limit_for_host(host: &str) -> usize {
    if host_matches_rule(host, "wikipedia.org") {
        60
    } else {
        MAX_DISCOVERED_PER_PAGE
    }
}

pub fn chunk_limit_for_host(host: &str) -> usize {
    if host_matches_rule(host, "wikipedia.org") {
        120
    } else if host_matches_rule(host, "docs.rs") {
        120
    } else if host_matches_rule(host, "llvm.org") {
        140
    } else {
        MAX_CHUNKS_PER_PAGE
    }
}

fn normalized_domain_cap(host: &str) -> Option<usize> {
    const RULES: &[(&str, usize)] = &[
        ("infoq.com", 5_000),
        ("joelonsoftware.com", 3_000),
        ("databricks.com", 3_000),
        ("elastic.co", 8_000),
        ("terraform.io", 15_000),
        ("postgresql.org", 20_000),
        ("kernel.org", 20_000),
        ("brendangregg.com", 5_000),
        ("linuxfromscratch.org", 2_000),
        ("interdb.jp", 500),
        ("cockroachlabs.com", 5_000),
        ("scattered-thoughts.net", 1_000),
        ("usenix.org", 10_000),
        ("cl.cam.ac.uk", 2_000),
        ("cs.cornell.edu", 5_000),
        ("nand2tetris.org", 1_000),
        ("haskell.org", 10_000),
    ];

    RULES
        .iter()
        .find(|(rule, _)| host_matches_rule(host, rule))
        .map(|(_, cap)| *cap)
}

pub fn domain_cap(host: &str) -> usize {
    match host {
        // Tier S — Canonical docs (high signal, structured, enormous)
        "cppreference.com" | "en.cppreference.com" | "ch.cppreference.com" => 25_000,
        "developer.mozilla.org" => 30_000,
        "docs.python.org" => 25_000,
        "doc.rust-lang.org" => 25_000,
        "en.wikipedia.org" => 12_000,
        "docs.rs" => 30_000,
        "go.dev" => 20_000,
        "pkg.go.dev" => 20_000,
        "kubernetes.io" => 20_000,
        "learn.microsoft.com" => 25_000,
        "docs.oracle.com" => 20_000,
        "hexdocs.pm" => 20_000,
        "docs.aws.amazon.com" => 20_000,
        "www.postgresql.org" => 20_000,
        "www.kernel.org" => 20_000,
        "docs.kernel.org" => 20_000,
        "llvm.org" => 15_000,
        "docs.docker.com" => 15_000,
        "docs.ansible.com" => 15_000,
        "www.terraform.io" => 15_000,
        "kafka.apache.org" => 10_000,
        "redis.io" => 10_000,
        "sqlite.org" => 10_000,
        "prometheus.io" => 10_000,
        "opentelemetry.io" => 10_000,

        // Tier S — Language docs
        "nodejs.org" => 15_000,
        "docs.julialang.org" => 10_000,
        "docs.swift.org" => 10_000,
        "kotlinlang.org" => 10_000,
        "docs.scala-lang.org" => 10_000,
        "wiki.haskell.org" => 10_000,
        "www.haskell.org" => 5_000,
        "ziglang.org" => 8_000,
        "clojure.org" => 5_000,
        "elixir-lang.org" => 5_000,

        // Tier A — Strong tech blogs
        "stackoverflow.com" => 8_000,
        "blog.cloudflare.com" => 10_000,
        "jvns.ca" => 5_000,
        "martinfowler.com" => 8_000,
        "blog.acolyer.org" => 8_000,
        "eng.uber.com" => 4_000,
        "netflixtechblog.com" => 4_000,
        "research.google" => 8_000,
        "blog.rust-lang.org" => 5_000,
        "stackoverflow.blog" => 2_000,
        "simonwillison.net" => 5_000,
        "danluu.com" => 5_000,
        "rachelbythebay.com" => 5_000,
        "engineering.fb.com" => 4_000,
        "developers.googleblog.com" => 4_000,
        "aws.amazon.com" => 4_000,
        "explore.alas.aws.amazon.com" => 200,
        "engineering.linkedin.com" => 4_000,
        "slack.engineering" => 3_000,
        "dropbox.tech" => 3_000,
        "shopify.engineering" => 3_000,
        "grafana.com" => 5_000,
        "www.elastic.co" => 6_000,

        // Tier A — Research & academic
        "arxiv.org" => 25_000,
        "paperswithcode.com" => 10_000,
        "distill.pub" => 3_000,
        "cacm.acm.org" => 8_000,
        "people.csail.mit.edu" => 8_000,
        "cs.stanford.edu" => 10_000,
        "ocw.mit.edu" => 15_000,
        "lwn.net" => 15_000,

        // Tier A — Q&A
        "ask.ubuntu.com" => 5_000,
        "unix.stackexchange.com" => 5_000,
        "cstheory.stackexchange.com" => 8_000,

        // Tier A — System design & SRE
        "sre.google" => 10_000,
        "www.oreilly.com" => 8_000,
        "owasp.org" => 10_000,
        "cheatsheetseries.owasp.org" => 8_000,
        "portswigger.net" => 10_000,
        "www.schneier.com" => 5_000,
        "systemdesign.one" => 5_000,
        "microservices.io" => 3_000,
        "12factor.net" => 1_000,
        "highscalability.com" => 5_000,

        // Tier B — Noisy/UGC (keep very low or effectively disabled)
        "news.ycombinator.com" => 100,
        "dev.to" => 100,
        "medium.com" => 100,
        "www.infoq.com" => 500,
        "thenewstack.io" => 250,
        "techcrunch.com" => 0,
        "www.joelonsoftware.com" => 1_000,
        "www.databricks.com" => 250,
        "community.databricks.com" => 0,

        // Tier A — Niche high-quality
        "without.boats" => 3_000,
        "matklad.github.io" => 3_000,

        // ── New seeds from seeds.md ──
        "cp-algorithms.com" => 5_000,
        "norvig.com" => 2_000,
        "www.brendangregg.com" => 5_000,
        "pages.cs.wisc.edu" => 2_000,
        "pdos.csail.mit.edu" => 2_000,
        "os.phil-opp.com" => 1_000,
        "www.linuxfromscratch.org" => 2_000,
        "www.agner.org" => 500,
        "uops.info" => 500,
        "sandpile.org" => 1_000,
        "book.rvemu.app" => 500,
        "circt.llvm.org" => 1_000,
        "clang.llvm.org" => 8_000,
        "gcc.gnu.org" => 8_000,
        "craftinginterpreters.com" => 1_000,
        "eli.thegreenplace.net" => 5_000,
        "www.cs.cornell.edu" => 5_000,
        "swtch.com" => 500,
        "softwarefoundations.cis.upenn.edu" => 5_000,
        "homotopytypetheory.org" => 2_000,
        "xavierleroy.org" => 1_000,
        "faultlore.com" => 1_000,
        "www.scattered-thoughts.net" => 1_000,
        "www.interdb.jp" => 500,
        "15445.courses.cs.cmu.edu" => 1_000,
        "15721.courses.cs.cmu.edu" => 1_000,
        "www.cockroachlabs.com" => 5_000,
        "jepsen.io" => 2_000,
        "martin.kleppmann.com" => 2_000,
        "muratbuffalo.blogspot.com" => 5_000,
        "fly.io" => 3_000,
        "explained.ai" => 500,
        "cs231n.github.io" => 500,
        "nlp.stanford.edu" => 3_000,
        "learnopengl.com" => 2_000,
        "vulkan-tutorial.com" => 1_000,
        "raytracing.github.io" => 500,
        "beej.us" => 2_000,
        "hacks.mozilla.org" => 3_000,
        "webassembly.org" => 1_000,
        "cryptopals.com" => 500,
        "blog.cryptographyengineering.com" => 2_000,
        "www.usenix.org" => 10_000,
        "isocpp.org" => 5_000,
        "abseil.io" => 2_000,
        "paulgraham.com" => 2_000,
        "thume.ca" => 1_000,
        "lamport.azurewebsites.net" => 1_000,
        "coq.inria.fr" => 2_000,
        "www.cl.cam.ac.uk" => 2_000,
        "interrupt.memfault.com" => 5_000,
        "quantum.country" => 200,
        "learn.qiskit.org" => 2_000,
        "missing.csail.mit.edu" => 500,
        "staffeng.com" => 1_000,
        "www.nand2tetris.org" => 1_000,
        _ => normalized_domain_cap(host).unwrap_or(2_000),
    }
}

pub fn host_allowed(host: &str) -> bool {
    let allow = [
        "docs.aws.amazon.com",
        "doc.rust-lang.org",
        "docs.python.org",
        "developer.mozilla.org",
        "en.wikipedia.org",
        "stackoverflow.com",
        "cppreference.com",
        "en.cppreference.com",
        "blog.rust-lang.org",
        "blog.cloudflare.com",
        "jvns.ca",
        "martinfowler.com",
        "blog.acolyer.org",
        "research.google",
        "docs.rs",
        "go.dev",
        "pkg.go.dev",
        "nodejs.org",
        "kubernetes.io",
        "docs.docker.com",
        "learn.microsoft.com",
        "docs.julialang.org",
        "highscalability.com",
        "simonwillison.net",
        "www.joelonsoftware.com",
        "danluu.com",
        "rachelbythebay.com",
        "without.boats",
        "matklad.github.io",
        "arxiv.org",
        "paperswithcode.com",
        "distill.pub",
        "cacm.acm.org",
        "people.csail.mit.edu",
        "cs.stanford.edu",
        "ocw.mit.edu",
        "ask.ubuntu.com",
        "unix.stackexchange.com",
        "docs.swift.org",
        "kotlinlang.org",
        "docs.scala-lang.org",
        "clojure.org",
        "elixir-lang.org",
        "hexdocs.pm",
        "wiki.haskell.org",
        "www.haskell.org",
        "ziglang.org",
        "docs.oracle.com",
        "slack.engineering",
        "dropbox.tech",
        "grafana.com",
        "www.elastic.co",
        "shopify.engineering",
        "engineering.linkedin.com",
        "systemdesign.one",
        "www.oreilly.com",
        "microservices.io",
        "12factor.net",
        "sre.google",
        "owasp.org",
        "cheatsheetseries.owasp.org",
        "portswigger.net",
        "www.schneier.com",
        "www.terraform.io",
        "docs.ansible.com",
        "prometheus.io",
        "opentelemetry.io",
        "kafka.apache.org",
        "redis.io",
        "www.postgresql.org",
        "sqlite.org",
        "cp-algorithms.com",
        "cstheory.stackexchange.com",
        "norvig.com",
        "www.kernel.org",
        "docs.kernel.org",
        "www.brendangregg.com",
        "pages.cs.wisc.edu",
        "pdos.csail.mit.edu",
        "os.phil-opp.com",
        "www.linuxfromscratch.org",
        "lwn.net",
        "www.agner.org",
        "uops.info",
        "sandpile.org",
        "book.rvemu.app",
        "llvm.org",
        "clang.llvm.org",
        "gcc.gnu.org",
        "craftinginterpreters.com",
        "eli.thegreenplace.net",
        "www.cs.cornell.edu",
        "swtch.com",
        "softwarefoundations.cis.upenn.edu",
        "homotopytypetheory.org",
        "xavierleroy.org",
        "faultlore.com",
        "www.scattered-thoughts.net",
        "www.interdb.jp",
        "15445.courses.cs.cmu.edu",
        "15721.courses.cs.cmu.edu",
        "www.cockroachlabs.com",
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
        "www.usenix.org",
        "isocpp.org",
        "abseil.io",
        "paulgraham.com",
        "thume.ca",
        "lamport.azurewebsites.net",
        "coq.inria.fr",
        "www.cl.cam.ac.uk",
        "interrupt.memfault.com",
        "quantum.country",
        "learn.qiskit.org",
        "missing.csail.mit.edu",
        "staffeng.com",
        "www.nand2tetris.org",
    ];

    allow.iter().any(|rule| host_matches_exact_rule(host, rule))
}

pub fn path_looks_binary(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [
        ".tar", ".tar.gz", ".tgz", ".zip", ".7z", ".gz", ".xz", ".bz2", ".pdf", ".png", ".jpg",
        ".jpeg", ".gif", ".webp", ".svg", ".mp4", ".mp3", ".woff", ".woff2", ".ttf",
    ]
    .iter()
    .any(|ext| p.ends_with(ext))
}

pub fn path_has_locale_segment(path: &str) -> bool {
    const LOCALES: &[&str] = &[
        "zh", "ja", "ko", "ru", "uk", "it", "es", "fr", "de", "pt", "tr", "pl", "cs", "nl", "sv",
        "fi", "no", "da", "hu", "ro",
    ];

    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_ascii_lowercase())
        .any(|segment| LOCALES.contains(&segment.as_str()))
}

pub fn score_url(url: &Url, depth: u16) -> i32 {
    let mut score: i32 = 100 - (depth as i32 * 10); // Signal decay by depth

    let path = url.path().to_ascii_lowercase();
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();

    // High signal boost
    let high_signal = [
        "/docs/",
        "/api/",
        "/guide/",
        "/tutorial/",
        "/manual/",
        "/reference/",
        "/wiki/",
        "/blog/",
        "/articles/",
        "/post/",
        "/news/",
    ];
    if high_signal.iter().any(|p| path.contains(p)) {
        score += 50;
    }

    // Low signal penalty
    let low_signal = [
        "/tag/",
        "/category/",
        "/archive/",
        "/search/",
        "/page/",
        "/index/",
        "/commit/",
        "/diff/",
        "/blob/",
        "/source/",
        "/raw/",
        "/v-",
        "/rev-",
    ];
    if low_signal.iter().any(|p| path.contains(p)) {
        score -= 70;
    }

    // Extra penalty for deep query parameters
    if url.query_pairs().count() > 0 {
        score -= 20;
    }

    // Boost S-Tier domains
    let s_tier = [
        "docs.rs",
        "doc.rust-lang.org",
        "docs.python.org",
        "en.cppreference.com",
        "developer.mozilla.org",
        "en.wikipedia.org",
    ];
    if s_tier.iter().any(|d| host_matches_rule(&host, d)) {
        score += 30;
    }

    score
}

pub fn url_allowed(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };

    if !host_allowed(host) || path_looks_binary(url.path()) {
        return false;
    }

    let path = url.path();
    let lower_path = path.to_ascii_lowercase();

    if (host_matches_rule(host, "stackoverflow.com") && host != "stackoverflow.com")
        || (host_matches_rule(host, "research.google") && host != "research.google")
        || (host_matches_rule(host, "arxiv.org") && host != "arxiv.org")
        || (host_matches_rule(host, "usenix.org") && host != "www.usenix.org")
    {
        return false;
    }

    if host_matches_rule(host, "kernel.org")
        && !matches!(host, "www.kernel.org" | "docs.kernel.org")
    {
        return false;
    }

    if host_matches_rule(host, "haskell.org")
        && !matches!(
            host,
            "www.haskell.org" | "wiki.haskell.org" | "downloads.haskell.org"
        )
    {
        return false;
    }

    if lower_path.contains("/doxygen/")
        || lower_path.ends_with("-members.html")
        || lower_path.ends_with("_source.html")
    {
        return false;
    }

    let localized_prefixes = [
        "/zh", "/ja", "/ko", "/ru", "/uk", "/it", "/es", "/fr", "/de", "/pt", "/tr", "/pl", "/cs",
        "/nl", "/sv", "/fi", "/no", "/da", "/hu", "/ro",
    ];
    if localized_prefixes.iter().any(|p| {
        lower_path == *p
            || lower_path.starts_with(&format!("{p}/"))
            || lower_path.starts_with(&format!("{p}-"))
    }) {
        return false;
    }

    let blocked_lang_markers = [
        "locale=", "lang=", "/zh-cn/", "/zh-tw/", "/ja-jp/", "/ko-kr/", "/ru-ru/", "/uk-ua/",
        "/it-it/", "/es-ar/", "/fr-fr/", "/de-de/", "/pt-br/",
    ];
    let full = url.as_str().to_ascii_lowercase();
    if blocked_lang_markers.iter().any(|m| full.contains(m)) {
        return false;
    }

    if path_has_locale_segment(path) {
        return false;
    }

    if host == "en.wikipedia.org" {
        if path.starts_with("/w/") {
            return false;
        }
        if !path.starts_with("/wiki/") {
            return false;
        }
        if path.starts_with("/wiki/Special:") || path.starts_with("/wiki/Talk:") {
            return false;
        }

        if let Some(title) = path.strip_prefix("/wiki/") {
            let t = title.to_ascii_lowercase();
            let starts_with_digit = t.as_bytes().first().is_some_and(|b| b.is_ascii_digit());
            if t.contains(':')
                || starts_with_digit
                || t.contains("_in_")
                || t.contains("election")
                || t.contains("_season")
                || t.contains("_cup")
                || t.contains("_championship")
                || t.contains("_league")
                || t.contains("football")
                || t.contains("basketball")
                || t.contains("olympics")
            {
                return false;
            }

            if !wiki_topic_allowed(&t) {
                return false;
            }
        }
    }

    if host == "stackoverflow.com" {
        let ok_path = path.starts_with("/questions")
            || path.starts_with("/q/")
            || path.starts_with("/a/")
            || path.starts_with("/tags");
        if !ok_path {
            return false;
        }
    }

    if host_matches_rule(host, "cppreference.com") {
        if !matches!(host, "cppreference.com" | "en.cppreference.com") {
            return false;
        }
        if lower_path.starts_with("/mwiki/") || lower_path.starts_with("/w/special:") {
            return false;
        }
        if !lower_path.starts_with("/w/c/") && !lower_path.starts_with("/w/cpp/") {
            return false;
        }
    }

    if host_matches_rule(host, "aws.amazon.com") {
        let is_docs = host == "docs.aws.amazon.com";
        let is_blog = (host == "aws.amazon.com" || host == "www.aws.amazon.com")
            && lower_path.starts_with("/blogs/");
        if !is_docs && !is_blog {
            return false;
        }
    }

    if host_matches_rule(host, "databricks.com") {
        let is_docs = host == "docs.databricks.com";
        let is_blog = (host == "databricks.com" || host == "www.databricks.com")
            && lower_path.starts_with("/blog");
        if !is_docs && !is_blog {
            return false;
        }
    }

    if host_matches_rule(host, "medium.com") {
        if host != "medium.com" {
            return false;
        }
        let blocked_prefixes = [
            "/about",
            "/help",
            "/membership",
            "/m/signin",
            "/m/signout",
            "/search",
            "/tag/",
            "/topics",
            "/me",
            "/following",
            "/followers",
        ];
        if blocked_prefixes
            .iter()
            .any(|prefix| lower_path.starts_with(prefix))
            || lower_path.ends_with("/followers")
            || lower_path.ends_with("/following")
        {
            return false;
        }
    }

    if host == "git.kernel.org" {
        return false;
    }

    if host == "downloads.haskell.org" {
        let latest_docs =
            lower_path.starts_with("/ghc/latest/") || lower_path.starts_with("/~ghc/latest/");
        if !latest_docs {
            return false;
        }
        if lower_path.contains("/src/") || lower_path.contains("doc-index") {
            return false;
        }
    }

    if host == "gcc.gnu.org"
        && (lower_path.starts_with("/bugzilla/") || lower_path.starts_with("/legacy-ml/"))
    {
        return false;
    }

    let blocked_params = [
        "action",
        "oldid",
        "diff",
        "lastactivity",
        "answertab",
        "tab",
        "printable",
        "veaction",
        "search",
    ];

    for (k, v) in url.query_pairs() {
        let key = k.to_ascii_lowercase();
        let value = v.to_ascii_lowercase();
        if blocked_params.contains(&key.as_str()) {
            return false;
        }
        if key == "action" && (value == "edit" || value == "history") {
            return false;
        }
        if key.starts_with("utm_")
            || matches!(
                key.as_str(),
                "source" | "ref" | "ref_src" | "fbclid" | "gclid" | "mc_cid" | "mc_eid"
            )
        {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_allowlist_blocks_unwanted_subdomains() {
        assert!(host_allowed("docs.kernel.org"));
        assert!(host_allowed("docs.aws.amazon.com"));
        assert!(!host_allowed("community.grafana.com"));
        assert!(!host_allowed("reviews.llvm.org"));
        assert!(!host_allowed("commitfest.postgresql.org"));
    }

    #[test]
    fn url_policy_rejects_noisy_subdomains() {
        let noisy = Url::parse("https://discuss.elastic.co/t/some-thread").unwrap();
        let low_signal = Url::parse("https://commitfest.postgresql.org/42/").unwrap();
        let allowed = Url::parse("https://docs.kernel.org/core-api/index.html").unwrap();

        assert!(!url_allowed(&noisy));
        assert!(!url_allowed(&low_signal));
        assert!(url_allowed(&allowed));
    }
}
