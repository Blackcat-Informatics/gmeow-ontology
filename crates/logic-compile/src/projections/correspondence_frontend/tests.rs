// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::CorrespondenceRelation;
use crate::projections::correspondence::{extract_correspondences, project_correspondence};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";

/// Build a representative `dsl/mappings/` fixture: two `gmeow:TermEquivalence` cells (an
/// `exactMatch` and a `closeMatch`, so two distinct relation bands) plus one
/// `gmeow:ProjectionMapping` carrying a single per-profile binding.
fn fixture_dsl() -> std::sync::Arc<gmeow_rdf::RdfDataset> {
    use gmeow_rdf::{RdfDatasetBuilder, RdfLiteral};

    let mut b = RdfDatasetBuilder::new();
    let triple =
        |b: &mut RdfDatasetBuilder, s: &str, p: &str, o_iri: Option<&str>, o_lit: Option<&str>| {
            let s = b.intern_iri(s.to_owned());
            let p = b.intern_iri(p.to_owned());
            let o = match (o_iri, o_lit) {
                (Some(o), _) => b.intern_iri(o.to_owned()),
                (_, Some(l)) => b.intern_literal(RdfLiteral::simple(l.to_owned())),
                _ => unreachable!(),
            };
            b.push_quad(s, p, o, None);
        };

    // ── Two TermEquivalence cells ──────────────────────────────────────────────────
    let eq1 = format!("{GMEOW}eq1");
    triple(
        &mut b,
        &eq1,
        RDF_TYPE,
        Some(&format!("{GMEOW}TermEquivalence")),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}alignSubject"),
        Some(&format!("{GMEOW}Foo")),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}alignPredicate"),
        Some(SKOS_EXACT_MATCH),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}alignObject"),
        Some(&format!("{GMEOW}Bar")),
        None,
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}sssomFile"),
        None,
        Some("demo.sssom.tsv"),
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}confidence"),
        None,
        Some("1.0"),
    );
    triple(
        &mut b,
        &eq1,
        &format!("{GMEOW}justification"),
        Some("https://w3id.org/semapv/vocab/ManualMappingCuration"),
        None,
    );

    let eq2 = format!("{GMEOW}eq2");
    triple(
        &mut b,
        &eq2,
        RDF_TYPE,
        Some(&format!("{GMEOW}TermEquivalence")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}alignSubject"),
        Some(&format!("{GMEOW}Baz")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}alignPredicate"),
        Some(SKOS_CLOSE_MATCH),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}alignObject"),
        Some(&format!("{GMEOW}Qux")),
        None,
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}sssomFile"),
        None,
        Some("demo.sssom.tsv"),
    );
    triple(
        &mut b,
        &eq2,
        &format!("{GMEOW}confidence"),
        None,
        Some("0.8"),
    );

    // ── One ProjectionMapping with one per-profile binding ─────────────────────────
    let pm = format!("{GMEOW}pm1");
    let pat = format!("{GMEOW}pm1/pattern");
    let bind = format!("{GMEOW}pm1/binding/schema-org");
    triple(
        &mut b,
        &pm,
        RDF_TYPE,
        Some(&format!("{GMEOW}ProjectionMapping")),
        None,
    );
    triple(
        &mut b,
        &pm,
        &format!("{GMEOW}hasMappingPattern"),
        Some(&pat),
        None,
    );
    triple(&mut b, &pat, &format!("{GMEOW}anchor"), None, Some("?s"));
    triple(
        &mut b,
        &pm,
        &format!("{GMEOW}hasBinding"),
        Some(&bind),
        None,
    );
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}profile"),
        None,
        Some("schema-org"),
    );
    triple(&mut b, &bind, &format!("{GMEOW}relation"), None, Some("="));
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}toPredicate"),
        Some("https://schema.org/name"),
        None,
    );
    triple(
        &mut b,
        &bind,
        &format!("{GMEOW}confidence"),
        None,
        Some("0.9"),
    );

    b.freeze().expect("freeze fixture dsl")
}

fn parse_nt(nt: &str) -> std::sync::Arc<gmeow_rdf::RdfDataset> {
    gmeow_rdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection N-Triples")
}

/// The transpiler materializes one typed node per cell, and the projected program
/// ROUND-TRIPS through its backing graph: `extract_correspondences` re-derives the exact
/// typed nodes (content identity) AND returns NO errors (a malformed transpile must not
/// pass silently — extract is fail-soft, so an empty error vec is the real assertion).
#[test]
fn transpile_round_trips_with_no_extract_errors() {
    let dsl = fixture_dsl();
    // The frontend accepts an ontology view for symmetry; an empty one suffices here.
    let empty = parse_nt("");
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&empty);

    let program =
        transpile_correspondences(&dsl_view, &onto_view).expect("transpile the fixture cells");

    // Two TermEquivalence cells + one ProjectionMapping binding = three typed nodes.
    assert_eq!(
        program.correspondences.len(),
        3,
        "two term-equivalences and one projection binding materialize three nodes"
    );

    // The relation bands come from the SHARED derivations: exactMatch → Equiv,
    // closeMatch → Overlaps, the `=` binding → Equiv.
    let relations: Vec<CorrespondenceRelation> =
        program.correspondences.iter().map(|c| c.relation).collect();
    assert_eq!(
        relations
            .iter()
            .filter(|r| **r == CorrespondenceRelation::Equiv)
            .count(),
        2,
        "the exactMatch cell and the `=` binding are both Equiv"
    );
    assert!(
        relations.contains(&CorrespondenceRelation::Overlaps),
        "the closeMatch cell is an Overlaps"
    );

    // Project the typed program to its backing graph, then re-extract the bare nodes.
    let nt = project_correspondence(&program);
    let backing = parse_nt(&nt);
    let (extracted, errors) = extract_correspondences(&backing);

    // (b) Extract is fail-soft per-node; a malformed transpile would surface here — the
    // empty error vec is the load-bearing assertion that the transpile is well-formed.
    assert!(
        errors.is_empty(),
        "extract_correspondences must report no malformed nodes: {errors:?}"
    );

    // (a) The typed nodes round-trip by content identity: same count and field-for-field
    // equal (both sides are sorted by IRI — the program ctor and `extract` both order
    // ascending — so a positional compare is a content compare).
    assert_eq!(
        extracted.len(),
        program.correspondences.len(),
        "every materialized node round-trips through the backing graph"
    );
    assert_eq!(
        extracted, program.correspondences,
        "the re-extracted nodes are content-identical to the transpiled set"
    );
}

/// The correspondence IRIs are content-addressed and STABLE across re-runs: transpiling
/// the same corpus twice yields byte-identical node identities (no run-to-run drift).
#[test]
fn correspondence_iris_are_content_stable() {
    let dsl = fixture_dsl();
    let empty = parse_nt("");
    let onto_view = DslView::new(&empty);

    let a = transpile_correspondences(&DslView::new(&dsl), &onto_view).expect("transpile a");
    let b = transpile_correspondences(&DslView::new(&dsl), &onto_view).expect("transpile b");

    let iris_a: Vec<&str> = a.correspondences.iter().map(|c| c.iri.as_str()).collect();
    let iris_b: Vec<&str> = b.correspondences.iter().map(|c| c.iri.as_str()).collect();
    assert_eq!(iris_a, iris_b, "content-addressed IRIs are run-stable");
    // And the whole program keys identically.
    assert_eq!(a.content_key(), b.content_key());
}

/// The justification → evidence-strength band is honest: a manually-curated cell carries
/// a modest non-zero warrant, an un-justified cell leaves the axis unset (never a
/// fabricated number).
#[test]
fn evidence_strength_tracks_the_justification_band() {
    assert_eq!(
        evidence_strength_of_justification(Some(
            "https://w3id.org/semapv/vocab/ManualMappingCuration"
        )),
        Some(0.5)
    );
    assert_eq!(evidence_strength_of_justification(None), None);
    assert_eq!(
        evidence_strength_of_justification(Some("https://example.org/UnknownJustification")),
        None
    );
}
