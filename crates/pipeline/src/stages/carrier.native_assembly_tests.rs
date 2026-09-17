// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Count the `owl:AllDisjointClasses` typed subjects (blank nodes) and the
/// `owl:members` list-head triples in a canonical N-Quads blob.
fn disjoint_shape(canon: &str) -> (usize, usize) {
    let all_disjoint = canon
        .lines()
        .filter(|l| {
            l.contains("<http://www.w3.org/2002/07/owl#AllDisjointClasses>")
                && l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>")
        })
        .count();
    let members = canon
        .lines()
        .filter(|l| l.contains("<http://www.w3.org/2002/07/owl#members>"))
        .count();
    (all_disjoint, members)
}

/// Two distinct `owl:AllDisjointClasses` axioms authored in SEPARATE files MUST
/// survive the native `RdfDataset::union` standardize-apart as TWO distinct blank
/// lists — never collapsing into one. This is exactly why the removed
/// `ingest_turtle_scoped` string-prefixed per-file blanks; the union's per-input
/// `BlankScope` is its native replacement. Each file independently mints `_:b0`
/// (the codecs restart blank counters per parse), so without standardize-apart the
/// two axioms would merge into a single subject and one of the lists would vanish.
#[test]
fn two_all_disjoint_lists_survive_union_distinctly() {
    // Two files, each with ONE owl:AllDisjointClasses over a DIFFERENT class set,
    // both anonymous (blank-node subject + blank-node list cells).
    let file_a = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:A ex:B ex:C ) .
"#;
    let file_b = br#"@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex:  <https://example.org/> .
[] a owl:AllDisjointClasses ; owl:members ( ex:D ex:E ) .
"#;

    let union = union_turtle_datasets(&[file_a.to_vec(), file_b.to_vec()])
        .expect("union two disjoint files");
    let nq = dataset_to_nquads(&union).expect("union → n-quads");
    let canon = canonicalize_nq(&nq, "base").expect("canonicalize union");

    let (subjects, members) = disjoint_shape(&canon);
    assert_eq!(
        subjects, 2,
        "the union must keep TWO distinct AllDisjointClasses subjects (one per file); \
             a collapse would leave only 1.\nCanonical:\n{canon}"
    );
    assert_eq!(
        members, 2,
        "each AllDisjointClasses must keep its own owl:members list head"
    );

    // The two list contents (3-element and 2-element) must both be present — a
    // collapse would lose one set entirely. Count rdf:first cells: 3 + 2 = 5.
    let first_cells = canon
        .lines()
        .filter(|l| l.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#first>"))
        .count();
    assert_eq!(
        first_cells, 5,
        "both lists (3 + 2 members) must survive distinctly; got {first_cells} rdf:first cells"
    );

    // Contrast: parsing BOTH files into ONE dataset WITHOUT standardize-apart
    // would let the two `_:b0` subjects collide. We can't easily force that here,
    // but the union path above is the production assembly — its 2-subject result
    // is the proof the native union preserves per-file distinctness.
}

/// The projection-ledger named graph built natively (`turtle_to_nquads`)
/// canonicalizes to a STABLE, idempotent RDFC-1.0 N-Quads form that carries every
/// authored triple (typed literals + the blank-node structural-drop list). This
/// retired the prior oxigraph-`Store` cross-check: the conversion is now fully native
/// (`turtle_to_nquads` → `canonical_flat_nquads`), so the meaningful invariant is
/// canonical idempotence + content fidelity, not equality to a removed oxigraph path.
#[test]
fn projection_ledger_canonicalizes_stably() {
    // A representative projection-report fragment: typed loss-ledger entries with
    // a blank-node structural-drop list (exercises blank canonicalization).
    let report_ttl = br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
<https://blackcatinformatics.ca/gmeow/projection/okf>
    a gmeow:ProjectionLedgerEntry ;
    rdfs:label "OKF projection" ;
    gmeow:preservationKind gmeow:Lossy ;
    gmeow:droppedCount "3"^^xsd:integer ;
    gmeow:structuralDrop [ gmeow:dropKind gmeow:StatementLayer ] .
"#;

    // Native path: the C3 helper, then RDFC-1.0 canonicalize.
    let native = turtle_to_nquads(report_ttl).expect("native turtle → n-quads");
    let native_canon = canonicalize_nq(&native, "projledger").expect("canon native");

    // Idempotence: re-canonicalizing the canonical form is a fixpoint.
    let recanon = canonicalize_nq(native_canon.as_bytes(), "projledger").expect("recanon");
    assert_eq!(
        native_canon, recanon,
        "RDFC-1.0 canonicalization of the projection-ledger N-Quads must be idempotent"
    );

    // Content fidelity: every authored triple survives (typed literal + blank-node
    // structural-drop list), and a canonical blank label (`_:c14n…`) is minted.
    assert!(native_canon.contains("<https://blackcatinformatics.ca/gmeow/projection/okf>"));
    assert!(native_canon.contains("\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
    assert!(native_canon.contains("<https://blackcatinformatics.ca/gmeow/StatementLayer>"));
    assert!(
        native_canon.contains("_:c14n"),
        "the blank structural-drop node must carry a canonical RDFC-1.0 label"
    );
}
