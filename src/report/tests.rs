use super::*;

const DAY: u64 = 86_400;
const NOW: u64 = 1_000 * DAY;

fn file(path: &str, size: u64, owner: &str, age_days: Option<u64>) -> FileEntry {
    FileEntry {
        path: PathBuf::from(path),
        size,
        owner: Some(owner.to_string()),
        inodes: None,
        entry_type: EntryType::File,
        atime: age_days.map(|d| NOW - d * DAY),
        at_risk_bytes: None,
    }
}

fn dir(path: &str, size: u64, owner: &str) -> FileEntry {
    FileEntry {
        entry_type: EntryType::Dir,
        inodes: Some(0),
        ..file(path, size, owner, Some(0))
    }
}

fn source(label: &str, path: &str) -> Source {
    Source {
        label: label.to_string(),
        path: PathBuf::from(path),
    }
}

fn owner<'a>(st: &'a Stocktake, name: &str) -> &'a OwnerTotals {
    st.owners().find(|o| o.o == name).expect("owner present")
}

#[test]
fn test_parse_source_label_and_default() {
    assert_eq!(
        parse_source("gb02=/g/data/gb02").unwrap(),
        source("gb02", "/g/data/gb02")
    );
    // A missing directory still gets its last component as the label.
    assert_eq!(
        parse_source("/no/such/dir/if69").unwrap(),
        source("if69", "/no/such/dir/if69")
    );
    // `=` after a `/` is part of the path, not a label separator.
    assert_eq!(
        parse_source("/data/run=3").unwrap(),
        source("run=3", "/data/run=3")
    );
    assert!(parse_source("").is_err());
}

#[test]
fn test_validate_sources_rejects_duplicates_and_missing() {
    let a = tempfile::TempDir::new().unwrap();
    let b = tempfile::TempDir::new().unwrap();
    let pa = a.path().to_str().unwrap();
    let pb = b.path().to_str().unwrap();

    assert!(validate_sources(&[source("x", pa), source("y", pb)]).is_ok());
    assert!(validate_sources(&[source("x", pa), source("x", pb)]).is_err());
    assert!(validate_sources(&[source("x", "/definitely/not/here")]).is_err());
    assert!(validate_sources(&[]).is_err());
}

#[test]
fn test_bytes_and_inodes_attributed_per_owner_and_source() {
    let mut st = Stocktake::new(100, NOW);
    st.add_source(
        &source("p1", "/d/p1"),
        &[
            dir("/d/p1", 700, "root"),
            dir("/d/p1/alice", 600, "alice"),
            file("/d/p1/alice/a", 500, "alice", Some(5)),
            file("/d/p1/alice/b", 100, "bob", Some(200)),
        ],
        false,
    );
    st.add_source(
        &source("p2", "/d/p2"),
        &[dir("/d/p2", 50, "root"), file("/d/p2/c", 50, "alice", None)],
        false,
    );

    let alice = owner(&st, "alice");
    assert_eq!(alice.b.get("p1"), Some(&500));
    assert_eq!(alice.b.get("p2"), Some(&50));
    // One dir plus one file in p1, one file in p2.
    assert_eq!(alice.i.get("p1"), Some(&2));
    assert_eq!(alice.i.get("p2"), Some(&1));
    assert_eq!((alice.files, alice.dirs), (2, 1));
    // Recent file is not stale; an unknown atime is never counted stale.
    assert_eq!(alice.stale, 0);

    let bob = owner(&st, "bob");
    assert_eq!(bob.stale, 100);

    // Directory sizes are subtree aggregates and must not be counted again.
    let root = owner(&st, "root");
    assert!(root.b.is_empty());
    assert_eq!(root.dirs, 2);

    let p1 = &st.sources()[0];
    assert_eq!((p1.bytes, p1.stale, p1.inodes), (600, 100, 4));
    assert_eq!((p1.files, p1.dirs), (2, 2));
}

#[test]
fn test_stale_threshold_is_inclusive() {
    let mut st = Stocktake::new(100, NOW);
    st.add_source(
        &source("p", "/p"),
        &[
            file("/p/edge", 10, "u", Some(100)),
            file("/p/young", 20, "u", Some(99)),
        ],
        false,
    );
    assert_eq!(owner(&st, "u").stale, 10);
}

#[test]
fn test_unresolved_owner_is_grouped() {
    let mut st = Stocktake::new(100, NOW);
    let mut orphan = file("/p/x", 1, "ignored", Some(0));
    orphan.owner = None;
    st.add_source(&source("p", "/p"), &[orphan], false);
    assert_eq!(owner(&st, UNKNOWN_OWNER).files, 1);
}

#[test]
fn test_html_embeds_data_and_escapes_script_breakouts() {
    let mut st = Stocktake::new(30, NOW);
    st.add_source(
        &source("p", "/g/data/p"),
        &[file("/g/data/p/f", 1, "</script><b>", Some(0))],
        true,
    );
    let html = st
        .to_html(&ReportMeta {
            title: "A & <B>".to_string(),
            command: "rudu --report r.html".to_string(),
        })
        .unwrap();

    assert!(!html.contains(DATA_PLACEHOLDER));
    assert!(!html.contains(TITLE_PLACEHOLDER));
    assert!(html.contains("<title>A &amp; &lt;B&gt;</title>"));
    // The only literal `</script>` sequences are the template's own two.
    assert_eq!(html.matches("</script>").count(), 2);

    // The embedded JSON round-trips to what was aggregated.
    let start = html.find(r#"id="data">"#).unwrap() + r#"id="data">"#.len();
    let end = start + html[start..].find("</script>").unwrap();
    let data: serde_json::Value = serde_json::from_str(&html[start..end]).unwrap();
    assert_eq!(data["purge_days"], 30);
    assert_eq!(data["common_parent"], "/g/data");
    assert_eq!(data["sources"][0]["partial"], true);
    assert_eq!(data["owners"][0]["o"], "</script><b>");
}

#[test]
fn test_common_parent_only_when_shared() {
    let t = |paths: &[&str]| {
        let sources: Vec<_> = paths
            .iter()
            .map(|p| SourceTotals {
                path: p.to_string(),
                ..Default::default()
            })
            .collect();
        common_parent(&sources)
    };
    assert_eq!(t(&["/g/data/a", "/g/data/b"]), Some("/g/data".to_string()));
    assert_eq!(t(&["/g/data/a", "/scratch/b"]), None);
    assert_eq!(t(&["relative"]), None);
}
