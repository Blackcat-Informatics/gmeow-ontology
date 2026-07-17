// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the coat-side distinctiveness gate
//! (`gmeow_slice_quality::coat_guard::slice_coat_collisions`).

use std::path::PathBuf;

use gmeow_slice_quality::coat_guard::slice_coat_collisions;

const SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/coattest";
const NS: &str = "https://blackcatinformatics.ca/gmeow/coattest/";

/// Write a throwaway slice dir with the given `module.ttl` body and return its path.
/// The body is prefixed with the common namespaces and a `manifest.ttl` declaring the
/// slice is written alongside (so `slice_iri_of_dir` resolves the IRI).
fn fixture(name: &str, module_body: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!(
        "gmeow-coatguard-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    std::fs::write(
        dir.join("manifest.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <{SLICE_IRI}> a gmeow:Slice .\n"
        ),
    )
    .unwrap();

    let prefixes = "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix ct: <https://blackcatinformatics.ca/gmeow/coattest/> .\n";
    std::fs::write(dir.join("module.ttl"), format!("{prefixes}\n{module_body}")).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shared_usewhen_skeleton_reds() {
    // Two distinct TBox classes carrying the same useWhen (modulo a swapped CURIE and
    // case/whitespace) collide.
    let dir = fixture(
        "usewhen-collide",
        &format!(
            "ct:Alpha a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 gmeow:useWhen \"Use when validating a ct:Alpha node reached through a cast.\"@en .\n\
             ct:Beta a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 gmeow:useWhen \"Use  when validating a ct:Beta node reached through a cast.\"@en .\n"
        ),
    );
    let hits = slice_coat_collisions(&dir).unwrap();
    cleanup(&dir);
    assert_eq!(hits.len(), 1, "one usewhen collision: {hits:#?}");
    assert!(
        hits[0].contains("useWhen")
            && hits[0].contains(&format!("{NS}Alpha"))
            && hits[0].contains(&format!("{NS}Beta")),
        "names the predicate and both terms: {hits:#?}"
    );
}

#[test]
fn distinct_usewhen_passes() {
    // The same two terms with genuinely term-specific useWhen do not collide.
    let dir = fixture(
        "usewhen-distinct",
        &format!(
            "ct:Alpha a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 gmeow:useWhen \"Use when the alpha channel is premultiplied.\"@en .\n\
             ct:Beta a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 gmeow:useWhen \"Use when the beta decay rate is measured.\"@en .\n"
        ),
    );
    let hits = slice_coat_collisions(&dir).unwrap();
    cleanup(&dir);
    assert!(hits.is_empty(), "distinct usewhen must pass: {hits:#?}");
}

#[test]
fn shared_definition_reds_even_with_load_bearing_curies_kept() {
    // A byte-identical (modulo case/space) definition across two distinct TBox terms —
    // e.g. a class and its property twin — is a near-duplicate. Definitions use the
    // no-strip exact-match, so a CURIE-free duplicate is still caught.
    let dir = fixture(
        "def-collide",
        &format!(
            "ct:HonorificPosition a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 skos:definition \"Whether an honorific is rendered before or after the name.\"@en .\n\
             ct:honorificPosition a owl:FunctionalProperty ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 skos:definition \"Whether an honorific is rendered before or after the name.\"@en .\n"
        ),
    );
    let hits = slice_coat_collisions(&dir).unwrap();
    cleanup(&dir);
    assert_eq!(hits.len(), 1, "one definition collision: {hits:#?}");
    assert!(
        hits[0].contains("skos:definition"),
        "names the predicate: {hits:#?}"
    );
}

#[test]
fn distinct_definition_curies_pass() {
    // Two constraint definitions whose ONLY difference is a load-bearing CURIE stay
    // distinct — the no-strip skeleton keeps the CURIE, so they do not collide.
    let dir = fixture(
        "def-curie-distinct",
        &format!(
            "ct:Aye a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 skos:definition \"A closed-world integrity constraint: a ct:Foo declares a ct:Bar.\"@en .\n\
             ct:Bee a owl:Class ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 skos:definition \"A closed-world integrity constraint: a ct:Baz declares a ct:Qux.\"@en .\n"
        ),
    );
    let hits = slice_coat_collisions(&dir).unwrap();
    cleanup(&dir);
    assert!(
        hits.is_empty(),
        "distinct CURIEs keep definitions distinct: {hits:#?}"
    );
}

#[test]
fn abox_individuals_sharing_a_definition_are_not_checked() {
    // Two A-Box individuals (not owl:Class / owl:*Property) sharing a definition are NOT
    // a distinguishing-coat collision — the guard is TBox-scoped (this is why real A-Box
    // fixtures sharing a fixture definition do not trip it).
    let dir = fixture(
        "abox-shared-def",
        &format!(
            "ct:seg1 a ct:MusicalSegment ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 skos:definition \"A mobile-form fragment in the fixture.\"@en .\n\
             ct:seg2 a ct:MusicalSegment ; rdfs:isDefinedBy <{SLICE_IRI}> ;\n\
                 skos:definition \"A mobile-form fragment in the fixture.\"@en .\n"
        ),
    );
    let hits = slice_coat_collisions(&dir).unwrap();
    cleanup(&dir);
    assert!(
        hits.is_empty(),
        "A-Box individuals are TBox-scoped out: {hits:#?}"
    );
}
