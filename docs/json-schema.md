# JSON Output Schema

`rudu --format json` writes one JSON document, to stdout or to `--output FILE`.

```bash
rudu /scratch/ab12 --format json --show-atime > scan.json
rudu /scratch/ab12 --format json --output scan.json
```

The document has three blocks:

| Block | Contents |
|---|---|
| `scan_info` | How the scan was run: root, version, time, and every display filter |
| `entries` | The rows that were displayed — exactly the rows, order, and fields of the CSV output |
| `summary` | Totals over the **whole** scan, before display filters |

Like the CSV, `entries` is affected by `--depth`, `--min-size`, `--max-size`,
`--older-than`, `--show-files`, and `--sort`. `summary` is not: it describes
everything that was scanned, so `summary.total_bytes` is the same whatever you
chose to display. Everything outside the document (the banner, cache statistics,
warnings) goes to stderr, so stdout is always valid JSON.

## Example

```json
{
  "scan_info": {
    "schema_version": 1,
    "rudu_version": "1.6.0",
    "root": "/scratch/ab12/cold",
    "generated": "2026-10-03T06:59:39+00:00",
    "generated_unix": 1791010779,
    "show_atime": true,
    "purge_days": 100,
    "filters": {
      "depth": 0,
      "show_files": true,
      "min_size": null,
      "max_size": null,
      "older_than": null,
      "exclude": [".git"]
    }
  },
  "entries": [
    {
      "entry_type": "DIR",
      "size_bytes": 1228800,
      "size_human": "1.23 MB",
      "owner": null,
      "path": "/scratch/ab12/cold",
      "inodes": null,
      "atime": "2026-01-01",
      "atime_unix": 1767243060,
      "age_days": 275,
      "at_risk_bytes": 1228800
    }
  ],
  "summary": {
    "total_bytes": 1228800,
    "files": 3,
    "dirs": 1,
    "cache_hits": 0,
    "cache_total": 0,
    "partial": false,
    "entries_shown": 1,
    "access_age": {
      "buckets": [
        { "label": "<30d", "bytes": 0, "files": 0 },
        { "label": "30-90d", "bytes": 0, "files": 0 },
        { "label": "90-100d", "bytes": 0, "files": 0 },
        { "label": ">=100d", "bytes": 1228800, "files": 3 }
      ],
      "total_bytes": 1228800,
      "at_risk_bytes": 1228800,
      "at_risk_files": 3,
      "oldest_days": 275,
      "unknown_files": 0
    }
  }
}
```

## `scan_info`

| Field | Type | Notes |
|---|---|---|
| `schema_version` | integer | Currently `1`. Incremented on any incompatible change to this document |
| `rudu_version` | string | Version of rudu that wrote it |
| `root` | string | The scanned path, as given on the command line |
| `generated` | string | Reference time for all ages, RFC 3339, UTC |
| `generated_unix` | integer | The same instant in Unix seconds |
| `show_atime` | boolean | Whether the access-age fields are populated |
| `purge_days` | integer \| null | The `--purge-days` threshold; `null` unless `show_atime` |
| `filters.depth` | integer \| null | `--depth` |
| `filters.show_files` | boolean | `--show-files` |
| `filters.min_size` | integer \| null | `--min-size`, in bytes |
| `filters.max_size` | integer \| null | `--max-size`, in bytes |
| `filters.older_than` | integer \| null | `--older-than`, in days |
| `filters.exclude` | string[] | `--exclude` patterns, plus the `--auto-exclude` list when given |

## `entries[]`

Identical to the CSV columns (`rudu --output file.csv`), in the same order; absent values
are `null` rather than empty strings.

| Field | Type | Notes |
|---|---|---|
| `entry_type` | `"DIR"` \| `"FILE"` | Symlinks and special files are `FILE` |
| `size_bytes` | integer | Disk usage (`st_blocks × 512`). For a directory, its whole subtree, hard links counted once |
| `size_human` | string | e.g. `"1.23 MB"` (decimal units) |
| `owner` | string \| null | `null` unless `--show-owner` |
| `path` | string | Full path, starting with `scan_info.root` |
| `inodes` | integer \| null | Directories only; `null` unless `--show-inodes` |
| `atime` | string \| null | `YYYY-MM-DD`, local time; `null` unless `show_atime`. For a directory, its oldest leaf |
| `atime_unix` | integer \| null | Same, in Unix seconds |
| `age_days` | integer \| null | Whole days between `atime_unix` and `generated_unix` |
| `at_risk_bytes` | integer \| null | Directories only, with `show_atime`: bytes beneath it at or past `purge_days` |

## `summary`

| Field | Type | Notes |
|---|---|---|
| `total_bytes` | integer | Disk usage of the scan root — what `du -s` reports |
| `files` | integer | Leaf entries scanned (files, symlinks, special files) |
| `dirs` | integer | Directories scanned, including the root |
| `cache_hits` | integer | Directories restored from cache |
| `cache_total` | integer | Directories checked against the cache |
| `partial` | boolean | `true` if `--memory-limit` stopped the scan early: every total is then a lower bound |
| `entries_shown` | integer | Length of `entries` |
| `access_age` | object \| null | `null` unless `show_atime`; see below |

`access_age` is the same breakdown rudu prints to stderr with `--show-atime`:

| Field | Type | Notes |
|---|---|---|
| `buckets[]` | `{label, bytes, files}` | Age bands derived from `purge_days`; the last is `>=purge_days` |
| `total_bytes` | integer | Bytes in leaves with a readable atime |
| `at_risk_bytes` | integer | Bytes in leaves at or past `purge_days` |
| `at_risk_files` | integer | Leaves at or past `purge_days` |
| `oldest_days` | integer \| null | Age of the single oldest leaf |
| `unknown_files` | integer | Leaves whose atime could not be read (not counted above) |

These count each hard link separately, unlike `total_bytes`.

## Reading it

```bash
# Biggest directories one level down
rudu /data --format json --depth 1 --show-files false \
  | jq -r '.entries[] | [.size_human, .path] | @tsv' | sort -h

# How much is past the purge threshold?
rudu /scratch/ab12 --format json --show-atime --depth 0 \
  | jq '.summary.access_age.at_risk_bytes'
```

```python
import json, subprocess

doc = json.loads(subprocess.run(
    ["rudu", "/data", "--format", "json", "--show-atime"],
    capture_output=True, text=True, check=True).stdout)

if doc["summary"]["partial"]:
    print("warning: scan was cut short; numbers are a lower bound")
files = [e for e in doc["entries"] if e["entry_type"] == "FILE"]
```

## Compatibility

Within `schema_version` 1, fields may be **added** but are never removed,
renamed, or retyped. Ignore fields you don't recognise.
