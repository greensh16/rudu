#!/usr/bin/env python3
"""Performance checks for CI: scan-time regressions and JSON output overhead.

Builds a synthetic tree once, then times rudu commands *interleaved* (A, B, A,
B, ...) so that drift on a shared CI runner hits every variant equally, and
compares medians.

  # JSON vs CSV overhead on one build
  scripts/perf_check.py --head target/release/rudu

  # ...plus a regression check against another build (e.g. the PR's base)
  scripts/perf_check.py --head target/release/rudu --base base/target/release/rudu

Exits non-zero if a check fails. Thresholds are deliberately looser than the
targets they guard: hosted runners are noisy, and a flaky perf gate gets
ignored. A real regression of the kind this exists to catch (an accidental
extra syscall per entry, a quadratic loop) is far larger than the noise.
"""

import argparse
import os
import statistics
import subprocess
import sys
import tempfile
import time


def build_tree(root, dirs, files_per_dir):
    for d in range(dirs):
        sub = os.path.join(root, f"d{d:04}", f"s{d % 7}")
        os.makedirs(sub, exist_ok=True)
        for f in range(files_per_dir):
            with open(os.path.join(sub, f"f{f:04}.dat"), "wb") as fh:
                fh.write(b"x" * (64 + (d * 31 + f * 17) % 4096))


def time_once(cmd, env):
    start = time.perf_counter()
    subprocess.run(cmd, check=True, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.perf_counter() - start


def interleaved(variants, runs, env):
    """variants: {name: argv}. Returns {name: median seconds}."""
    times = {name: [] for name in variants}
    for cmd in variants.values():  # warm-up: page cache, dentry cache
        time_once(cmd, env)
    for _ in range(runs):
        for name, cmd in variants.items():
            times[name].append(time_once(cmd, env))
    return {name: statistics.median(t) for name, t in times.items()}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--head", required=True, help="rudu binary under test")
    ap.add_argument("--base", help="baseline rudu binary for the regression check")
    ap.add_argument("--dirs", type=int, default=400)
    ap.add_argument("--files-per-dir", type=int, default=125)
    ap.add_argument("--runs", type=int, default=9)
    ap.add_argument("--max-regression", type=float, default=0.20,
                    help="fail if head is this much slower than base (default 0.20 = 20%%)")
    ap.add_argument("--max-json-overhead", type=float, default=0.10,
                    help="fail if JSON output is this much slower than CSV (default 0.10; target 0.05)")
    args = ap.parse_args()

    failed = False
    with tempfile.TemporaryDirectory() as work:
        tree = os.path.join(work, "tree")
        build_tree(tree, args.dirs, args.files_per_dir)
        entries = args.dirs * (args.files_per_dir + 2) + 1
        env = dict(os.environ, RUDU_CACHE_DIR=os.path.join(work, "cache"))
        out = lambda name: os.path.join(work, name)
        print(f"tree: ~{entries:,} entries, {args.runs} interleaved runs per variant")

        variants = {
            "csv": [args.head, tree, "--no-cache", "--output", out("o.csv")],
            "json": [args.head, tree, "--no-cache", "--format", "json", "--output", out("o.json")],
        }
        if args.base:
            variants["base csv"] = [args.base, tree, "--no-cache", "--output", out("b.csv")]
        med = interleaved(variants, args.runs, env)
        for name, t in med.items():
            print(f"  {name:<9} {t * 1000:8.1f} ms")

        overhead = med["json"] / med["csv"] - 1
        ok = overhead <= args.max_json_overhead
        failed |= not ok
        print(f"JSON vs CSV: {overhead:+.1%} (limit {args.max_json_overhead:.0%}, target 5%) "
              f"{'ok' if ok else 'FAIL'}")

        if args.base:
            change = med["csv"] / med["base csv"] - 1
            ok = change <= args.max_regression
            failed |= not ok
            print(f"head vs base: {change:+.1%} (limit +{args.max_regression:.0%}) "
                  f"{'ok' if ok else 'FAIL'}")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
