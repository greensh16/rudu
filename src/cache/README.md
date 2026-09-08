# Cache Module

Disk-based caching for incremental scans: rudu stores metadata about scanned
directories **and files** so that unchanged subtrees can be skipped on
subsequent runs.

## How it works

- Cache files are bincode-serialized `Cache` structures (a `CacheHeader` plus
  a map of path-hash → `CacheEntry`), written atomically (temp file + rename).
- Files live in a configurable cache directory — `RUDU_CACHE_DIR` if set,
  otherwise the XDG cache directory (`~/.cache/rudu/`) — **never** inside the
  scanned tree, since writing there would perturb the mtimes the cache
  validates against. One file per scanned root, named by the FNV-1a hash of
  the root path.
- A subtree is only reused as a cache hit if the directory *and every cached
  directory beneath it* still match their recorded nanosecond mtime and nlink.
  This catches structural changes (create/delete/rename) at any depth. The
  scan root itself is never treated as a hit.
- In-place modification of an existing file changes no directory mtime and is
  therefore only picked up when the TTL expires (default 7 days,
  `--cache-ttl`) or with `--no-cache`.

## Access times

`CacheEntry` stores each entry's raw `atime`, but never a derived rollup: the
`--purge-days` threshold can change between runs, so directory rollups
(oldest leaf, at-risk bytes) are recomputed from restored entries every time.
atime is cached even when `--show-atime` was not requested, so a cache built by
a plain run still serves a later access-age run.

A restored atime can be older than reality — reading a file updates its atime
but not its parent's mtime, so the subtree still validates as a hit. Since atime
only moves forward, this over-states age and therefore purge risk, and never
reports at-risk data as safe.

## Invalidation

The whole cache is discarded when any of these mismatch: rudu version, TTL,
root path, or the root directory's own mtime. Individual subtrees are
rescanned when their directory metadata changes.

## API

- `load_cache(root, ttl_seconds) -> HashMap<PathBuf, CacheEntry>` — returns an
  empty map if the cache is missing, invalid, or disabled.
- `save_cache(root, &cache) -> Result<()>` /
  `save_cache_with_mtime(root, &cache, root_mtime) -> Result<()>`
- `invalidate_cache(root) -> Result<bool>` — removes the cache file.
- `set_enabled(bool)` / `is_enabled()` — runtime toggle, used to shed work
  when nearing a `--memory-limit`.

## Dependencies

- **bincode** — binary serialization
- **anyhow** — error handling
- **tempfile** — testing (dev-dependency)

Run tests with:

```bash
cargo test cache
```
