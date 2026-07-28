// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One predicate, one domain: every `gmeow:declaredLoss` subject is a
//! `gmeow:LossBearingProfile`.
//!
//! Three independent producers emit `gmeow:declaredLoss`, over three structurally
//! unrelated subject types:
//!
//! * `gmeow_pipeline::stages::distribution_catalog` — `gmeow:DocumentationDistribution`
//!   subjects (and the console's directly-typed `gmeow:LossBearingProfile` surface);
//! * `gmeow_pipeline::stages::docs_format_rendering` — `gmeow:NotationProjectionProfile`
//!   subjects;
//! * `gmeow_music::manifest_turtle` — a bare render-manifest node.
//!
//! A two-class union domain would have left the music manifest out of domain, and
//! `owl:unionOf` is outside EL besides. The resolution is a single EL-safe category the
//! kernel declares, which the other two subject classes subsume. This test is what makes
//! that resolution load-bearing rather than aspirational: it checks BOTH halves — that
//! every emitted subject carries an admissible type, AND that the admissible set really is
//! subsumed by `gmeow:LossBearingProfile` in the committed ontology. Checking only the
//! first would pass against an axiom set that no longer says what it is assumed to say.
//!
//! This test lives in `gmeow-dev-cli` because it is the one crate that can see the
//! pipeline emitters and the music emitter at once; it is the whole point that all three
//! are checked together rather than each against its own local convention.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The classes an emitted `gmeow:declaredLoss` subject may assert. Each is either
/// `gmeow:LossBearingProfile` itself or a declared `rdfs:subClassOf` it — which
/// [`the_admissible_types_are_really_subsumed_by_the_category`] verifies against the
/// committed modules, so this list cannot quietly outlive the axioms that justify it.
const ADMISSIBLE_TYPES: [&str; 3] = [
    "LossBearingProfile",
    "DocumentationDistribution",
    "NotationProjectionProfile",
];

/// Parse N-Triples into `(subject, predicate, object)`, keeping only IRI-object triples —
/// enough for the `rdf:type` and `gmeow:declaredLoss` edges this test reads.
fn iri_triples(ntriples: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for line in ntriples.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('<') else {
            continue;
        };
        let Some((subject, rest)) = rest.split_once("> <") else {
            continue;
        };
        let Some((predicate, rest)) = rest.split_once("> ") else {
            continue;
        };
        let Some(object) = rest.strip_prefix('<').and_then(|o| o.split_once("> .")) else {
            continue;
        };
        out.push((
            subject.to_owned(),
            predicate.to_owned(),
            object.0.to_owned(),
        ));
    }
    out
}

/// Every `gmeow:declaredLoss` subject in an N-Triples graph, with its asserted types.
fn loss_subjects(ntriples: &str) -> BTreeMap<String, BTreeSet<String>> {
    let triples = iri_triples(ntriples);
    let declared_loss = format!("{GMEOW_NS}declaredLoss");
    let subjects: BTreeSet<&str> = triples
        .iter()
        .filter(|(_, p, _)| *p == declared_loss)
        .map(|(s, _, _)| s.as_str())
        .collect();
    subjects
        .into_iter()
        .map(|subject| {
            let types: BTreeSet<String> = triples
                .iter()
                .filter(|(s, p, _)| s == subject && p == RDF_TYPE)
                .map(|(_, _, o)| o.clone())
                .collect();
            (subject.to_owned(), types)
        })
        .collect()
}

fn assert_every_subject_is_loss_bearing(
    label: &str,
    subjects: &BTreeMap<String, BTreeSet<String>>,
) {
    assert!(
        !subjects.is_empty(),
        "{label} emits no gmeow:declaredLoss at all — this gate would pass vacuously"
    );
    let admissible: BTreeSet<String> = ADMISSIBLE_TYPES
        .iter()
        .map(|local| format!("{GMEOW_NS}{local}"))
        .collect();
    for (subject, types) in subjects {
        assert!(
            types.intersection(&admissible).next().is_some(),
            "{label}: {subject} declares a loss but asserts no gmeow:LossBearingProfile type \
             (asserted: {types:?}; admissible: {admissible:?})"
        );
    }
}

/// The distribution catalog: `gmeow:DocumentationDistribution` subjects, the console
/// included — it is a shipped distribution, and `gmeow:DocumentationDistribution` is
/// `logic:subClassOf gmeow:LossBearingProfile`, which is what makes it a legal subject of
/// `gmeow:declaredLoss`.
#[test]
fn the_distribution_catalog_declares_loss_only_on_loss_bearing_subjects() {
    let nt = gmeow_pipeline::stages::distribution_catalog::distribution_catalog_ntriples()
        .expect("the distribution catalog emits");
    let text = String::from_utf8(nt).expect("utf8 n-triples");
    let subjects = loss_subjects(&text);
    assert_every_subject_is_loss_bearing("distribution_catalog", &subjects);

    // The console really is in there — it declares a loss like any other surface — and it
    // reaches the loss-bearing category through its DISTRIBUTION typing rather than by
    // asserting `gmeow:LossBearingProfile` directly. That is the whole point of promoting
    // it: one row shape for every shipped surface, and `gmeow docs matrix` lists it.
    let console = subjects
        .keys()
        .find(|s| s.ends_with("/dist/console"))
        .unwrap_or_else(|| panic!("the console distribution declares no loss: {subjects:?}"));
    let types = &subjects[console];
    assert!(
        types.contains(&format!("{GMEOW_NS}DocumentationDistribution")),
        "the console must be typed as a shipped distribution: {types:?}"
    );
}

/// The docs-format rendering corpus: `gmeow:NotationProjectionProfile` subjects.
#[test]
fn the_docs_format_corpus_declares_loss_only_on_loss_bearing_subjects() {
    let corpus = gmeow_pipeline::stages::docs_format_rendering::build_docs_format_corpus(
        "blake3:00",
        "blake3:01",
        "blake3:02",
    );
    let text = String::from_utf8(corpus.ntriples).expect("utf8 n-triples");
    let subjects = loss_subjects(&text);
    assert_every_subject_is_loss_bearing("docs_format_rendering", &subjects);
    for types in subjects.values() {
        assert!(types.contains(&format!("{GMEOW_NS}NotationProjectionProfile")));
    }
}

/// The music render manifest: a bare node, which therefore has to assert the category
/// itself. This is the producer a two-class union domain would have excluded.
#[test]
fn the_music_manifest_declares_loss_only_on_a_loss_bearing_subject() {
    let manifest =
        gmeow_music::manifest_turtle("musicxml", Some("render provenance")).expect("manifest");
    assert!(
        manifest.contains("gmeow:declaredLoss"),
        "the manifest must declare loss, or this gate is vacuous"
    );
    // The manifest is Turtle over a single blank-node subject, so the type set is the one
    // `a` list at the top of the block.
    let head = manifest
        .split("gmeow:targetNotationSystem")
        .next()
        .expect("manifest head");
    assert!(
        head.contains("gmeow:LossBearingProfile"),
        "the music manifest declares a loss but is not typed gmeow:LossBearingProfile: \
         {manifest}"
    );
}

/// The other half of the claim: the admissible types really ARE subsumed by
/// `gmeow:LossBearingProfile` in the committed ontology, and `gmeow:declaredLoss` really
/// does take that category as its domain. Without this, the type list above would be a
/// convention the axioms no longer back.
///
/// The two subsumptions are authored as `logic:subClassOf`, not `rdfs:subClassOf`: the
/// RDFS surface is a generated projection of `logic:` (Principle 17), so a hand-authored
/// `rdfs:subClassOf` would be ungrounded second-source residue that the projection-ceiling
/// ratchet counts. Both spellings are traversed by the OntoUML discipline checks, so the
/// subsumption is no weaker for being grounded.
#[test]
fn the_admissible_types_are_really_subsumed_by_the_category() {
    let kernel = read("slices/core/kernel/module.ttl");
    let notation = read("slices/core/notation/module.ttl");

    assert!(
        kernel.contains("gmeow:LossBearingProfile\n    a logic:Category ,\n        owl:Class ;"),
        "the kernel must declare gmeow:LossBearingProfile as an EL-safe category"
    );
    assert!(
        kernel.contains(
            "    rdfs:label \"Documentation Distribution\"@x-gmeow-english ;\n    logic:subClassOf gmeow:LossBearingProfile ;"
        ),
        "gmeow:DocumentationDistribution must be declared logic:subClassOf \
         gmeow:LossBearingProfile"
    );
    assert!(
        notation.contains(
            "    rdfs:subClassOf gmeow:Profile ;\n    logic:subClassOf gmeow:LossBearingProfile ;"
        ),
        "gmeow:NotationProjectionProfile must be declared logic:subClassOf \
         gmeow:LossBearingProfile"
    );
    assert!(
        notation.contains(
            "gmeow:declaredLoss\n    a owl:ObjectProperty ;\n    rdfs:domain gmeow:LossBearingProfile ;"
        ),
        "gmeow:declaredLoss must take gmeow:LossBearingProfile as its domain — not a \
         narrower class, and not an owl:unionOf, which is outside EL"
    );
    assert!(
        notation.contains(
            "gmeow:representableParameter\n    a owl:ObjectProperty ;\n    rdfs:domain gmeow:LossBearingProfile ;"
        ),
        "gmeow:representableParameter is gmeow:declaredLoss's exact complement and must \
         share its domain, or the completeness pairing has only one half"
    );
    assert!(
        !notation.contains("owl:unionOf"),
        "the loss domain must never be widened with a union: EL++ has intersection and \
         existential restriction, not union"
    );
    assert!(
        !kernel.contains("owl:unionOf"),
        "the kernel's new distribution vocabulary must stay EL-safe: no owl:unionOf"
    );
}
