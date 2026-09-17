// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
    std::fs::write(path, content).unwrap();
}

const PREFIXES: &str =
    "@prefix sh: <http://www.w3.org/ns/shacl#> .\n@prefix ex: <https://example.test/> .\n";

/// The sorted `sh:targetClass` IRIs of a loaded union — the assertable identity
/// of which member files actually joined it.
fn target_classes(shapes: &Shapes) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for shape in &shapes.node_shapes {
        for target in &shape.targets {
            if let purrdf::shapes::shapes::Target::Class(c) = target {
                out.push(c.as_str().to_string());
            }
        }
    }
    out.sort();
    out
}

/// A tiny repo-root fixture: one authored shape + one on-disk generated member.
fn mock_repo(disk_generated: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("shapes/authored-shapes.ttl"),
        &format!("{PREFIXES}ex:AuthoredShape a sh:NodeShape ; sh:targetClass ex:Authored .\n"),
    );
    write(
        &repo.path().join("generated/shapes/validation-shapes.ttl"),
        disk_generated,
    );
    std::fs::create_dir_all(repo.path().join("slices")).unwrap();
    repo
}

/// The regression the stale-disk-fold fix pins: the on-disk generated member
/// carries one (stale) shape, the fresh product byte carries a DIFFERENT one —
/// the loaded union MUST reflect the FRESH bytes, never the disk bytes.
#[test]
fn fresh_bytes_win_over_stale_disk_bytes() {
    let repo = mock_repo(&format!(
        "{PREFIXES}ex:StaleShape a sh:NodeShape ; sh:targetClass ex:Stale .\n"
    ));
    let fresh = BTreeMap::from([(
        "generated/shapes/validation-shapes.ttl".to_string(),
        format!("{PREFIXES}ex:FreshShape a sh:NodeShape ; sh:targetClass ex:Fresh .\n")
            .into_bytes(),
    )]);
    let (_store, shapes) = load_shapes_fresh(repo.path(), &fresh).expect("fresh union loads");
    let classes = target_classes(&shapes);
    assert!(
        classes.contains(&"https://example.test/Fresh".to_string()),
        "the union must carry THIS run's fresh generated shape; got {classes:?}"
    );
    assert!(
        !classes.contains(&"https://example.test/Stale".to_string()),
        "the union must NOT carry the previous run's on-disk bytes (the \
             stale-disk-fold class); got {classes:?}"
    );
    assert!(
        classes.contains(&"https://example.test/Authored".to_string()),
        "the authored disk member still joins the union; got {classes:?}"
    );
}

/// A generated member on disk with NO fresh entry is IGNORED: the fresh product
/// keys are the SOLE authority for the generated section, so a stale on-disk file
/// this run does not produce never joins the union (the fanout prunes it as an
/// orphan). Reading it would be the stale-disk-fold class; and the authored section
/// is enumerated without purrdf's `generated/shapes` fail-closed read, so an empty
/// fresh set on a cold-shaped tree loads cleanly rather than hard-failing.
#[test]
fn stale_disk_generated_member_without_fresh_entry_is_ignored() {
    let repo = mock_repo(&format!(
        "{PREFIXES}ex:StaleShape a sh:NodeShape ; sh:targetClass ex:Stale .\n"
    ));
    let fresh: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let (_store, shapes) = load_shapes_fresh(repo.path(), &fresh)
        .expect("union loads with no fresh generated members");
    let classes = target_classes(&shapes);
    assert!(
        !classes.contains(&"https://example.test/Stale".to_string()),
        "a stale on-disk generated member with no fresh entry must NOT join the union \
             (fresh keys are the sole generated authority); got {classes:?}"
    );
    assert!(
        classes.contains(&"https://example.test/Authored".to_string()),
        "the authored disk member still joins; got {classes:?}"
    );
}

/// A fresh member that does not exist on disk yet (a first run) still joins
/// the union — the case a disk enumeration can never serve.
#[test]
fn fresh_only_member_joins_the_union() {
    let repo = mock_repo(&format!(
        "{PREFIXES}ex:DiskShape a sh:NodeShape ; sh:targetClass ex:Disk .\n"
    ));
    let fresh = BTreeMap::from([
        (
            "generated/shapes/validation-shapes.ttl".to_string(),
            format!("{PREFIXES}ex:DiskShape a sh:NodeShape ; sh:targetClass ex:Disk .\n")
                .into_bytes(),
        ),
        (
            "generated/shapes/constraint-shapes.ttl".to_string(),
            format!("{PREFIXES}ex:FirstRunShape a sh:NodeShape ; sh:targetClass ex:FirstRun .\n")
                .into_bytes(),
        ),
    ]);
    let (_store, shapes) = load_shapes_fresh(repo.path(), &fresh).expect("fresh union loads");
    let classes = target_classes(&shapes);
    assert!(
        classes.contains(&"https://example.test/FirstRun".to_string()),
        "a product-only generated member (absent on disk) must join the union; got {classes:?}"
    );
}

/// A fresh key outside `generated/shapes/` is a misuse — hard-fail.
#[test]
fn non_generated_fresh_key_hard_fails() {
    let repo = mock_repo("# generated\n");
    let fresh = BTreeMap::from([("shapes/authored-shapes.ttl".to_string(), Vec::new())]);
    let err = load_shapes_fresh(repo.path(), &fresh)
        .expect_err("an authored path may never be byte-overridden");
    assert!(format!("{err}").contains("generated/shapes/"));
}
