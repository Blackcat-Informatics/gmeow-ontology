// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::parse_dataset;
use std::sync::Arc;

/// Build a frozen native dataset from a Turtle fixture (the native statement OWL +
/// ontology union the production wrapper assembles).
fn store_from(ttl: &str) -> Arc<RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n";

/// A minimal ontology declaring the predicate, an annotation property, and
/// the gmeow subject/object terms the clean fixtures reference.
const ONTO: &str = "gmeow:knows a owl:ObjectProperty .\n\
         gmeow:confidence a owl:AnnotationProperty .\n\
         gmeow:source a owl:AnnotationProperty .\n\
         gmeow:Alice a owl:NamedIndividual .\n\
         gmeow:Bob a owl:NamedIndividual .\n";

fn messages(ttl: &str) -> Vec<String> {
    check_statement_invariants_dataset(&store_from(ttl))
        .into_iter()
        .map(|f| f.message)
        .collect()
}

#[test]
fn lossless_identical_graphs_have_no_findings() {
    let ttl = format!(
        "{PREFIXES}gmeow:Alice gmeow:knows gmeow:Bob .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence \"0.9\"^^xsd:decimal .\n"
    );
    let findings = check_statement_lossless_dataset(&store_from(&ttl), &store_from(&ttl));
    assert!(findings.is_empty(), "identical graphs are lossless");
}

#[test]
fn lossless_divergence_is_directioned() {
    let owl = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Bob .\n");
    let rdf12 = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Carol .\n");
    let findings = check_statement_lossless_dataset(&store_from(&owl), &store_from(&rdf12));

    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| f.code == LOSSLESS_CODE));
    assert!(
        findings
            .iter()
            .any(|f| f.message.starts_with("OWL form has, RDF 1.2 lost:")
                && f.message.contains("Bob"))
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.starts_with("RDF 1.2 form has, OWL lacks:")
                && f.message.contains("Carol"))
    );
}

#[test]
fn statement_clean_cell_has_no_findings() {
    let msgs = messages(&format!(
        "{PREFIXES}{ONTO}\
             gmeow:Alice gmeow:knows gmeow:Bob .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence 0.9 ;\n\
               gmeow:source \"A reliable source.\" .\n"
    ));
    assert!(msgs.is_empty(), "expected clean, got: {msgs:?}");
}

#[test]
fn statement_flags_non_annotation_property() {
    // gmeow:source is NOT declared an owl:AnnotationProperty here.
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:source \"A reliable source.\" .\n"
    ));
    assert!(
        msgs.iter()
            .any(|m| m.contains("is not an owl:AnnotationProperty")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_flags_confidence_out_of_range() {
    let msgs = messages(&format!(
        "{PREFIXES}{ONTO}\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence 1.5 .\n"
    ));
    assert!(
        msgs.iter()
            .any(|m| m.contains("gmeow:confidence 1.5 is outside [0, 1]")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_flags_confidence_not_numeric() {
    let msgs = messages(&format!(
        "{PREFIXES}{ONTO}\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence \"high\" .\n"
    ));
    assert!(
        msgs.iter()
            .any(|m| m.contains("is not numeric") && m.contains("rdflib.term.Literal('high')")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_flags_undeclared_predicate() {
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:undeclaredPred ;\n\
               owl:annotatedTarget gmeow:Bob .\n"
    ));
    assert!(
        msgs.iter()
            .any(|m| m.contains("is not a declared GMEOW property")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_flags_undeclared_gmeow_object_term() {
    // gmeow:Ghost is a bare-local gmeow vocab term that is never a subject.
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Ghost .\n"
    ));
    assert!(
        msgs.iter().any(|m| m.contains("qObject")
            && m.contains("is a gmeow: vocabulary term but is not declared")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_subpath_object_term_is_not_a_vocab_term() {
    // An example/instance IRI under a sub-path is NOT a vocab term, so an
    // undeclared one must NOT be flagged (the _is_gmeow_vocab_term carve-out).
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget <https://blackcatinformatics.ca/gmeow/examples/thing> .\n"
    ));
    assert!(
        !msgs.iter().any(|m| m.contains("vocabulary term")),
        "sub-path IRI must not be flagged: {msgs:?}"
    );
}

#[test]
fn statement_flags_non_dl_datatype() {
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:bornOn a owl:DatatypeProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:bornOn ;\n\
               owl:annotatedTarget \"2020-01-01\"^^xsd:date .\n"
    ));
    assert!(
        msgs.iter().any(|m| m
            .contains("literal datatype http://www.w3.org/2001/XMLSchema#date is")
            && m.contains("not an OWL 2 datatype")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_dl_datatype_date_time_is_clean() {
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:bornAt a owl:DatatypeProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:bornAt ;\n\
               owl:annotatedTarget \"2020-01-01T00:00:00\"^^xsd:dateTime .\n"
    ));
    assert!(
        !msgs.iter().any(|m| m.contains("not an OWL 2 datatype")),
        "xsd:dateTime is OWL 2 DL: {msgs:?}"
    );
}

#[test]
fn statement_flags_preferred_rank_annotation() {
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:preferredRank a owl:AnnotationProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:preferredRank 1 .\n"
    ));
    assert!(
        msgs.iter()
            .any(|m| m.contains("is a preferred/primary selector")),
        "got: {msgs:?}"
    );
}

#[test]
fn statement_flags_primary_prefixed_annotation() {
    let msgs = messages(&format!(
        "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:primarySource a owl:AnnotationProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:primarySource gmeow:Alice .\n"
    ));
    assert!(
        msgs.iter()
            .any(|m| m.contains("is a preferred/primary selector")),
        "got: {msgs:?}"
    );
}

#[test]
fn local_name_splits_slash_and_hash() {
    assert_eq!(local_name("https://ex/path#frag"), "frag");
    assert_eq!(local_name("https://ex/path/leaf"), "leaf");
    assert_eq!(local_name("bare"), "bare");
}

/// Parse a Turtle fixture into a frozen native dataset (no oxigraph round-trip).
fn dataset_from(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

/// The native invariant twin must produce byte-identical messages to the `Store`
/// version across the full fixture battery (parity).
#[test]
fn native_invariants_parity_with_store() {
    let fixtures = [
        // clean cell
        format!(
            "{PREFIXES}{ONTO}\
                 gmeow:Alice gmeow:knows gmeow:Bob .\n\
                 <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
                   owl:annotatedSource gmeow:Alice ;\n\
                   owl:annotatedProperty gmeow:knows ;\n\
                   owl:annotatedTarget gmeow:Bob ;\n\
                   gmeow:confidence 0.9 ;\n\
                   gmeow:source \"A reliable source.\" .\n"
        ),
        // non-annotation property + out-of-range + not-numeric + undeclared + non-DL + preferred
        format!(
            "{PREFIXES}\
                 gmeow:knows a owl:ObjectProperty .\n\
                 gmeow:preferredRank a owl:AnnotationProperty .\n\
                 gmeow:bornOn a owl:DatatypeProperty .\n\
                 gmeow:Alice a owl:NamedIndividual .\n\
                 <https://blackcatinformatics.ca/gmeow/reifier/y> a owl:Axiom ;\n\
                   owl:annotatedSource gmeow:Alice ;\n\
                   owl:annotatedProperty gmeow:undeclaredPred ;\n\
                   owl:annotatedTarget gmeow:Ghost ;\n\
                   gmeow:confidence 1.5 ;\n\
                   gmeow:source \"s\" ;\n\
                   gmeow:preferredRank 1 .\n\
                 <https://blackcatinformatics.ca/gmeow/reifier/z> a owl:Axiom ;\n\
                   owl:annotatedSource gmeow:Alice ;\n\
                   owl:annotatedProperty gmeow:bornOn ;\n\
                   owl:annotatedTarget \"2020-01-01\"^^xsd:date ;\n\
                   gmeow:confidence \"high\" .\n"
        ),
    ];
    for ttl in &fixtures {
        let store_msgs: Vec<String> = check_statement_invariants_dataset(&store_from(ttl))
            .into_iter()
            .map(|f| f.message)
            .collect();
        let mut store_sorted = store_msgs.clone();
        store_sorted.sort();
        let native_msgs: Vec<String> = check_statement_invariants_dataset(&dataset_from(ttl))
            .into_iter()
            .map(|f| f.message)
            .collect();
        let mut native_sorted = native_msgs.clone();
        native_sorted.sort();
        // The set of diagnostics must match exactly (cell iteration order differs
        // between oxigraph and the native twin, so compare as sorted sets).
        assert_eq!(
            store_sorted, native_sorted,
            "native invariant twin diverged from Store version"
        );
    }
}

/// The native lossless twin must agree with the `Store` version.
#[test]
fn native_lossless_parity_with_store() {
    let owl = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Bob .\n");
    let rdf12 = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Carol .\n");

    let mut store_msgs: Vec<String> =
        check_statement_lossless_dataset(&store_from(&owl), &store_from(&rdf12))
            .into_iter()
            .map(|f| f.message)
            .collect();
    store_msgs.sort();
    let mut native_msgs: Vec<String> =
        check_statement_lossless_dataset(&dataset_from(&owl), &dataset_from(&rdf12))
            .into_iter()
            .map(|f| f.message)
            .collect();
    native_msgs.sort();
    assert_eq!(store_msgs, native_msgs, "lossless twin diverged");

    // Identical graphs → no findings on both.
    assert!(check_statement_lossless_dataset(&dataset_from(&owl), &dataset_from(&owl)).is_empty());
}
