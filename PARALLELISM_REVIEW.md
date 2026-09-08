# rudu — Parallelism Review (July 2026)

Focused review of the concurrency architecture at v1.4.10 (post-CODE_REVIEW.md fixes): thread-pool configuration, both scan paths, shared-state design, and synchronization primitives. Findings ordered by expected impact. No data races or deadlocks were found — the safety story is sound; the issues are about how much of the machine the design can actually use.

**Summary:** rudu's parallelism is a fork-join pipeline over a *sequentially collected* entry list. The directory walk — the phase that dominates cold-cache wall time — runs on one thread, so Amdahl's law caps speedup regardless of pool size. Within the parallel phases, the main costs are DashMap traffic keyed by full `PathBuf`s and shard contention on hot ancestor keys. The "work-stealing" strategy largely re-implements what rayon's scheduler already does. Several concurrent data structures are used from purely sequential code.

---

## Architecture as built

Both scan paths share this shape:

```
WalkDir traversal (SEQUENTIAL, 1 thread)  ← readdir + filter + cache checks
        ↓ collect Vec<DirEntry>
lstat per entry   (PARALLEL, par_iter)    ← one syscall per entry
        ↓
aggregation       (SEQUENTIAL, incremental path)
                  (PARALLEL w/ DashMap, work-stealing path)
        ↓
FileEntry build   (PARALLEL, par_iter)
        ↓
sort              (SEQUENTIAL)
```

The pool is rayon's global one, configured once in `setup_thread_pool` (`src/main.rs`) via `configure_pool` (`src/thread_pool.rs`). Strategies map to thread counts: `Default`/`WorkStealingUneven` → N cores, `NumCpusMinus1` → N-1, `IOHeavy` → 2N, `Fixed`/`--threads` → exact.

---

## P1. The traversal is single-threaded, and it is the bottleneck

`WalkDir` is a sequential iterator. In the incremental path (`src/scan.rs`, `scan_files_and_dirs_with_monitor`) the walk additionally performs, inline on that one thread: glob matching, per-component exclude comparison, cache lookups, `get_dir_metadata` stats for every candidate directory, and — since the C1 fix — `validate_cached_subtree`, which re-stats every cached directory under each candidate hit. The work-stealing path's walk (`scan_with_work_stealing`) is lighter but equally serial.

On a cold cache the scan's wall time is dominated by `readdir` + first-touch metadata I/O, all of which happens here before any rayon worker gets involved. The parallel lstat phase then re-visits entries whose dentries/inodes the walk just pulled into the page cache — meaning the *parallel* phase runs against a warm cache and the *sequential* phase paid the actual I/O cost. This inverts where the concurrency should be.

Options, in increasing order of effort: (a) swap `walkdir` for `jwalk`, which parallelizes traversal on rayon and preserves per-directory ordering — `filter_entry`-style pruning and the cache-hit check can move into its `process_read_dir` callback; (b) hand-rolled parallel walk: seed a work queue with the root, workers `readdir` directories and push subdirectories (this is what `fd` and `dust` do via `ignore::WalkParallel`-style designs); (c) keep walkdir but move per-entry filtering/stats out of the walk thread. Option (a) is the pragmatic one; the cache-hit subtree skip maps naturally to "don't descend" decisions in the callback.

The C1 `validate_cached_subtree` cost deserves its own note: it runs on the walk thread, serially, once per candidate hit. Its stats are independent and could be a `par_iter` over the cached dir list — but only after the walk itself is off the critical path; parallelizing validation inside a sequential walk just moves the wait.

## P2. The "work-stealing" strategy mostly re-implements rayon

`--threads-strategy work-stealing-uneven` exists to handle skewed trees (a few directories with >10k children). Its mechanism: group entries of large directories, `scope.spawn` a task per large directory, and run the remainder as a filtered `par_iter` (`src/scan.rs`, `scan_with_work_stealing`).

But every unit of work here is a single independent entry. A flat `walker_entries.par_iter().for_each(accumulate)` gives rayon's scheduler exactly the same stealable granularity — its adaptive splitting already balances skewed workloads of uniform items; that is the point of work stealing. The spawn-per-large-dir layer adds an O(n) grouping pass, a `HashMap<PathBuf, Vec<usize>>`, and scheduling structure without changing what can be stolen. The strategy's value would need to come from *traversal-level* parallelism (P1), not accumulation-level — accumulation was never the skewed part.

Recommendation: either fold this strategy into a flat `par_iter` (deleting ~60 lines) and reserve the flag name for a future parallel-traversal implementation, or benchmark it honestly against the flat version with `work_stealing_benchmark.rs` and keep whichever wins. My expectation is the flat version ties or wins.

## P3. Ancestor accumulation contends on hot DashMap shards (work-stealing path)

In `accumulate`, every non-directory entry walks its parent chain and does `dir_totals.entry(ancestor).and_modify(...)` per level. Two problems compound under parallelism:

- **Hot keys.** Every file in the tree updates the root's entry (and near-root ancestors). All threads therefore serialize on the same few DashMap shards at the top of the tree. The deeper and wider the tree, the more of the total op count lands on the same shard locks.
- **Repeated hashing and cloning.** Each update hashes a full `PathBuf` (O(path length)) and the miss path allocates a `to_path_buf()` clone. For a file at depth d that is d hashes + up to d clones, giving O(n·d) map operations of O(path-length) each.

The incremental path already demonstrates the fix: stat in parallel, aggregate sequentially afterward (a single-threaded pass over `scan_jobs` walking parent chains against a plain map has no contention at all and is cache-friendly). Alternatively, thread-local `HashMap` accumulation with a merge at scope end (`fold`/`reduce` in rayon) keeps aggregation parallel without shared-shard traffic. Either beats per-entry shared-map updates.

## P4. Concurrent data structures used from purely sequential code (incremental path) ✅ Fixed in 1.4.10

*Fix: `dir_totals`, `directory_children`, and `restored_entries` are plain `HashMap`s in the incremental path; `restore_subtree` takes `&mut` references. The work-stealing path keeps its DashMaps — its writes are genuinely concurrent.*

After the 1.4.10 restructuring, the incremental path's `dir_totals`, `directory_children`, and `restored_entries` are **only ever written sequentially** — from `filter_entry` (which runs inside the single-threaded walk loop), `restore_subtree` (called from it), and the sequential aggregation loop. Their only concurrent use is read-only, from the `scanned_entries` par_iter.

They are all `DashMap`s. Every access pays sharding, per-shard locking, and DashMap's entry API overhead for zero concurrency benefit; read-only sharing of a plain `HashMap` across a `par_iter` is free (`&HashMap: Sync`). Switching to `HashMap` (and adjusting `restore_subtree`'s signature) is a mechanical change that removes lock traffic from the hottest loops in the default path. This is the cheapest win in this review.

## P5. The UID cache is a global `Mutex` hit once per entry under `--show-owner`

`resolve_uid` (`src/utils.rs`) takes `UID_CACHE.lock()` on every call — including cache hits — and it is called from inside both parallel FileEntry-construction loops. With `--show-owner` on a large tree, N entries × 2 lock acquisitions (read attempt + write-back on miss) funnel through one `std::sync::Mutex`. Critical sections are short, so this is contention rather than serialization, but it is measurable at high core counts.

Cheap fixes, pick one: an `RwLock` (reads dominate after the first few entries — real trees have a handful of distinct UIDs), a `DashMap<u32, String>`, or best: resolve each distinct UID once *before* the parallel loop (collect distinct UIDs from metadata — typically < 10 — then hand the par_iter a read-only `HashMap<u32, String>`). The last removes synchronization from the hot loop entirely. A related benign quirk: on startup, many threads can miss simultaneously and all call `getpwuid_r` for the same UID (thundering herd); harmless, but the pre-resolution approach eliminates it too.

## P6. Final sort is sequential; rayon is already a dependency ✅ Fixed in 1.4.10

*Fix: `sort_entries` uses `par_sort_by` (parallel stable sort, same ordering guarantees).*

`sort_entries` uses `slice::sort_by`. For multi-million-entry results the sort is a visible tail phase on one core. Rayon's `par_sort_by` / `par_sort_unstable_by` is a drop-in replacement (`src/utils.rs:71-76`). Sorting by size (u64 compare) parallelizes near-linearly; by path it still helps. One-line change, worth it above ~100k entries, harmless below.

## P7. Pool configuration nits

- `configure_pool` uses `build_global()`, which errors if the global pool was already initialized — fine for the binary (called once, before any rayon use), but library consumers or tests that scan twice with different strategies get `Err("Failed to configure thread pool")` from a `build_global` double-call. Consider `ThreadPoolBuilder::build()` into an owned pool + `pool.install(|| scan(...))` if the library API is meant to support per-call thread counts; otherwise document that thread configuration is process-global and first-wins.
- `IOHeavy` (2× cores) is a reasonable heuristic while workers block in `lstat`, but note that once P1 is addressed (parallel traversal), oversubscription value shrinks — the walk threads become the I/O concurrency. Revisit after.
- `Default` returns early without touching the builder, relying on rayon's lazy default pool — correct, but it means `RAYON_NUM_THREADS` env config silently works for `Default` and is silently overridden for every other strategy. Worth one line in `--help`.

## P8. Small frictions

- **Progress ticks** ✅ *fixed in 1.4.10 — all per-entry `pb.tick()` calls removed; the steady tick alone animates the spinner.* `pb.tick()` was called per entry inside the walk loops even though `enable_steady_tick` already animates the spinner from its own thread; each manual tick was an atomic RMW on the walk thread.
- **`entry_owners: DashMap<PathBuf, u32>`** in the work-stealing path clones every path a second time when `--show-owner` is set. If P3's thread-local accumulation happens, owners ride along in the same per-thread state for free.
- **Memory-limit checks only cover the walk.** The `--memory-limit` monitor is polled inside the sequential collection loop; the subsequent parallel phases (`scan_jobs` construction, entry building) allocate proportionally to tree size with no checks. If the walk survives the limit, the parallel phases can still exceed it. Extending checks is awkward mid-`par_iter`; the honest alternative is accounting: the peak is predictable from `walker_entries.len()`, so the limit could be enforced by estimate before entering the parallel phases.

---

## What is sound

Credit where due: the `rayon::scope` usage is correct (accumulation guaranteed complete before construction reads the maps); the hard-link dedup uses `DashSet::insert`'s atomicity correctly for first-wins semantics under parallelism; `seen_inodes`/`file_sizes` partitioning between the large-dir tasks and the remaining pass is exact (disjoint and covering, verified); `getpwuid_r` is the re-entrant API and safe to call concurrently; the UID cache guard is released before FFI resolution, so no lock is held across a potentially slow call; and there are no `unsafe` concurrency constructs anywhere — all sharing goes through `Sync` types. The sequential-walk + parallel-stat pipeline, whatever its throughput ceiling, is free of ordering bugs: every cross-phase read happens after the writes complete via structured joins, not ad-hoc synchronization.

## Priority

~~P4 first (mechanical, hot path, zero risk), then P6 and P8's tick removal (one-liners).~~ *P4, P6, and the tick removal are done.* P1 is the structural change that actually moves wall time — treat it as its own project with `scan_benchmark.rs` before/after numbers, and fold P2 into it (the flag's premise dissolves once traversal is parallel). P3 and P5 matter mainly on high-core machines; both have simple thread-local/pre-resolution designs. P7 is a design decision to document rather than a bug.

*Static review; no toolchain in the sandbox, so claims were verified by data-flow tracing (write/read sites per shared structure) rather than profiling. The benchmarks in `benches/` are the right instruments to confirm P1-P3 empirically.*
