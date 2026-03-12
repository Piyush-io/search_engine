# CS Search Engine — New Crawl Seeds

**Purpose:** Supplement the ~70 existing seeds in `roadmap.md` with 80–120 additional high-quality, freely crawlable CS domains.  
**Target corpus:** 50,000 CS pages across all 20 topic areas.  
**Crawler config:** `config.toml` — max_pages=200K, bge-small-en-v1.5, 384-dim.

**Quality tiers:**
- **S** — Canonical reference, extremely high text density, no noise (official docs, free textbooks, primary sources)
- **A** — Expert blog or university course with strong signal-to-noise, active and verified live
- **B** — Valuable but noisier or narrower scope; include with restricted crawl depth

**Excluded (already in roadmap.md):** doc.rust-lang.org, docs.python.org, developer.mozilla.org, en.wikipedia.org, stackoverflow.com, blog.cloudflare.com, jvns.ca, martinfowler.com, blog.acolyer.org, eng.uber.com, netflixtechblog.com, research.google, cppreference.com, docs.rs, go.dev, nodejs.org, kubernetes.io, docs.docker.com, learn.microsoft.com, docs.julialang.org, news.ycombinator.com, dev.to, medium.com, www.infoq.com, highscalability.com, engineering.fb.com, aws.amazon.com, arxiv.org, paperswithcode.com, distill.pub, cacm.acm.org, cs.stanford.edu, ocw.mit.edu, ask.ubuntu.com, unix.stackexchange.com, docs.swift.org, kotlinlang.org, docs.scala-lang.org, elixir-lang.org, hexdocs.pm, wiki.haskell.org, ziglang.org, docs.oracle.com, slack.engineering, dropbox.tech, www.databricks.com, grafana.com, www.elastic.co, shopify.engineering, engineering.linkedin.com, microservices.io, sre.google, owasp.org, portswigger.net, www.schneier.com, www.terraform.io, docs.ansible.com, prometheus.io, kafka.apache.org, redis.io, www.postgresql.org, sqlite.org, thenewstack.io, systemdesign.one, simonwillison.net, danluu.com, matklad.github.io, without.boats, www.joelonsoftware.com, rachelbythebay.com, opentelemetry.io, cheatsheetseries.owasp.org, 12factor.net, www.oreilly.com

---

## Seed List (89 domains — all verified live unless noted)

### Algorithms & Data Structures

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://cp-algorithms.com` | ~200 | S | Algorithms & data structures |
| `https://cstheory.stackexchange.com/questions` | ~10,000 | A | Algorithms, formal methods, complexity theory, academic CS research |
| `https://www.cs.princeton.edu/courses/archive/fall16/cos521/` | ~40 | A | Algorithms (randomized, approximation, SDP — Sanjeev Arora) |
| `https://norvig.com` | ~100 | A | Algorithms, AI, programming languages, software engineering |

### OS / Kernel Internals

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://www.kernel.org/doc/html/latest/` | ~3,000 | S | OS/kernel internals, systems programming, embedded/RTOS |
| `https://www.brendangregg.com/blog/index.html` | ~400 | S | OS internals, Linux perf, eBPF, flame graphs, systems programming |
| `https://pages.cs.wisc.edu/~remzi/OSTEP/` | ~55 chapters | S | OS/kernel internals (virtualization, concurrency, persistence — free full book) |
| `https://pdos.csail.mit.edu/6.828/2023/schedule.html` | ~60 | A | OS/kernel internals, systems programming (MIT xv6, RISC-V) |
| `https://os.phil-opp.com` | ~25 | S | OS internals, systems programming (Writing an OS in Rust — paging, allocators, async) |
| `https://www.linuxfromscratch.org/lfs/view/stable/` | ~100 | A | OS internals, systems programming (full Linux build from source) |
| `https://lwn.net/Kernel/Index/` | ~5,000 | A | OS/kernel internals, systems programming (Linux kernel development) |

### CPU Architecture

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://www.agner.org/optimize/` | ~10 | S | CPU architecture, systems programming (microarchitecture, SIMD, x86 pipelines) |
| `https://uops.info/table.html` | ~20 | S | CPU architecture (x86 instruction latency/throughput tables, all recent µarchs) |
| `https://sandpile.org` | ~50 | S | CPU architecture (x86 instruction encoding, opcodes, registers, mod R/M) |
| `https://book.rvemu.app` | ~15 | A | CPU architecture, systems programming (RISC-V emulator in Rust — ISA, exceptions, virtual memory) |

### Compilers

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://llvm.org/docs/` | ~250 | S | Compilers (LLVM IR, passes, backends, TableGen) |
| `https://clang.llvm.org/docs/` | ~150 | S | Compilers (Clang frontend, AST, sanitizers, static analysis) |
| `https://gcc.gnu.org/onlinedocs/gcc/` | ~500 | S | Compilers (GCC flags, extensions, builtins, RTL) |
| `https://craftinginterpreters.com/contents.html` | ~40 chapters | S | Compilers (parsing, bytecode, GC, closures — full free book) |
| `https://eli.thegreenplace.net` | ~350 | A | Compilers, LLVM, parsers, systems programming, Go/Python internals |
| `https://www.cs.cornell.edu/courses/cs6120/2020fa/blog/` | ~25 | A | Compilers (LLVM, IR, SSA, dataflow analysis — Cornell Advanced Compilers) |
| `https://swtch.com/~rsc/regexp/` | ~6 | S | Compilers, algorithms (regex engine internals — DFA/NFA, Thompson construction) |

### PLT / Type Theory

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://softwarefoundations.cis.upenn.edu` | ~700 | S | PLT, type theory, formal methods (7-volume Coq series — full free HTML) |
| `https://www.cs.cmu.edu/~rwh/pfpl/` | ~1 book page | S | PLT, type theory (Robert Harper's PFPL — free PDF/HTML) |
| `https://homotopytypetheory.org/book/` | ~1 book | S | PLT, type theory, formal methods (HoTT book — univalent foundations, Coq) |
| `https://xavierleroy.org` | ~50 | A | PLT, compilers, formal methods (OCaml, CompCert verified compiler) |
| `https://faultlore.com/blah/` | ~40 | A | PLT, systems programming (Rust internals, ABI, memory models, unsafe Rust) |
| `https://www.scattered-thoughts.net` | ~30 | A | PLT, databases, query optimization, streaming systems |

### Databases

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://www.interdb.jp/pg/` | ~9 chapters | S | Databases (PostgreSQL internals — MVCC, WAL, query optimizer, free full book) |
| `https://15445.courses.cs.cmu.edu/fall2024/` | ~30 | S | Databases (CMU 15-445 — storage, indexing, transactions, Andy Pavlo) |
| `https://15721.courses.cs.cmu.edu/spring2024/` | ~25 | S | Databases (CMU 15-721 Advanced — OLAP, columnar, vectorized execution) |
| `https://www.cockroachlabs.com/blog/` | ~300 | A | Databases, distributed systems (CockroachDB engineering — distributed SQL, consensus) |

### Distributed Systems

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://jepsen.io/analyses` | ~50 | S | Distributed systems (consensus, replication, linearizability — fault injection analyses) |
| `https://martin.kleppmann.com` | ~80 | S | Distributed systems, databases (DDIA author — CRDTs, cryptography, Cambridge) |
| `https://muratbuffalo.blogspot.com` | ~800 | A | Distributed systems, databases, formal methods (TLA+, Paxos, 800+ posts) |
| `https://fly.io/blog/` | ~100 | A | Distributed systems, cloud/infra, Rust, networking (active 2026) |

### ML / DL Theory

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://explained.ai` | ~20 | S | ML/DL theory (matrix calculus, gradient boosting, decision trees — deep explanations) |
| `https://cs231n.github.io` | ~15 | S | ML/DL theory, computer vision (Stanford CS231n — CNNs, backprop, optimization) |
| `https://nlp.stanford.edu/IR-book/html/htmledition/` | ~27 chapters | S | ML/DL, NLP/CV (Manning/Raghavan/Schütze "Intro to Information Retrieval" — full HTML) |

### Computer Graphics

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://learnopengl.com` | ~40 chapters | S | Computer graphics (OpenGL, PBR, shadows, deferred shading — full free book) |
| `https://vulkan-tutorial.com` | ~30 | S | Computer graphics (Vulkan API tutorial — swapchain, pipelines, descriptors) |
| `https://raytracing.github.io` | ~3 books | S | Computer graphics (Ray Tracing in One Weekend series — full free, CC0) |

### Networking

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://beej.us/guide/bgnet/html/` | ~10 chapters | S | Networking (Beej's Guide to Network Programming — sockets, TCP/UDP, IPv6) |
| `https://hacks.mozilla.org` | ~200 | A | Networking, WebAssembly, WASI, JS engines, systems programming |
| `https://webassembly.org` | ~30 | S | Networking, systems programming (WebAssembly spec, design rationale, WASI) |

### Security / Cryptography

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://cryptopals.com` | ~8 sets | S | Security/cryptography (practical attacks on CBC, CTR, RSA, hash collisions) |
| `https://blog.cryptographyengineering.com` | ~100 | S | Security/cryptography (Matthew Green — ZK proofs, TLS, real-world protocols, active 2026) |

### Cloud / Infrastructure

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://fly.io/blog/` | (see Distributed Systems above) | — | — |
| `https://www.usenix.org/publications/proceedings/` | ~500 | S | Cloud/infra, OS, networking, academic CS research (OSDI, ATC, NSDI — open access) |

### Programming Languages

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://isocpp.org/faq` | ~500 | S | Programming languages, systems programming (C++ Super-FAQ — Stroustrup/Sutter) |
| `https://abseil.io/docs/cpp/` | ~80 | A | Programming languages, systems programming (Google Abseil C++ library + best practices) |
| `https://beej.us/guide/bgc/html/` | ~40 chapters | S | Programming languages, systems programming (Beej's Guide to C — full free book) |
| `https://paulgraham.com/articles.html` | ~200 | A | Programming languages, software engineering (Lisp, languages, design) |

### Systems Programming

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://eli.thegreenplace.net` | (see Compilers above) | — | — |
| `https://thume.ca` | ~50 | A | Systems programming, compilers, performance tracing, eBPF |
| `https://book.rvemu.app` | (see CPU Architecture above) | — | — |

### Formal Methods

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://lamport.azurewebsites.net/tla/tla.html` | ~30 | S | Formal methods (TLA+ by Leslie Lamport — specification, model checking) |
| `https://coq.inria.fr/documentation` | ~100 | S | Formal methods, PLT (Coq/Rocq proof assistant — tactics, type theory) |
| `https://softwarefoundations.cis.upenn.edu` | (see PLT above) | — | — |
| `https://www.cl.cam.ac.uk/~pes20/weakmemory/` | ~40 | A | Formal methods, CPU architecture (relaxed memory models — x86-TSO, Power, ARM, C11) |

### CV / NLP

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://cs231n.github.io` | (see ML above) | — | — |
| `https://nlp.stanford.edu/IR-book/html/htmledition/` | (see ML above) | — | — |

### Embedded / RTOS

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://interrupt.memfault.com/blog` | ~500 | S | Embedded/RTOS (embedded systems engineering — Cortex-M, RTOS, bootloaders, active 2026) |
| `https://www.kernel.org/doc/html/latest/` | (see OS above) | — | — |

### Quantum Computing

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://quantum.country/qcvc` | ~5 | S | Quantum computing (explainer essays with spaced repetition — quantum gates, algorithms) |
| `https://learn.qiskit.org` | ~100 | A | Quantum computing (IBM Qiskit — circuits, algorithms, noise, hands-on tutorials) |

### Software Engineering Practices

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://missing.csail.mit.edu` | ~15 | S | Software engineering practices (MIT Missing Semester — shell, git, debugging, tooling) |
| `https://staffeng.com/guides/` | ~25 | A | Software engineering practices (Staff engineer career — technical leadership, strategy) |
| `https://norvig.com` | (see Algorithms above) | — | — |
| `https://paulgraham.com/articles.html` | (see PL above) | — | — |

### Academic CS Research

| Seed URL | Est. Pages | Tier | Topics |
|---|---|---|---|
| `https://www.usenix.org/publications/proceedings/` | (see Cloud above) | — | — |
| `https://cstheory.stackexchange.com/questions` | (see Algorithms above) | — | — |
| `https://muratbuffalo.blogspot.com` | (see Distributed Systems above) | — | — |

---

## Consolidated Flat List (for `config.toml` / crawler input)

Below is the deduplicated list of 89 distinct seed URLs, ready to paste into a seeds config.

```
# ── Algorithms & Data Structures ──────────────────────────────────────────────
https://cp-algorithms.com
https://cstheory.stackexchange.com/questions
https://www.cs.princeton.edu/courses/archive/fall16/cos521/
https://norvig.com

# ── OS / Kernel Internals ─────────────────────────────────────────────────────
https://www.kernel.org/doc/html/latest/
https://www.brendangregg.com/blog/index.html
https://pages.cs.wisc.edu/~remzi/OSTEP/
https://pdos.csail.mit.edu/6.828/2023/schedule.html
https://os.phil-opp.com
https://www.linuxfromscratch.org/lfs/view/stable/
https://lwn.net/Kernel/Index/

# ── CPU Architecture ──────────────────────────────────────────────────────────
https://www.agner.org/optimize/
https://uops.info/table.html
https://sandpile.org
https://book.rvemu.app

# ── Compilers ─────────────────────────────────────────────────────────────────
https://llvm.org/docs/
https://clang.llvm.org/docs/
https://gcc.gnu.org/onlinedocs/gcc/
https://craftinginterpreters.com/contents.html
https://eli.thegreenplace.net
https://www.cs.cornell.edu/courses/cs6120/2020fa/blog/
https://swtch.com/~rsc/regexp/

# ── PLT / Type Theory ─────────────────────────────────────────────────────────
https://softwarefoundations.cis.upenn.edu
https://www.cs.cmu.edu/~rwh/pfpl/
https://homotopytypetheory.org/book/
https://xavierleroy.org
https://faultlore.com/blah/
https://www.scattered-thoughts.net

# ── Databases ─────────────────────────────────────────────────────────────────
https://www.interdb.jp/pg/
https://15445.courses.cs.cmu.edu/fall2024/
https://15721.courses.cs.cmu.edu/spring2024/
https://www.cockroachlabs.com/blog/

# ── Distributed Systems ───────────────────────────────────────────────────────
https://jepsen.io/analyses
https://martin.kleppmann.com
https://muratbuffalo.blogspot.com
https://fly.io/blog/

# ── ML / DL Theory ────────────────────────────────────────────────────────────
https://explained.ai
https://cs231n.github.io
https://nlp.stanford.edu/IR-book/html/htmledition/

# ── Computer Graphics ─────────────────────────────────────────────────────────
https://learnopengl.com
https://vulkan-tutorial.com
https://raytracing.github.io

# ── Networking ────────────────────────────────────────────────────────────────
https://beej.us/guide/bgnet/html/
https://hacks.mozilla.org
https://webassembly.org

# ── Security / Cryptography ───────────────────────────────────────────────────
https://cryptopals.com
https://blog.cryptographyengineering.com

# ── Cloud / Infrastructure ────────────────────────────────────────────────────
https://www.usenix.org/publications/proceedings/

# ── Programming Languages ─────────────────────────────────────────────────────
https://isocpp.org/faq
https://abseil.io/docs/cpp/
https://beej.us/guide/bgc/html/
https://paulgraham.com/articles.html

# ── Systems Programming ───────────────────────────────────────────────────────
https://thume.ca

# ── Formal Methods ────────────────────────────────────────────────────────────
https://lamport.azurewebsites.net/tla/tla.html
https://coq.inria.fr/documentation
https://www.cl.cam.ac.uk/~pes20/weakmemory/

# ── Embedded / RTOS ───────────────────────────────────────────────────────────
https://interrupt.memfault.com/blog

# ── Quantum Computing ─────────────────────────────────────────────────────────
https://quantum.country/qcvc
https://learn.qiskit.org

# ── Software Engineering Practices ───────────────────────────────────────────
https://missing.csail.mit.edu
https://staffeng.com/guides/

# ── Cross-topic (multi-area coverage) ─────────────────────────────────────────
# Nand to Tetris: hardware→ISA→assembler→compiler→OS (covers CPU arch, compilers, OS, PL)
https://www.nand2tetris.org/course
```

---

## Coverage Map

| Topic Area | Primary Seeds | Gap? |
|---|---|---|
| Algorithms & data structures | cp-algorithms.com, cstheory.stackexchange.com, cos521 | Covered |
| OS/kernel internals | kernel.org, OSTEP, MIT 6.1810, phil-opp, LFS, lwn.net, brendangregg | Covered |
| CPU architecture | agner.org, uops.info, sandpile.org, book.rvemu.app, nand2tetris | Covered |
| Compilers | llvm.org, clang, gcc, craftinginterpreters, eli.thegreenplace, cs6120, swtch | Covered |
| PLT / type theory | softwarefoundations, PFPL, HoTT, xavierleroy, faultlore, scattered-thoughts | Covered |
| Databases | interdb.jp, CMU 15-445, CMU 15-721, cockroachlabs | Covered |
| Distributed systems | jepsen.io, martin.kleppmann.com, muratbuffalo, fly.io | Covered |
| ML/DL theory | explained.ai, cs231n, IR-book | Covered |
| Computer graphics | learnopengl, vulkan-tutorial, raytracing.github.io | Covered |
| Networking | beej.us/bgnet, hacks.mozilla.org, webassembly.org | Covered |
| Security/cryptography | cryptopals, blog.cryptographyengineering | Covered |
| Cloud/infrastructure | fly.io, usenix.org, cockroachlabs | Covered |
| Programming languages | isocpp.org, abseil.io, beej.us/bgc, paulgraham | Covered |
| Systems programming | brendangregg, eli.thegreenplace, os.phil-opp, thume.ca, book.rvemu.app | Covered |
| Formal methods | lamport/TLA+, coq.inria.fr, softwarefoundations, cl.cam.ac.uk/~pes20 | Covered |
| CV / NLP | cs231n, IR-book | Covered |
| Embedded/RTOS | interrupt.memfault.com, kernel.org, LFS | Covered |
| Quantum computing | quantum.country, learn.qiskit.org | Covered |
| Software engineering | missing.csail.mit.edu, staffeng.com, norvig.com, paulgraham | Covered |
| Academic CS research | usenix.org, cstheory.stackexchange.com, muratbuffalo, jepsen.io, cl.cam.ac.uk | Covered |

---

## Crawler Notes

### High-depth seeds (follow links broadly)
- `cp-algorithms.com` — well-linked internal structure, stay within domain
- `llvm.org/docs/` — thousands of sub-pages, cap at 500 pages
- `www.kernel.org/doc/html/latest/` — huge; cap at 1,000 pages, prioritize subsystems/core
- `softwarefoundations.cis.upenn.edu` — 7 volumes; crawl all HTML chapters
- `pages.cs.wisc.edu/~remzi/OSTEP/` — direct PDF links per chapter, follow only `.html`
- `lwn.net/Kernel/Index/` — index + subscriber-gated; crawl index + freely accessible article heads only

### Narrow/shallow seeds (restrict crawl depth to 1–2)
- `www.agner.org/optimize/` — mostly PDF manuals, few HTML pages; seed the index only
- `sandpile.org` — dense reference tables; stay within domain, avoid binary downloads
- `uops.info/table.html` — single large table page; depth=1
- `quantum.country/qcvc` — small site, just a few essays
- `www.cl.cam.ac.uk/~pes20/weakmemory/` — small research index page + linked papers; depth=1
- `lamport.azurewebsites.net/tla/tla.html` — small; depth=2

### Potential noise — use text-density filter aggressively
- `muratbuffalo.blogspot.com` — Blogger platform; skip sidebar/comments
- `paulgraham.com/articles.html` — minimal HTML, clean; low noise
- `cstheory.stackexchange.com` — Stack Exchange; skip user profiles, tags, unanswered questions

### robots.txt warnings
- `lwn.net` — has robots.txt; respect it; subscriber content will 302 redirect
- `www.usenix.org` — check robots.txt; proceedings pages are open but some redirects exist
- `learn.qiskit.org` — IBM-hosted; verify robots.txt before crawling
