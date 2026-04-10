# Modal workflow

Use Modal for bursty remote work only:
- remote build/test
- remote performance testing
- occasional full embedding / full index rebuilds

Do **not** keep the search server hosted on Modal if you want to stay near a low monthly budget.

## Goals

Keep local:
- crawling
- RocksDB source-of-truth corpus
- serving
- normal incremental updates

Use Modal only for:
- clean Linux build/test
- GPU embedding benchmarks
- query/ANN performance benchmarks
- full embed/index rebuilds that do not fit locally

## Files

- `modal_app.py` — staged Modal jobs
- `config.modal.toml` — remote config profile

## One-time setup

```bash
pip install modal
modal setup
```

## Volume names

Default volume names used by `modal_app.py`:
- `search-engine-data`
- `search-engine-results`

These store temporary remote DB/artifacts/results. Delete or clear them when done if you want to minimize storage costs.

## Recommended low-cost usage

### 1. Remote build/test only

```bash
modal run modal_app.py --action tests
```

This runs release builds and tests on CPU only.

### 2. Upload local DB snapshot only when needed

```bash
modal run modal_app.py --action sync_db --local-db ./crawl_data
```

This copies your local RocksDB snapshot to the Modal data volume.

### 3. Remote embedding throughput benchmark

```bash
modal run modal_app.py --action bench_embed --samples 2000
```

Writes results into the results volume.

### 4. Remote query latency benchmark

Requires a built lexical/vector index in the data volume.

```bash
modal run modal_app.py --action bench_query
```

### 5. Remote ANN benchmark

```bash
modal run modal_app.py --action bench_ann
```

Warning: `bench_ann` loads embeddings into memory and builds a brute-force baseline in-process. It is best used on a reduced corpus snapshot, not the full 4.6M-vector dataset.

### 6. Remote full embedding rebuild

```bash
modal run modal_app.py --action embed_full
```

### 7. Remote full HNSW rebuild

```bash
modal run modal_app.py --action index_full
```

### 8. Remote lexical rebuild

```bash
modal run modal_app.py --action lexical_full
```

## Suggested rebuild sequence

```bash
modal run modal_app.py --action sync_db --local-db ./crawl_data
modal run modal_app.py --action embed_full
modal run modal_app.py --action index_full
modal run modal_app.py --action lexical_full
```

## Pulling artifacts/results back home

Use the Modal CLI to download from the volumes.

Examples:

```bash
modal volume ls
modal volume get search-engine-data /hnsw_index.bin ./modal_restore_tmp/hnsw_index.bin
modal volume get search-engine-data /hnsw_index.bin.hnsw.data ./modal_restore_tmp/hnsw_index.bin.hnsw.data
modal volume get search-engine-data /hnsw_index.bin.hnsw.graph ./modal_restore_tmp/hnsw_index.bin.hnsw.graph
modal volume get search-engine-data /lexical_index ./modal_restore_tmp/lexical_index
modal volume get search-engine-results /reports ./modal_restore_tmp/reports
```

Then move them into your local project paths.

## Cleaning remote storage

To avoid ongoing storage costs, clear remote data after successful download.

```bash
modal run modal_app.py --action clear_results
modal run modal_app.py --action clear_data_artifacts
```

`clear_data_artifacts` removes derived artifacts but keeps `/data/crawl_data`.

## Cost discipline

To stay around a low monthly budget:

- do not deploy the web server on Modal
- do not schedule recurring jobs
- do not autoscale GPU workers
- use exactly one GPU worker for embedding jobs
- use CPU-only jobs for build/test/index/lexical unless profiling says otherwise
- only sync the DB when you actually need a remote rebuild or benchmark
- delete remote artifacts/results when done

## Notes

- `config.modal.toml` is copied over `config.toml` inside the remote workspace for Modal jobs only.
- Remote jobs mount the current local workspace snapshot, so `--git-ref` is currently ignored.
- If you want reproducible remote runs, commit the workspace state you want before launching the job.
