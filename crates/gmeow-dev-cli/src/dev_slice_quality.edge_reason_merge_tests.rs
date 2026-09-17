// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A scratch tree that removes itself on drop — including while unwinding from a
/// failed assertion, which is when a fixture leaves the most litter.
struct TempDir {
    _tmp: tempfile::TempDir,
    path: std::path::PathBuf,
}
fn temp_dir(label: &str) -> TempDir {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("gmeow-edge-reason-{label}-"))
        .tempdir()
        .expect("create temp dir");
    let path = tmp.path().to_path_buf();
    TempDir { _tmp: tmp, path }
}

/// A `gmeow:CeilingRelocation` declaring `terms` on the `from -> to` edge, dated
/// arbitrarily (the merge under test does not read the date).
fn reloc(
    iri: &str,
    terms: &[&str],
    from: &str,
    to: &str,
) -> gmeow_slice_quality::CeilingRelocation {
    gmeow_slice_quality::CeilingRelocation {
        iri: iri.to_owned(),
        terms: terms.iter().map(|s| (*s).to_owned()).collect(),
        from_slice: from.to_owned(),
        to_slice: to.to_owned(),
        vocabulary: Some("sh".to_owned()),
        date: "2026-01-01".to_owned(),
    }
}

#[test]
fn two_declarations_on_one_edge_both_survive_the_merge() {
    // Two DISTINCT `gmeow:CeilingRelocation` individuals legitimately share the
    // same (from, to, vocab) edge — the gate itself merges witnesses and
    // declaration IRIs per edge. Each declares a DIFFERENT term, and each term's
    // grounding axiom is left behind at the destination (GroundingOrphaned for
    // both). Before the fix, `out.insert` on the second declaration silently
    // DROPPED the first declaration's reason codes; the merge must union them.
    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
    const FROM: &str = "https://blackcatinformatics.ca/gmeow/slices/src";
    const TO: &str = "https://blackcatinformatics.ca/gmeow/slices/dst";
    let prefixes = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix gmeow: <{NS}> .\n\
             @prefix logic: <{LOGIC}> .\n"
    );

    // The base tree's guard is handed to the `BaseSurfaces` below, exactly as production
    // does: the materialized tree is owned by the measurement that reads it, so it
    // dies with the measurement rather than being cleaned up by a second owner.
    let base_tmp = tempfile::Builder::new()
        .prefix("gmeow-edge-reason-base-")
        .tempdir()
        .expect("create temp dir");
    let source_dir = base_tmp.path().join("rel/src");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("module.ttl"),
        format!(
            "{prefixes}\
                 gmeow:S1 a sh:NodeShape ; logic:formalizes logic:s1Axiom .\n\
                 logic:s1Axiom a logic:Formula .\n\
                 gmeow:S2 a sh:NodeShape ; logic:formalizes logic:s2Axiom .\n\
                 logic:s2Axiom a logic:Formula .\n"
        ),
    )
    .unwrap();

    let dest_root = temp_dir("dest");
    std::fs::write(
        dest_root.path.join("module.ttl"),
        format!(
            "{prefixes}\
                 gmeow:S1 a sh:NodeShape ; logic:formalizes logic:s1Axiom .\n\
                 gmeow:S2 a sh:NodeShape ; logic:formalizes logic:s2Axiom .\n"
        ),
    )
    .unwrap();

    let base = BaseMeasurement {
        tree: Some(BaseSurfaces { tmp: base_tmp }),
        dirs: std::collections::BTreeMap::from([(FROM.to_owned(), "rel/src".to_owned())]),
        constructs: std::collections::BTreeMap::new(),
    };
    let declarations = vec![
        reloc("gmeow:reloc1", &[&format!("{NS}S1")], FROM, TO),
        reloc("gmeow:reloc2", &[&format!("{NS}S2")], FROM, TO),
    ];
    let vocab = gmeow_slice_quality::counting::shacl_vocab();
    let slices: Vec<(&Path, String)> = vec![(dest_root.path.as_path(), TO.to_owned())];

    let out = derive_edge_reasons(&base, &declarations, std::slice::from_ref(&vocab), &slices)
        .expect("both fixture surfaces parse");

    let edge = out
        .get(&(FROM.to_owned(), TO.to_owned(), "sh".to_owned()))
        .expect("the shared edge carries reasons");
    assert_eq!(
        edge.get(&format!("{NS}S1"))
            .map(|r| r.iter().copied().collect::<Vec<_>>()),
        Some(vec![
            gmeow_slice_quality::RelocationReason::GroundingOrphaned
        ]),
        "reloc1's reason survives the merge: {edge:?}"
    );
    assert_eq!(
        edge.get(&format!("{NS}S2"))
            .map(|r| r.iter().copied().collect::<Vec<_>>()),
        Some(vec![
            gmeow_slice_quality::RelocationReason::GroundingOrphaned
        ]),
        "reloc2's reason survives the merge, not just the LAST declaration processed: {edge:?}"
    );
}
