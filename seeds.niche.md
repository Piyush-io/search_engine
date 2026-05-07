# Niche Seeds: Database Internals And Transaction Systems

This seed set is intentionally narrow.

Goal:
- build a high-signal corpus for database internals
- stress lexical retrieval on exact identifiers, acronyms, and config keys
- avoid broad web noise that can hide ranking signal

## PostgreSQL Core Docs

https://www.postgresql.org/docs/current/
https://www.postgresql.org/docs/current/mvcc.html
https://www.postgresql.org/docs/current/transaction-iso.html
https://www.postgresql.org/docs/current/wal-intro.html
https://www.postgresql.org/docs/current/routine-vacuuming.html
https://www.postgresql.org/docs/current/storage-page-layout.html
https://www.postgresql.org/docs/current/indexes-types.html

## SQLite Core Docs

https://sqlite.org/docs.html
https://sqlite.org/wal.html
https://sqlite.org/lockingv3.html
https://sqlite.org/lang_transaction.html
https://sqlite.org/queryplanner.html

## Database Internals Courses And References

https://www.interdb.jp/pg/
https://15445.courses.cs.cmu.edu/fall2024/
https://15721.courses.cs.cmu.edu/spring2024/

## Distributed Systems Validation And Field Reports

https://jepsen.io/analyses
https://martin.kleppmann.com/
https://muratbuffalo.blogspot.com/
https://www.cockroachlabs.com/blog/
https://www.usenix.org/publications/proceedings/

## Missing Canonical URLs (from qrels audit)

https://www.postgresql.org/docs/current/wal-archiving.html
https://www.postgresql.org/docs/current/indexes-scanning.html
https://www.postgresql.org/docs/current/indexes-hash.html
https://www.postgresql.org/docs/current/streaming-replication.html
https://www.postgresql.org/docs/current/gin-intro.html
https://www.postgresql.org/docs/high-availability/log-shipping.html
https://www.postgresql.org/docs/current/runtime-config-wal.html
https://www.postgresql.org/docs/current/runtime-config-resource.html
https://www.postgresql.org/docs/current/runtime-config-connection.html
https://www.postgresql.org/docs/current/wal-configuration.html
https://sqlite.org/datatype3.html
https://sqlite.org/withoutrowid.html
https://sqlite.org/c3ref/autovacuum_pages.html
https://wiki.postgresql.org/wiki/Work_mem
https://wiki.postgresql.org/wiki/Maintenance_work_mem
https://wiki.postgresql.org/wiki/Tuning_Your_PostgreSQL_Server
https://wiki.postgresql.org/wiki/PgBouncer
https://wiki.postgresql.org/wiki/Number_Of_Database_Connections

## Notes

- Keep this corpus narrow for the first pass.
- Expand only after metrics improve on niche query sets.
- Prefer stable docs and primary technical sources over broad community content.
