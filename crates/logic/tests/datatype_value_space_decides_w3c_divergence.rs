// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Family 5 acceptance: the native datatype value-space refutation sub-decider
//! DECIDES the committed W3C OWL 2 Full divergence slugs it now covers, matching
//! the W3C published verdict EXACTLY.
//!
//! Each slug's `input.nq` is run through the SAME `dl_consistency` path the
//! grader/runner uses. The native token — `incomplete` when a construct is
//! undecided (a non-empty `gaps`), otherwise the consistency boolean — must equal
//! the W3C ground truth. These cases were `native_verdict = "incomplete"` before
//! Family 5; the subsolver now decides them soundly and completely (an empty
//! `gaps` plus the correct consistency), so the token is the W3C verdict.
//!
//! Cases the subsolver leaves WITHHELD (an unbounded/undecidable facet — e.g. an
//! `xsd:pattern` value space) stay `incomplete` and are deliberately NOT listed
//! here: soundness over coverage.

use purrdf::{
    NativeRdfFormat, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, dataset_from_bytes,
};

/// The committed W3C-divergence slugs Family 5 now DECIDES, with the W3C published
/// verdict each must reproduce.
const DECIDED: &[(&str, &str)] = &[
    ("datatype-datacomplementof-001", "consistent"),
    ("datatype-float-discrete-001", "inconsistent"),
    ("webont-i5-8-001", "inconsistent"),
    ("webont-i5-8-002", "consistent"),
    ("new-feature-rational-001", "consistent"),
    ("new-feature-rational-002", "inconsistent"),
    ("new-feature-rational-003", "consistent"),
];

/// Resolve a slug's `input.nq`, looking in the `w3c-owl2-full-decided` corpus
/// first (the relocated now-decided cases) and falling back to the sibling
/// `w3c-owl2-full-divergence` corpus. The two corpora partition the original
/// W3C-full set, so exactly one holds the slug.
fn case_input(slug: &str) -> String {
    let decided = format!(
        "{}/../../conformance/logic/cases/external/w3c-owl2-full-decided/{slug}/input.nq",
        env!("CARGO_MANIFEST_DIR")
    );
    if std::path::Path::new(&decided).is_file() {
        return decided;
    }
    format!(
        "{}/../../conformance/logic/cases/external/w3c-owl2-full-divergence/{slug}/input.nq",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn native_token(slug: &str) -> String {
    let path = case_input(slug);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .unwrap_or_else(|e| panic!("parse {path}: {e}"));
    let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
        .unwrap_or_else(|e| panic!("dl_consistency on {slug}: {e}"));
    if !verdict.gaps.is_empty() {
        "incomplete".to_owned()
    } else if verdict.consistent {
        "consistent".to_owned()
    } else {
        "inconsistent".to_owned()
    }
}

#[test]
fn family5_decides_the_datatype_value_space_divergence_slugs_matching_w3c() {
    let mut failures = Vec::new();
    for (slug, expected) in DECIDED {
        let token = native_token(slug);
        if token != *expected {
            failures.push(format!(
                "{slug}: native decided {token:?}, W3C published {expected:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "datatype value-space acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}

// ── Negative pins: an honest withhold, and the G1 masking regression ─────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const OWL_WITH_RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_PATTERN: &str = "http://www.w3.org/2001/XMLSchema#pattern";
const XSD_POSITIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#positiveInteger";
const W: &str = "https://gmeow.example/test/datatype-value-space-negative/w";

fn iri_quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

fn lit_quad(s: &str, p: &str, value: &str, dt: &str) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::iri(s),
        p,
        RdfTerm::Literal(RdfLiteral::typed(value, dt)),
    )
    .in_graph(RdfTerm::iri(W))
}

fn build(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for q in quads {
        b.push_owned_quad(&q);
    }
    b.freeze().expect("freeze the synthetic dataset")
}

/// (a) An out-of-fragment datatype case: a `someValuesFrom` obligation constrained
/// by an `xsd:pattern` facet on the filler datatype. `xsd:pattern` is deliberately
/// bounded OUT of the certified fragment (see the module docs on
/// [`gmeow_logic::reason::refute::datatype`]: sound XSD-pattern reasoning needs the
/// XML-Schema regular-expression dialect, not the host regex engine), so the whole
/// case must stay an honest withhold — never a forced `consistent`/`inconsistent`
/// verdict.
#[test]
fn xsd_pattern_restricted_datatype_stays_incomplete() {
    let quads = vec![
        iri_quad("http://ex/p", RDF_TYPE, OWL_DATATYPE_PROPERTY),
        iri_quad("http://ex/R", RDF_TYPE, OWL_RESTRICTION),
        iri_quad("http://ex/R", OWL_ON_PROPERTY, "http://ex/p"),
        iri_quad("http://ex/R", OWL_SOME_VALUES_FROM, "http://ex/D"),
        iri_quad("http://ex/D", OWL_ON_DATATYPE, XSD_STRING),
        iri_quad("http://ex/D", OWL_WITH_RESTRICTIONS, "http://ex/l0"),
        iri_quad("http://ex/l0", RDF_FIRST, "http://ex/f0"),
        iri_quad("http://ex/l0", RDF_REST, RDF_NIL),
        lit_quad("http://ex/f0", XSD_PATTERN, "a.*", XSD_STRING),
        iri_quad("http://ex/y", RDF_TYPE, "http://ex/R"),
    ];
    let edb = build(quads);
    let verdict = gmeow_logic::reason::dl_consistency(edb.as_ref())
        .expect("dl_consistency over the pattern-restricted datatype case");
    assert!(
        !verdict.gaps.is_empty(),
        "an xsd:pattern-restricted value space must stay an honest boundary (withheld), \
         never a forced verdict: {:?}",
        verdict.gaps
    );
}

/// (b) THE G1 REGRESSION TEST — a satisfiable datatype value-space obligation
/// (`y ∈ ∃p.xsd:positiveInteger`, obstruction-free on its own) coexists with an
/// UNRELATED `owl:unionOf` × `owl:disjointWith` case-split inconsistency
/// (`x : Test`, `Test ⊑ (A ⊔ B)`, `Test` disjoint with BOTH `A` and `B`, so every
/// branch of the disjunction closes) the datatype decider never inspects.
///
/// This union+disjoint pattern is DELIBERATELY not a direct-typing complement
/// clash (`i rdf:type N`, `N owl:complementOf M`, `i rdf:type M`) — that shape is
/// decided by the NATIVE forward chase with no case-split at all (see
/// `crate::reason::dl::tests`, `"owl:complementOf is decided natively, not a gap"`),
/// so a mixed case built from it would pass even without this fix and prove
/// nothing. A genuine disjunction case-split is Horn-incomplete: the native forward
/// chase cannot enumerate `x ∈ A ∨ x ∈ B` without branching, so ONLY the case-split
/// sub-decider proves this inconsistent (mirrors `union_disjoint_unsat_is_inconsistent`
/// in `crates/logic/src/reason/refute/casesplit.rs` and the acceptance slug
/// `webont-description-logic-504`).
///
/// Before the whole-case completeness gate this file's G1 fix adds, the datatype
/// decider's per-obligation analysis saw no obstruction of its own and certified
/// `InFragment{Consistent}` — the FIRST certificate in the sub-decider registry
/// order — short-circuiting `refute_with` before the case-split decider (which DOES
/// model `owl:unionOf`/`owl:disjointWith`) ever ran, silently masking the real
/// inconsistency (an UNSOUND `consistent` verdict). After the fix, the foreign
/// `owl:unionOf`/`owl:disjointWith`/`rdfs:subClassOf` predicates are outside the
/// datatype decider's allowlist (crate-private; exercised here only through the
/// public `dl_consistency` surface), so the datatype decider withholds
/// (`OutOfFragment`) and the registry falls through to the case-split decider,
/// which proves the whole case INCONSISTENT.
#[test]
fn satisfiable_datatype_obligation_does_not_mask_a_foreign_case_split_clash() {
    let quads = vec![
        // A satisfiable datatype value-space obligation, obstruction-free in
        // isolation: xsd:positiveInteger is non-empty.
        iri_quad("http://ex/p", RDF_TYPE, OWL_DATATYPE_PROPERTY),
        iri_quad("http://ex/R", RDF_TYPE, OWL_RESTRICTION),
        iri_quad("http://ex/R", OWL_ON_PROPERTY, "http://ex/p"),
        iri_quad("http://ex/R", OWL_SOME_VALUES_FROM, XSD_POSITIVE_INTEGER),
        iri_quad("http://ex/y", RDF_TYPE, "http://ex/R"),
        // An UNRELATED union+disjoint case-split clash: x : Test; Test ⊑ (A ⊔ B);
        // Test disjointWith A; Test disjointWith B ⇒ every branch closes.
        iri_quad("http://ex/x", RDF_TYPE, "http://ex/Test"),
        iri_quad("http://ex/Test", RDFS_SUBCLASSOF, "http://ex/union"),
        iri_quad("http://ex/union", OWL_UNION_OF, "http://ex/l0"),
        iri_quad("http://ex/Test", OWL_DISJOINT_WITH, "http://ex/A"),
        iri_quad("http://ex/Test", OWL_DISJOINT_WITH, "http://ex/B"),
        iri_quad("http://ex/l0", RDF_FIRST, "http://ex/A"),
        iri_quad("http://ex/l0", RDF_REST, "http://ex/l1"),
        iri_quad("http://ex/l1", RDF_FIRST, "http://ex/B"),
        iri_quad("http://ex/l1", RDF_REST, RDF_NIL),
    ];
    let edb = build(quads);
    let verdict = gmeow_logic::reason::dl_consistency(edb.as_ref())
        .expect("dl_consistency over the mixed datatype-obligation + case-split-clash case");
    assert!(
        verdict.gaps.is_empty(),
        "the case-split decider must fully decide this whole case (no honest withhold \
         should remain): {:?}",
        verdict.gaps
    );
    assert!(
        !verdict.consistent,
        "G1 SOUNDNESS: a foreign owl:unionOf/owl:disjointWith case-split clash must NOT be \
         masked by a satisfiable, unrelated datatype value-space obligation — the whole-case \
         completeness gate must force the datatype decider to withhold so the case-split \
         decider proves INCONSISTENT"
    );
}
